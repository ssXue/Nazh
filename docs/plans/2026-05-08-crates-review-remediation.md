# Crates 审阅问题修复跟踪

> **状态：** 草案，来自 2026-05-08 `crates/` 只读审阅
> **范围：** `crates/*`，不包含 `src-tauri/` 与 `web/` 的完整实现审阅
> **审阅方式：** 5 个 subagents 分组审阅 + 主线程交叉复核

**Goal:** 把 `crates/` 审阅中发现的问题转化为可执行修复清单，逐项说明错误场景、可能触发路径、修复方案与验证方式。

**Architecture:** 按子系统边界拆分：DSL 到运行时闭环、连接与 I/O 可靠性、DAG 调度与生命周期、脚本与节点语义、AI/IPC 类型契约、文档与工程治理。P1 优先修复会造成业务语义错误、设备动作未执行、并发资源竞争的问题；P2 修复运行时可靠性、fail-fast 和背压闭环；P3 修复文档、诊断与长期维护风险。

**Tech Stack:** Rust / Tauri / React / TypeScript / ts-rs / Rhai / Tokio / SQLite / cargo test / cargo clippy。

---

## 使用说明

- 本文是修复跟踪文档，不是 ADR。若修复改变运行时语义、节点定位、连接治理策略或 DSL 语义，需要补 ADR 或更新相关 RFC/AGENTS。
- 每个问题先写失败测试或最小复现，再实施修复。
- 运行验证应在 Dev Container 或 CI 等价环境内执行，避免在 macOS host 直接安装项目工具链。
- 修复涉及类型导出时，必须重新生成 `web/src/generated/` 并检查 diff。

## 优先级总览

| 优先级 | 主题 | 主要风险 |
|--------|------|----------|
| P1 | DSL/action 参数、timeout、capabilityCall、连接共享会话、连接替换、switch 路由 | 图能通过校验但语义错误、设备动作未执行、硬件会话竞态、运行时静默丢消息 |
| P2 | DataStore 引用计数、阻塞路径、连接健康、fail-fast、stateMachine step limit、AI/TS 契约 | 长运行泄漏、Tokio worker 阻塞、错误观测失真、配置缺失被默认值掩盖 |
| P3 | 文档漂移、生成物漂移、诊断定位、文件过大 | 后续维护和 AI/人协作判断被误导 |

---

## 1. DSL 到运行时闭环

### CR-P1-01 同一 capability 多次调用时 args 绑定错误

**影响文件：**

- `crates/dsl-compiler/src/output.rs:286`
- `crates/dsl-compiler/src/output.rs:384`

**错误场景：**

同一个 workflow 中多次调用同一个 capability/action target，但每次参数不同。例如状态 `approaching` 调用 `hydraulic_axis.move_to(position: "${approach_position}")`，状态 `returning` 调用 `hydraulic_axis.move_to(position: 0.0)`。编译器生成两个 capabilityCall 节点时，只按 `target_id` 查找 args，后一个节点可能拿到前一个调用的参数。

**可能触发路径：**

1. 用户在 Workflow DSL 的多个 state entry/exit/transition 中引用同一个 capability。
2. `OutputBuilder` 收集 action 时生成 `ActionKey`，但 key 只包含 port/target 信息，没有携带 action instance 的 args。
3. `build_capability_call_nodes()` 调用 `find_action_args(&target_id)`。
4. `find_action_args()` 在 `states.values()` 上找第一个同名 target，返回第一个匹配 args。
5. 生成图结构合法，但运行时执行错误参数。

**修复方案：**

- 引入 `ActionInstance` 或扩展 `ActionKey`，把 args、source state、entry/exit/transition index、port_id 一起保存。
- `collect_actions()` 直接产出带 args 的实例列表，不再靠 `target_id` 回查。
- transition port 对同 from/to 多条 transition 要带 index 或显式 id，避免 port 碰撞。
- 对重复 target + 不同 args 增加回归测试，断言两个生成节点的 `config.args` 不同且稳定。

**验证方式：**

```bash
cargo test -p dsl-compiler
```

Expected: 新增测试能在修复前失败，修复后两个同名 capabilityCall 节点分别保留各自参数。

### CR-P1-02 Workflow DSL 条件表达式作用域与 stateMachine 运行时不一致

**影响文件：**

- `crates/dsl-core/src/workflow.rs:405`
- `crates/dsl-compiler/src/output.rs:215`
- `crates/nodes-flow/src/state_machine.rs:196`

**错误场景：**

DSL 示例和资产可能写 `start_button == true`、`pressure > 34` 这样的裸变量表达式；编译器原样复制到 stateMachine；运行时 Rhai scope 只注入 `payload`，没有把 payload 字段注入为顶层变量，也没有注入 WorkflowVariables。结果是 WorkflowSpec 能编译成图，但真实 dispatch 时 transition 条件求值失败或永远不成立。

**可能触发路径：**

1. 用户或 AI 生成 Workflow DSL，transition `when` 使用裸变量。
2. `dsl-compiler` 将 `when` 原样写入 stateMachine config。
3. `StateMachineNode::evaluate_transitions()` 只向 Rhai scope 注入 `payload`。
4. Rhai 解析裸变量失败，状态机不转移，业务流程卡住。

**修复方案：**

- 明确 DSL 表达式作用域：只允许 `payload.foo`，或允许裸变量但编译器统一重写。
- 推荐先选择一种主路径并 fail-fast：编译期检查裸变量，不满足规则时报可定位错误。
- 如决定支持裸变量，运行时需把 payload object 的顶层字段注入 scope，并定义与 `vars` 冲突时的优先级。
- 更新 RFC/DSL 示例、`dsl-core` 文档和 stateMachine 测试。

**验证方式：**

```bash
cargo test -p dsl-compiler
cargo test -p nodes-flow
```

Expected: 裸变量策略有明确测试；不支持时编译期失败，支持时运行时 transition 正确触发。

### CR-P1-03 timeout/on_timeout 只建模未执行

**影响文件：**

- `crates/dsl-core/src/workflow.rs:170`
- `crates/dsl-compiler/src/output.rs:224`
- `crates/nodes-flow/src/state_machine.rs:258`

**错误场景：**

DSL 声明某个 state 的 timeout 和 on_timeout fallback，看起来有超时保护。编译器也输出 `timeout_rules`。但运行时 stateMachine 的 `transform()` 只评估普通 transitions，不检查状态进入时间、elapsed time，也没有 tick 机制。结果是超时保护永远不会触发。

**可能触发路径：**

1. 设备动作进入 `moving`、`waiting_pressure` 等状态。
2. DSL 中声明 `timeout: 5s`、`on_timeout: fault`。
3. 没有新的 payload 或普通 transition 不成立。
4. stateMachine 不主动检查超时，状态永久停留。

**修复方案：**

- 短期：对非空 timeout 配置 fail-fast，避免给用户“已保护”的错觉。
- 中期：为 stateMachine 增加状态进入时间 `entered_at`，在每次 transform 或定时 tick 中检查 timeout。
- 如果需要无输入也能超时，编译器应生成 timer/tick 边，或运行时为 stateMachine 管理 tick task。
- timeout 触发应产生明确 metadata 和 `ExecutionEvent`，便于观测层显示超时来源。

**验证方式：**

```bash
cargo test -p nodes-flow state_machine
cargo test -p dsl-compiler
```

Expected: timeout 未实现时部署失败；实现后测试能证明超时 transition 在指定时间后触发。

### CR-P1-04 capabilityCall 未真实借连接执行协议动作

> **状态：** 2026-05-09 已修复。`capabilityCall` 现在继承/保存 `connection_id`，Modbus/MQTT/Serial/CAN 分支通过协议 helper 执行动作并写入底层 metadata；缺连接、连接类型不匹配和未接入执行器的 `script` implementation 均 fail-fast。CAN 覆盖 mock 后端成功路径。

**影响文件：**

- `crates/dsl-compiler/src/output.rs:301`
- `crates/nodes-io/src/capability_call.rs:58`
- `crates/nodes-io/src/capability_call.rs:144`

**错误场景：**

Workflow DSL 编译出 `capabilityCall` 节点，图结构和 capability snapshot 都存在，但节点运行时只是解析模板并输出动作意图 payload/metadata，没有通过 `ConnectionManager` 执行 Modbus/MQTT/Serial/CAN 等协议动作。业务层以为设备已经执行，现场设备实际无动作。

**可能触发路径：**

1. 用户用 Device/Capability DSL 生成设备动作工作流。
2. `dsl-compiler` 生成 `capabilityCall` 节点，并写入顶层 `connection_id`。
3. `CapabilityCallConfig` 不保存 `connection_id`，节点内 `connection_manager` 当前未使用。
4. `transform()` 返回 intent payload，未借连接，未发送协议请求。

**修复方案：**

- 让 `capabilityCall` 显式继承 `WorkflowNodeDefinition::connection_id()`，或把连接 ID 纳入 `CapabilityCallConfig`。
- 为 `CapabilityImplSnapshot` 的每种协议实现执行 helper，复用低层节点的协议操作但不复用节点生命周期。
- 无连接、连接类型不匹配、模板参数缺失必须 fail-fast。
- 成功/失败都要通过 `ConnectionGuard::mark_success/mark_failure` 更新连接健康。
- metadata 同时保留设备语义和底层协议细节。

**验证方式：**

```bash
cargo test -p nodes-io capability_call
cargo test -p dsl-compiler
```

Expected: 无连接失败、mock 后端成功、模板参数解析成功、协议错误会标记连接失败。

### CR-P1-05 系统 action 生成的 code 节点配置无效

**影响文件：**

- `crates/dsl-compiler/src/output.rs:306`
- `crates/nodes-flow/src/code_node.rs:40`

**错误场景：**

DSL 中使用 `action: alarm.raise` 等非 capability action 时，编译器生成 `type: "code"` 节点，但 config 里放的是 `capability_id/device_id/implementation/args`，没有 `code` 节点必需的 `script` 字段。图 JSON 可能通过部分结构测试，但部署创建节点时会因配置无法反序列化失败。

**可能触发路径：**

1. Workflow DSL transition 或 state entry 使用 system action。
2. `build_capability_call_nodes()` 判断不是 capability，进入 code node 分支。
3. 输出 code node config 缺少 `script`。
4. `FlowPlugin` 创建 `CodeNodeConfig` 时失败。

**修复方案：**

- 短期：对未实现的 system action fail-fast，不生成伪 code node。
- 中期：为 system action 定义明确节点类型，如 `systemAction`，或定义可生成的 script 模板。
- 测试必须覆盖 WorkflowGraph deploy/create node，而不只检查 JSON 形状。

**验证方式：**

```bash
cargo test -p dsl-compiler
cargo test -p graph
```

Expected: 未支持 action 在编译期报错；支持后生成节点能被 registry 创建。

### CR-P2-01 dsl-core 解析与 validate 不闭合

**影响文件：**

- `crates/dsl-core/src/parser.rs:25`
- `crates/dsl-core/src/capability.rs:103`

**错误场景：**

YAML parser 只做 serde 反序列化，不强制调用 `CapabilitySpec::validate()`；同时 `validate()` 的实现也没有覆盖注释承诺的 required input range、模板变量存在性、重复 ID、fallback 引用存在性等规则。无效资产可以保存并进入编译链路，直到运行时才暴露为模板残留或执行失败。

**可能触发路径：**

1. 用户保存 capability YAML，缺少 required input range 或引用不存在的 fallback。
2. parser 成功返回 `CapabilitySpec`。
3. 编译器使用该 capability 生成节点。
4. 运行时模板替换后仍残留 `${...}`，或 fallback 路由不可达。

**修复方案：**

- 提供 `parse_*_yaml_validated()`，并让 Tauri asset 保存入口使用 validated path。
- 补齐 `CapabilitySpec::validate()`：required/range、模板变量、重复 ID、fallback 引用、空实现。
- 错误中包含 capability id、字段路径和建议修复。

**验证方式：**

```bash
cargo test -p dsl-core
```

Expected: 无效 capability YAML 在解析/保存入口失败，错误定位到字段。

### CR-P2-02 Device 信号生成 capability 时丢失编码语义

**影响文件：**

- `crates/dsl-core/src/capability.rs:170`
- `crates/dsl-core/src/device.rs:82`

**错误场景：**

从 Device 信号自动生成 Capability 时，CAN 只保留 `can_id/is_extended/data`，丢失 `byte_offset/byte_length/data_type/byte_order`；Modbus 也没有完整保留 scale/data_type 等写入语义。生成结果看似可执行，但不足以编码真实设备帧或寄存器值。

**可能触发路径：**

1. AI 或用户从 DeviceSpec 自动生成 capability。
2. 原始 signal source 含编码/解码字段。
3. 转换为 `CapabilityImpl` 时只保留协议片段。
4. `capabilityCall` 真正执行后无法正确编码设备数据。

**修复方案：**

- Capability 生成应引用完整 `SignalSource`，或生成结构化 encoder 配置。
- 无法无损生成时返回诊断，不生成看似可执行的能力。
- 补 CAN/Modbus 编码字段 round-trip 测试。

**验证方式：**

```bash
cargo test -p dsl-core capability
```

Expected: 自动生成 capability 保留必要编码字段，或明确拒绝无损转换失败场景。

### CR-P2-03 dsl-compiler safety 检查过粗且诊断丢失

**影响文件：**

- `crates/dsl-compiler/src/safety.rs:586`
- `crates/dsl-compiler/src/safety.rs:598`
- `crates/dsl-compiler/src/output.rs:60`

**错误场景：**

Safety 检查把任意状态环都当错误，但状态机返回 idle 是正常业务模式；可达性分析只是把所有 transition.to 标为 reachable，没有从 initial 做真实图遍历。`compile_with_safety()` 失败时又只返回“共 N 个错误”，丢掉具体 `SafetyDiagnostic`。

**可能触发路径：**

1. 用户定义正常循环状态机，安全检查误报。
2. 用户定义从 initial 不可达的状态，但该状态继续连出 transition，检查漏报。
3. 前端或 AI 得到泛化错误，无法定位到 state/action/capability。

**修复方案：**

- 从 initial 做 DFS/BFS 可达分析。
- 循环只对无外部触发/无等待条件的自激路径报错，其余降级 warning 或要求显式注解。
- 增加 `CompileError::Safety { report }` 或让失败时返回完整 report。

**验证方式：**

```bash
cargo test -p dsl-compiler safety
```

Expected: 正常循环不误报，不可达状态可定位，compile 错误保留诊断详情。

### CR-P2-04 编译上下文与 sanitize 可能静默覆盖 ID

**影响文件：**

- `crates/dsl-compiler/src/context.rs:27`
- `crates/dsl-compiler/src/output.rs:486`

**错误场景：**

多个 device/capability 使用相同 id 时，`HashMap` 构建上下文会静默覆盖。`sanitize` 还可能把不同原始 id 映射为同一节点 id，例如 `a.b` 和 `a_b`。最终图边指向被合并后的节点，审计困难。

**可能触发路径：**

1. 资产库里出现重复 device/capability id，或 id sanitize 后碰撞。
2. 编译上下文构建成功。
3. 输出 graph 中某些节点/边被覆盖或合并。
4. 运行时执行错误设备或错误能力。

**修复方案：**

- `CompilerContext::new` 改为返回 `Result`，拒绝重复 id。
- 编译期维护 sanitize 前后映射，发现碰撞直接报错。
- 错误中列出冲突的原始 id。

**验证方式：**

```bash
cargo test -p dsl-compiler duplicate
```

Expected: 重复 id 和 sanitize 碰撞在编译期失败。

---

## 2. 连接与 I/O 可靠性

### CR-P1-06 共享会话首次并发初始化竞态

> **状态：** 2026-05-08 已修复。`ConnectionManager::ensure_shared_session` 增加 per-key 初始化锁，新增并发初始化回归测试。

**影响文件：**

- `crates/connections/src/lib.rs:896`
- `crates/connections/src/lib.rs:909`
- `crates/nodes-io/src/ethercat/backends/ethercrab_backend.rs:28`

**错误场景：**

多个节点首次同时使用同一个 CAN/EtherCAT connection，`ensure_shared_session` cache miss 后并发执行 factory。可能同时初始化两套硬件会话；EtherCAT 还涉及进程级 `PduStorage` 单例，失败方可能报错，成功方已建好会话。

**可能触发路径：**

1. workflow 部署后多个节点几乎同时触发同一 connection。
2. 每个调用都读到 shared session cache miss。
3. 多个 async factory 同时打开硬件或创建 backend。
4. 后写入 cache 的一个成功，其他失败或造成外设状态不一致。

**修复方案：**

- 按 `connection_id` 或 session key 增加 per-key 初始化锁。
- 可用 `OnceCell`/初始化状态机表示 initializing/ready/failed。
- factory 失败后重查 cache，避免“已有成功会话但并发失败仍返回错误”。
- 初始化失败要清理半开资源并记录连接失败。

**验证方式：**

```bash
cargo test -p connections shared_session
```

Expected: 并发 N 次初始化时 factory 只执行一次，失败/成功竞态有确定行为。

### CR-P1-07 连接定义替换破坏 in-use 排他语义

> **状态：** 2026-05-08 已修复。运行中 record 或共享会话存在时不再静默替换连接定义，新增 `upsert` / `replace` 回归测试。

**影响文件：**

- `crates/connections/src/lib.rs:372`
- `crates/connections/src/lib.rs:408`
- `crates/connections/src/lib.rs:441`

**错误场景：**

连接正在被 guard 借出时，`upsert_connection` 或 `replace_connections` 直接替换 `Arc<Mutex<ConnectionRecord>>`。旧 guard Drop 时更新旧 record，新 record 已可再次借出，导致同一硬件连接出现两份治理状态；shared session 也可能继续复用旧 metadata。

**可能触发路径：**

1. 运行中节点持有 connection guard。
2. 用户保存新的连接定义或重新部署 workflow。
3. `ConnectionManager` 替换 map 中的 record。
4. 新请求借到新 record，旧 guard 仍代表旧 record。
5. 连接排他、健康统计和 shared session 失真。

**修复方案：**

- 更新连接定义时保留 record 身份，内部增加 generation/config hash。
- 当 `in_use > 0` 或存在 shared session 时拒绝替换、延迟替换，或进入 draining 状态。
- 配置变更应触发 shared session shutdown，再切换新配置。
- 文档明确运行中连接变更策略。

**验证方式：**

```bash
cargo test -p connections upsert
```

Expected: in-use 连接无法被静默替换；替换后的 guard/health/session 行为一致。

### CR-P2-05 连接失败统计与熔断不闭合

> **状态：** 2026-05-08 已修复。`mark_failure()` Drop 出口推进失败计数与熔断；同一 lease 已手动 `record_connect_failure` 时不重复计数。

**影响文件：**

- `crates/connections/AGENTS.md:40`
- `crates/connections/src/lib.rs:996`
- `crates/connections/src/lib.rs:1147`

**错误场景：**

文档说失败路径由 Drop 自动计入失败统计，但 `ConnectionOutcome::Failure` 主要更新诊断，不增加 `total_failures/consecutive_failures`，也不会进入退避或熔断。大量协议失败会显示 Degraded，却不会触发治理状态机。

**可能触发路径：**

1. 节点借出 guard。
2. 协议调用失败后调用 `guard.mark_failure()`。
3. guard Drop 执行 failure outcome。
4. 连接诊断更新，但 failure counters/circuit breaker 不变。
5. 调度继续高频尝试失败连接。

**修复方案：**

- 明确区分业务失败和连接失败。
- 连接失败路径复用 `apply_runtime_failure`，或至少更新 failure counters 与 retry window。
- 节点协议错误分支统一标记具体失败原因。

**验证方式：**

```bash
cargo test -p connections failure
cargo test -p nodes-io
```

Expected: mark_failure 能推进 failure counters，达到阈值后进入预期治理状态。

### CR-P2-06 未知连接类型默认通过验证

**影响文件：**

- `crates/connections/src/lib.rs:1189`
- `crates/connections/src/lib.rs:1363`

**错误场景：**

连接定义 `type` 拼错或使用未支持类型时，validator 的 `_ => {}` 分支直接通过。连接以 Idle/可借出状态进入运行时，直到节点实际使用时才失败。

**可能触发路径：**

1. 用户配置 `type = "modbux"` 或 AI 生成未知连接类型。
2. `validate_connection_definition()` 通过。
3. workflow 部署成功。
4. 节点运行时找不到正确 backend 或使用错误逻辑。

**修复方案：**

- 未知 type 默认拒绝。
- 如确需 opaque/tool 连接，显式列入 allowlist，并限制可用节点。
- 错误列出支持的连接类型。

**验证方式：**

```bash
cargo test -p connections validate_connection_definition
```

Expected: 未知连接类型在保存/部署前失败。

### CR-P1-08 CAN mock 文档与实现不一致

> **状态：** 2026-05-08 已修复。无 `connection_id` 的 `canRead` / `canWrite` 使用内部 `mock-can` key 创建隐式 Mock 会话，不要求注册连接资源。

**影响文件：**

- `crates/nodes-io/AGENTS.md:21`
- `crates/nodes-io/src/can/can_read.rs:108`
- `crates/nodes-io/src/can/can_write.rs:140`
- `crates/nodes-io/src/can/session.rs:152`

**错误场景：**

文档声明 `canRead/canWrite` 无连接时走 Mock 回退；实现把空 `connection_id` 替换为 `"mock-can"` 后仍调用 `ConnectionManager::get`。如果没有注册 `mock-can` 连接，首帧直接失败。

**可能触发路径：**

1. 本地开发或测试未配置 CAN connection。
2. 节点 config 不填 `connection_id`。
3. 代码使用 `"mock-can"` 查 ConnectionManager。
4. manager 中无该连接，节点失败。

**修复方案：**

- 二选一：
  - 无连接时直接构造 mock session，不查 manager。
  - 或改为必须显式配置 mock connection，并同步 AGENTS、示例和错误消息。
- 更符合 fail-fast 的方向是显式 `simulation: true` + mock connection。

**验证方式：**

```bash
cargo test -p nodes-io can
```

Expected: 无连接策略和文档一致，未配置 mock 时行为明确。

### CR-P2-07 CAN/SLCAN 并发与阻塞模型不稳

**影响文件：**

- `crates/nodes-io/src/can/backends/slcan.rs:50`
- `crates/nodes-io/src/can/backends/slcan.rs:163`
- `crates/nodes-io/src/can/backends/slcan.rs:192`
- `crates/nodes-io/src/can/session.rs:31`
- `crates/nodes-io/src/can/can_read.rs:122`

**错误场景：**

SLCAN backend 在 async 函数内直接执行同步串口 open/write/flush，会阻塞 Tokio worker。CAN shared session 用 `try_lock`，正常读写竞争会变成配置错误；`canRead` 还可能持锁等待 `recv(timeout).await`，阻塞同连接其他操作。

**可能触发路径：**

1. 同一 CAN connection 上同时有 `canRead` 和 `canWrite`。
2. `canRead` 持有 bus 锁等待超时。
3. `canWrite` 调用 `try_lock` 失败，返回 NodeConfig 错误。
4. SLCAN 同步 I/O 进一步占住 async worker。

**修复方案：**

- SLCAN open/init/send/shutdown 放入 `spawn_blocking`，或改成专用线程/actor。
- CAN bus 使用 async 排队锁，或拆成 bus actor，通过 channel 串行化读写请求。
- 锁竞争应表现为等待、背压或超时，而不是配置错误。

**验证方式：**

```bash
cargo test -p nodes-io can
```

Expected: 并发 read/write 不随机报配置错误；阻塞 I/O 不占 Tokio worker。

### CR-P2-08 EtherCAT/PDO 错误不清理会话与连接健康

**影响文件：**

- `crates/nodes-io/src/ethercat/pdo_read.rs:85`
- `crates/nodes-io/src/ethercat/pdo_write.rs:124`
- `crates/nodes-io/AGENTS.md:113`

**错误场景：**

EtherCAT PDO 读写失败被转换成 `NodeConfig` 错误，但不记录连接失败，也不清理 shared session。连接健康可能仍显示 Healthy，旧 master session 被继续复用。

**可能触发路径：**

1. EtherCAT runtime 初始化成功。
2. 后续 PDO read/write 因链路、从站或状态错误失败。
3. 节点返回错误，但没有 `record_connect_failure` 或 `runtime.shutdown()`。
4. 下一次调用继续复用失效 session。

**修复方案：**

- 区分可恢复 PDO 错误和会话失效错误。
- 会话失效时调用连接失败记录并 shutdown/remove shared session。
- 错误类型从 `NodeConfig` 调整为更准确的 runtime/device error。

**验证方式：**

```bash
cargo test -p nodes-io ethercat
```

Expected: 失效会话不会继续复用，连接健康状态反映运行错误。

### CR-P2-09 运行时配置默认值掩盖现场差异

**影响文件：**

- `crates/dsl-core/src/capability.rs:46`
- `crates/dsl-core/src/device.rs:80`
- `crates/nodes-io/src/sql_writer.rs:15`
- `crates/nodes-io/src/can/mod.rs:123`
- `crates/nodes-io/src/ethercat/mod.rs:103`
- `crates/nodes-io/src/modbus_read.rs:372`

**错误场景：**

安全、协议、部署和现场相关配置缺失时被默认值补齐。例如 SQL 默认写 `./nazh-local.sqlite3`，CAN 默认 interface/bitrate，EtherCAT 默认 backend/cycle timeout，Modbus 无连接走模拟。工业现场可能写错数据库、跑错总线参数，或把缺配置误认为正常模拟。

**可能触发路径：**

1. 用户或 AI 生成节点/连接配置，漏填关键运行时字段。
2. serde default 或 normalize 填入默认值。
3. workflow 部署成功。
4. 现场行为与预期配置不一致，且错误不易定位。

**修复方案：**

- 运行态字段改为必填：连接 ID、总线参数、安全审批、协议编码等。
- 默认值只放 UI template、测试 fixture 或显式 demo profile。
- 对确需兼容旧资产的字段，在迁移层补值并产生诊断。
- 增加生产运行策略：`DEVICE_IO` 节点无显式连接或 simulation 标志时拒绝部署。

**验证方式：**

```bash
cargo test --workspace explicit_config
```

Expected: 关键运行时配置缺失在部署/保存前失败，demo/test 场景通过显式 profile 开启模拟。

### CR-P2-10 store 同步 SQLite API 容易阻塞 async 调用方

**影响文件：**

- `crates/store/src/lib.rs:20`
- `crates/store/src/variables.rs:31`
- `crates/store/src/history.rs:42`
- `crates/store/src/global_variables.rs:95`

**错误场景：**

`Store` 用 `std::sync::Mutex<rusqlite::Connection>`，公开 CRUD 是同步 sqlite 调用。Tauri 或 runtime async 调用方直接调用时会阻塞 worker；变量、历史、全局变量共用一把 DB mutex，会放大长事务或慢磁盘影响。

**可能触发路径：**

1. 运行时高频写变量历史或 observability。
2. async command 直接调用 store 同步方法。
3. SQLite I/O 或 mutex 等待阻塞 Tokio/Tauri worker。
4. UI/运行时延迟抖动。

**修复方案：**

- 在 `store` crate 提供 async `StoreHandle`/worker actor，统一串行执行 SQLite。
- 或提供明确 `spawn_blocking` 包装 API，禁止调用方直接在 async context 里执行同步 CRUD。
- 增加文档说明 store 调用边界和背压策略。

**验证方式：**

```bash
cargo test -p store
```

Expected: API 使用模式明确；慢查询不会直接阻塞 async worker。

### CR-P3-01 store migration/初始化错误被吞或 panic

**影响文件：**

- `crates/store/src/migrations.rs:51`
- `crates/store/src/lib.rs:53`

**错误场景：**

Migration 检查把任何 `schema_version` 查询错误都当作未应用；`open_unpersisted()` 生产可调用路径用 `expect` 处理初始化和 migration。数据库损坏、权限问题或 migration SQL 错误会被误导或直接 panic。

**可能触发路径：**

1. SQLite 文件结构异常或迁移表损坏。
2. `schema_version` 查询失败。
3. 代码当作 migration 未应用继续执行。
4. 后续报出更难理解的错误，或 `expect` 直接 panic。

**修复方案：**

- 只对“表不存在”走 bootstrap，其它 rusqlite 错误直接返回。
- migration 放事务里执行。
- `open_unpersisted()` 改为 `Result<Self, StoreError>`。

**验证方式：**

```bash
cargo test -p store migration
```

Expected: 损坏 schema 返回明确错误，不 panic、不误判。

---

## 3. DAG 调度、生命周期与背压

### CR-P2-11 DataStore 写入后发送失败会泄漏引用

> **状态：** 2026-05-08 部分修复。`NodeHandle::emit`、`WorkflowIngress::submit_to` / `blocking_submit_to`、runner downstream 发送失败均补偿释放 DataStore 引用；多根 `submit` 明确为 partial accepted 语义。

**影响文件：**

- `crates/core/src/lifecycle.rs:116`
- `crates/core/src/lifecycle.rs:132`
- `crates/graph/src/runner.rs:281`
- `crates/graph/src/runner.rs:305`
- `crates/graph/src/types.rs:183`
- `crates/graph/src/types.rs:197`

**错误场景：**

`NodeHandle::emit`、DAG runner 和 `WorkflowIngress::submit` 都有“先按目标数写入 DataStore，再逐个发送 channel”的路径。一旦某个 downstream/root channel 已关闭，发送失败后没有对失败或未发送目标补 `release`。长运行 workflow 在 shutdown、节点退出或下游失败时会积累不可释放 payload。

**可能触发路径：**

1. 上游节点产出 payload。
2. DataStore 按 N 个消费者写入引用计数。
3. 第 K 个 downstream 发送失败。
4. 代码记录错误或返回，但没有释放对应引用。
5. DataStore entry 永久保留。

**修复方案：**

- 推荐把 consumer count 绑定到成功发送数：先发送可克隆的 `ContextRef` 计划，或写入后对失败发送逐个补偿 release。
- 定义 partial submit 语义：all-or-nothing 或 partial accepted，并在错误返回中说明已发送目标。
- graph runner 出错 break 前清理本 trace 的 DataStore 引用与 PureMemo。
- 增加 channel closed/backpressure 回归测试。

**验证方式：**

```bash
cargo test -p graph channel_closed
cargo test -p core lifecycle
```

Expected: 下游关闭时 DataStore 引用计数闭合，测试能观测 entry 被释放。

### CR-P2-12 Data-only 输出被当成 workflow result

> **状态：** 2026-05-08 已修复。runner 对纯 Data 输出只写 `OutputCache`，不再写入 result stream 或创建无人消费的 DataStore entry。

**影响文件：**

- `crates/graph/src/runner.rs:7`
- `crates/graph/src/runner.rs:281`
- `crates/graph/src/runner.rs:297`

**错误场景：**

文件头说明 Data pin 只写 `OutputCache`，不推进 `ContextRef`；但当节点输出只有 Data 下游时，`matching_targets` 为空，代码仍写入 DataStore 并发送到 `result_tx`。Data 平面输出被当成 workflow result，污染执行结果。

**可能触发路径：**

1. 节点只连接 Data pin 给下游 pull collector。
2. 没有 Exec/Reactive matching target。
3. runner 认为这是“叶子输出”，发送 workflow result。
4. UI/observability 看到不该出现的 result。

**修复方案：**

- 明确区分“真正叶子 Exec 输出”和“纯 Data 输出”。
- 纯 Data 输出只更新 `OutputCache`，不写 result_tx。
- 增加 Data-only graph 测试，断言 result stream 不产生业务结果。

**验证方式：**

```bash
cargo test -p graph data_pin
```

Expected: Data-only 输出只影响 cache，不产生 workflow result。

### CR-P3-02 拓扑排序对 Data/Reactive 边的语义与文档不一致

**影响文件：**

- `crates/graph/src/topology/mod.rs:72`
- `crates/graph/src/topology/classify.rs:13`
- `crates/graph/src/deploy.rs:141`

**错误场景：**

分类文档说 Data/Reactive edges 不参与主拓扑；实际 deploy 先跑全边 topology，再做 edge 分类。Data/Reactive 关系可能影响 root 识别和部署顺序。

**可能触发路径：**

1. workflow 有 Data/Reactive 边但无 Exec 边。
2. topology 把这些边纳入入度计算。
3. root/downstream 与文档模型不一致。
4. 后续调度或部署错误难以判断是文档错还是实现错。

**修复方案：**

- 二选一：
  - topology 只纳入 Exec/触发边。
  - 或更新文档，说明当前是保守全边拓扑。
- 增加 pin kind 混合图测试，固定 root 识别语义。

**验证方式：**

```bash
cargo test -p graph topology
```

Expected: Data/Reactive 边参与或不参与拓扑的规则被测试固定。

### CR-P2-13 变量事件背压丢失只打 debug

> **状态：** 2026-05-09 已修复。Changed/Deleted 变量事件统一走 `EventSink::emit()`，channel full/closed 时以 `tracing::error!` 记录 workflow、变量名、事件类型、累计丢弃数和 dropped event；新增 full/closed 回归测试。

**影响文件：**

- `crates/core/src/variables.rs:58`
- `crates/core/src/variables.rs:303`
- `crates/core/src/variables.rs:455`

**错误场景：**

变量变更或删除事件发送失败时只打 `debug!`，但文档要求 channel full/closed 记录错误日志。UI 依赖 `workflow://variable-changed` 更新状态时，变量表已变更但事件丢失，界面状态与真实 runtime state 不一致。

**可能触发路径：**

1. 变量高频变更，事件 channel 满。
2. 或接收端已关闭。
3. `try_send` 失败。
4. 只打 debug，调用方和观测层不知道丢事件。

**修复方案：**

- 复用统一 `emit_variable_event`，并按 full/closed 分级记录 `error!` 或至少 `warn!`。
- 考虑暴露 dropped counter 或 `BackpressureDetected` 事件。
- 补 channel full/closed 测试。

**验证方式：**

```bash
cargo test -p nazh-core variables
```

Expected: channel full/closed 被可观测记录，测试能断言日志或计数。

---

## 4. 脚本与流程节点语义

### CR-P2-14 Rhai AI helper 阻塞 async worker 且不易被 timeout 抢占

**影响文件：**

- `crates/scripting/src/lib.rs:220`
- `crates/core/src/guard.rs:19`
- `crates/nodes-flow/src/code_node.rs:108`

**错误场景：**

脚本中调用 `ai_complete()` 时，native helper 创建 OS thread 执行 async runtime，然后当前调用线程 `join()` 等待。`CodeNode::transform()` 在 async runner 中调用脚本，等待期间占住 Tokio worker。`guarded_execute` 的 timeout 只能在 future 被 poll 时生效，阻塞段不易被及时抢占。

**可能触发路径：**

1. 多个 code/if/switch 节点脚本并发调用 AI helper。
2. 每个节点阻塞当前 worker 等待 thread join。
3. Tokio worker 被耗尽或延迟抖动。
4. timeout 到达但执行段未让出，取消不及时。

**修复方案：**

- 把脚本 AI 调用改为异步边界：节点层识别 AI call 并 await，或脚本执行整体放入 `spawn_blocking`。
- 为 AI helper 设置独立 timeout，并支持取消/中断。
- 限制脚本内 AI helper 的并发数，避免线程风暴。

**验证方式：**

```bash
cargo test -p scripting ai
cargo test -p nodes-flow code_node
```

Expected: 并发 AI 脚本调用不会阻塞 Tokio worker，timeout 可稳定触发。

### CR-P1-09 switch 未知分支不落 default

> **状态：** 2026-05-09 已修复。`switch` 只路由到已声明分支或 `default_branch`；未知非空分支名、空字符串和 unit 均落到 default，并新增回归测试。

**影响文件：**

- `crates/nodes-flow/src/switch_node.rs:3`
- `crates/nodes-flow/src/switch_node.rs:127`

**错误场景：**

文档说 switch 未匹配时路由到 default；实现只在返回 unit 或空字符串时走 default。脚本返回任意非空未知字符串时，节点会 `Route([unknown])`，如果这个 output pin 不存在，下游消息静默丢失。

**可能触发路径：**

1. 用户脚本返回 `"unexpected"`。
2. `branches` 中没有该 key。
3. `switch` 直接 route 到 `"unexpected"`。
4. Graph 没有对应边，消息消失。

**修复方案：**

- `next_branch` 只允许命中 `branches.key`。
- 未命中统一落 `default_branch`；如果没有 default，应返回明确错误。
- 部署期和运行时都要校验 route 只能落声明的 output pins。

**验证方式：**

```bash
cargo test -p nodes-flow switch
```

Expected: 返回未知分支时走 default 或明确失败。

### CR-P2-15 if/switch 标记 PURE 但脚本环境非纯

**影响文件：**

- `crates/nodes-flow/src/lib.rs:49`
- `crates/nodes-flow/src/lib.rs:63`
- `crates/scripting/src/lib.rs:329`
- `crates/scripting/src/package.rs:31`

**错误场景：**

`if/switch` 注册为 `PURE | BRANCHING`，但共用 `ScriptNodeBase`，脚本环境暴露 `rand()`、`now_ms()` 和 `vars.set/cas` 等非确定性或有副作用能力。未来如果 graph 对 PURE 节点做输入哈希缓存，分支结果可能被错误复用。

**可能触发路径：**

1. 用户脚本在 if/switch 中读取时间、随机数或写变量。
2. 节点能力表仍声明 PURE。
3. Runtime/AI/优化器按 PURE 假设做缓存或重排。
4. 分支行为与实际脚本副作用冲突。

**修复方案：**

- 短期移除 `if/switch` 的 PURE 标签。
- 如需要 PURE，创建受限脚本环境，禁用变量写入、随机数、时间和 AI helper。
- 节点能力表、AGENTS 和 registry contract test 同步更新。

**验证方式：**

```bash
cargo test -p nodes-flow registry
```

Expected: 能力声明与脚本环境一致。

### CR-P2-16 stateMachine transition 没有 step limit

**影响文件：**

- `crates/nodes-flow/src/state_machine.rs:31`
- `crates/nodes-flow/src/state_machine.rs:115`
- `crates/nodes-flow/src/state_machine.rs:194`

**错误场景：**

`StateMachineConfig` 有 `max_operations` 字段，但构造和运行时求值都用裸 `rhai::Engine::new()`，没有 `set_max_operations`。恶意或错误 transition 条件可能长时间执行。

**可能触发路径：**

1. transition `when` 中写入复杂循环或高成本表达式。
2. stateMachine 求值没有 step limit。
3. worker 被长时间占用。
4. panic/timeout 保护不一定及时阻断同步 Rhai 执行。

**修复方案：**

- 复用 scripting crate 的受限 engine 配置。
- 使用 `config.max_operations` 设置 Rhai step limit。
- 构造时预编译 AST，运行时 eval AST。

**验证方式：**

```bash
cargo test -p nodes-flow state_machine
```

Expected: 超步数 transition 条件返回错误，不阻塞 runtime。

### CR-P3-03 脚本文档与实际 package 不一致

**影响文件：**

- `crates/scripting/AGENTS.md:20`
- `crates/scripting/src/package.rs:27`

**错误场景：**

AGENTS 记录 `NazhScriptPackage` 提供 `sleep_ms()`，实际 package 没有导出。用户或 AI 按文档生成脚本会编译失败。

**可能触发路径：**

1. AI 读取 AGENTS 生成脚本。
2. 脚本调用 `sleep_ms()`。
3. Rhai 编译或运行时报函数不存在。

**修复方案：**

- 如果不希望脚本阻塞，删除文档中的 `sleep_ms()`。
- 如果保留，必须明确阻塞/step-limit 语义，并放入 `spawn_blocking` 或异步安全路径。

**验证方式：**

```bash
cargo test -p scripting package
```

Expected: 文档列出的 helper 与实际导出一致。

### CR-P3-04 nodes-flow stateMachine output pin 文档漂移

**影响文件：**

- `crates/nodes-flow/AGENTS.md:20`
- `crates/nodes-flow/AGENTS.md:73`
- `crates/nodes-flow/src/state_machine.rs:85`
- `crates/nodes-flow/src/state_machine.rs:325`

**错误场景：**

AGENTS 说 stateMachine output pin 是 `state.id` / `transition.to`；实现实际收集 entry/exit/action port 并路由这些 port。前端、DSL 或 AI 按文档连边会被部署校验拒绝或运行时路由不通。

**可能触发路径：**

1. AI 根据 crate AGENTS 生成 stateMachine output edges。
2. edges 指向 state id。
3. 实现只声明 action port。
4. pin validator 报未知 pin，或运行时不触发预期端口。

**修复方案：**

- 选择当前 action-port 语义作为真值，并更新 AGENTS、DSL 文档和示例。
- 如文档才是目标，则回改实现和 compiler。

**验证方式：**

```bash
cargo test -p nodes-flow state_machine
cargo test -p dsl-compiler
```

Expected: 文档、编译器、节点 pin 声明一致。

### CR-P3-05 lookup 错误定位硬编码 node_id

**影响文件：**

- `crates/nodes-pure/src/lookup.rs:68`

**错误场景：**

`LookupNode::stringify_key` 对复杂 key 报错时硬编码 node_id 为 `"lookup"`。多个 lookup 节点同时存在时，错误无法定位到实际节点。

**可能触发路径：**

1. workflow 中有多个 lookup 节点。
2. 某个节点收到 object/array key。
3. 错误消息统一显示 `"lookup"`。
4. 用户难以定位配置错误节点。

**修复方案：**

- 把 `stringify_key` 改成实例方法，或传入 `self.id`。
- 测试断言错误消息包含实际 node id。

**验证方式：**

```bash
cargo test -p nodes-pure lookup
```

Expected: 错误信息携带具体节点 ID。

---

## 5. AI 与 IPC 类型契约

### CR-P2-17 DeepSeek thinking 字段注入边界错误

> **状态：** 2026-05-09 已修复。`ai` crate 新增 DeepSeek provider/model 判定；非 DeepSeek provider 完全省略 `thinking` / `reasoning_effort`，DeepSeek 且全局 thinking 开启时才注入扩展字段；测试连接同样遵守该边界。

**影响文件：**

- `crates/ai/AGENTS.md:45`
- `crates/ai/src/client.rs:342`
- `crates/ai/src/client.rs:448`
- `crates/ai/src/client.rs:557`
- `crates/ai/src/client.rs:699`

**错误场景：**

AGENTS 约定按 `base_url/model` 判断是否注入 DeepSeek thinking/reasoning 字段；实现主要根据全局 `thinking_enabled`，且 `include_thinking_options == false` 时仍会发送 `thinking: { type: "disabled" }`。普通 OpenAI-compatible provider 可能因未知字段拒绝请求。

**可能触发路径：**

1. 用户配置非 DeepSeek OpenAI-compatible provider。
2. thinking 全局配置为 disabled 或 enabled。
3. 请求 payload 仍带 DeepSeek 风格字段。
4. provider 返回 400 或忽略字段，行为不可预测。

**修复方案：**

- 增加 `provider_accepts_deepseek_options(provider, model)`。
- 非 DeepSeek provider 完全省略 `thinking/reasoning_effort` 字段。
- 单测覆盖 DeepSeek 与非 DeepSeek payload。

**验证方式：**

```bash
cargo test -p ai thinking
```

Expected: 非 DeepSeek payload 不含 DeepSeek 私有字段。

### CR-P2-18 streaming 内置重试和吞错违背 crate 约定

> **状态：** 2026-05-09 已修复。streaming 路径移除内部伪断点续传；SSE read error 直接转 `AiError::NetworkError`，事件 JSON parse error 转 `AiError::ResponseParseError` 并携带 event preview，由上层 orchestrator 负责重试策略。

**影响文件：**

- `crates/ai/AGENTS.md:47`
- `crates/ai/src/client.rs:201`
- `crates/ai/src/client.rs:585`
- `crates/ai/src/client.rs:602`
- `crates/ai/src/client.rs:650`

**错误场景：**

AGENTS 写明重试策略在上层 orchestrator，ai crate 透明传 `AiError`。实现却内置 `MAX_STREAM_RETRIES` 和伪断点续传；SSE read error 被 break，JSON parse error 被 continue。UI 最终只看到泛化“意外中断”，且可能和上层重试重复造成内容重复。

**可能触发路径：**

1. provider streaming 中途网络错误或返回坏 JSON。
2. client 内部吞掉 parse/read 错误或尝试续写。
3. 上层无法区分协议错误、网络错误、模型错误。
4. UI 展示错误不准确，或重复续写。

**修复方案：**

- ai crate 只负责解析和透明转发错误。
- SSE read/JSON parse 错误发送带 preview 的 `AiError`。
- 重试和续写移到上层 orchestrator，并记录 retry policy。

**验证方式：**

```bash
cargo test -p ai stream
```

Expected: SSE/JSON 错误被明确传播；ai crate 不内置业务重试。

### CR-P3-06 extra_headers 可能成为密钥旁路

**影响文件：**

- `crates/ai/src/config.rs:195`
- `crates/ai/src/config.rs:209`
- `crates/ai/src/config.rs:261`
- `crates/ai/src/client.rs:475`

**错误场景：**

`extra_headers` 被保存到磁盘、原样回传前端，并允许前端提交任意 header。如果未来 provider 需要 `Authorization` 或 `X-Api-Key` 形式 header，这些 secret 会绕过 `AiSecretInput` 的 keep/clear/set 隔离。

**可能触发路径：**

1. 用户在 extra headers 中填入敏感 header。
2. 配置保存到磁盘。
3. `to_view()` 原样返回前端。
4. 日志、调试或 UI 泄露敏感值。

**修复方案：**

- 明确 extra_headers 只能存非敏感 header。
- 拒绝或 mask 常见敏感 header 名。
- 如确需 secret headers，拆成 secret record 和 masked view。

**验证方式：**

```bash
cargo test -p ai config
```

Expected: 敏感 header 不会明文进入 view。

### CR-P3-07 AI 请求配置快照非原子一致

**影响文件：**

- `crates/ai/src/client.rs:438`
- `crates/ai/src/client.rs:448`
- `crates/ai/src/client.rs:542`
- `crates/ai/src/client.rs:557`

**错误场景：**

一次请求先读 provider 快照，再读 agent_settings。并发保存配置时，一次请求可能混用旧 provider/API key 和新 thinking 设置。

**可能触发路径：**

1. UI 保存 AI 配置。
2. 同时发起 completion/stream request。
3. request 内多次读锁拿到不同版本字段。
4. provider 与 request policy 不一致。

**修复方案：**

- 在一次读锁内解析 provider、secret、agent_settings。
- 形成不可变 `ResolvedProvider`/request policy 快照后释放锁。
- 单测用并发配置更新模拟混合读。

**验证方式：**

```bash
cargo test -p ai config_snapshot
```

Expected: 单次请求使用同一版本配置。

### CR-P2-19 WorkflowRuntimePolicyInput 的 Option TS 契约不一致

> **状态：** 2026-05-09 已修复。`WorkflowRuntimePolicyInput` 的 Option 字段已补 `ts(optional)` 并重新导出；毫秒字段显式导出为 `number`，前端 `deployWorkflow` 删除手写镜像接口，改用生成类型。

**影响文件：**

- `crates/tauri-bindings/AGENTS.md:42`
- `crates/tauri-bindings/src/lib.rs:411`
- `web/src/generated/WorkflowRuntimePolicyInput.ts:7`
- `web/src/lib/tauri.ts:31`

**错误场景：**

crate 规则要求 Option 字段标 `ts(optional)`；`WorkflowRuntimePolicyInput` 的 Option 字段没有 attribute，生成 TS 为必填 `T | null`。前端已用手写接口绕过生成类型，削弱 single source of truth。

**可能触发路径：**

1. Rust input type 字段是 `Option<T>`。
2. ts-rs 生成 TS 要求字段必填但值可 null。
3. 前端为了 ergonomics 手写接口。
4. Rust/TS 契约开始漂移。

**修复方案：**

- 给该结构所有可缺省 Option 字段补 `#[ts(optional)]`。
- 重新生成 `web/src/generated/`。
- 删除或减少前端手写镜像类型。

**验证方式：**

```bash
cargo test -p tauri-bindings --features ts-export export_bindings
npm --prefix web run build
```

Expected: TS 生成类型字段可选，前端使用 generated type。

### CR-P2-20 stale AI 生成物目录仍被跟踪

> **状态：** 2026-05-09 已修复。已删除并 ignore `crates/ai/bindings/` 与 `crates/ai/web/src/generated/` 两套侧生成物；`crates/ai/AGENTS.md` 明确只有 root `web/src/generated/` 是提交和 CI 校验的 TS 契约真值源。

**影响文件：**

- `crates/ai/bindings/`
- `crates/ai/web/src/generated/`
- `web/src/generated/`

**错误场景：**

仓库中同时存在 root `web/src/generated/` 与 `crates/ai/bindings/`、`crates/ai/web/src/generated/`。后两套已漂移，例如缺少 `agentSettings`、thinking/reasoning 参数。读者或工具可能引用 stale 类型。

**可能触发路径：**

1. AI 或开发者在 `crates/ai` 下搜索 generated type。
2. 读到旧文件，误以为字段不存在。
3. 前后端契约判断错误。
4. CI 只校验 root generated，侧目录继续漂移。

**修复方案：**

- 删除两套侧生成目录，或迁入 ignore 并加废弃说明。
- 明确 root `web/src/generated/` 是唯一提交和 CI 真值源。
- CI 可增加检查，禁止跟踪多个 generated 输出根。

**验证方式：**

```bash
cargo test -p tauri-bindings --features ts-export export_bindings
git diff -- web/src/generated
```

Expected: 只有 root generated 目录参与契约，侧目录不存在或明确废弃。

---

## 6. 文档与工程治理

### CR-P3-08 crates/graph 缺 crate-local AGENTS.md

> **状态：** 2026-05-09 已修复。已新增 `crates/graph/AGENTS.md`，覆盖 DAG 调度模型、Exec/Data/Reactive 边语义、DataStore 引用计数、OutputCache/PureMemo 生命周期、依赖约束和测试清单。

**影响文件：**

- `AGENTS.md:227`
- `crates/graph/`

**错误场景：**

根规则要求每个 `crates/*/` 有且仅有一个 crate-local `AGENTS.md`，但 `crates/graph` 缺失。DAG 调度、pin 分类、pull collector、backpressure、DataStore 引用计数这些局部约定没有 crate-local 真值源。

**可能触发路径：**

1. 未来修改 graph crate。
2. agent 或开发者只读根 AGENTS，缺少 graph 局部边界。
3. Data/Exec/Reactive pin 语义、DataStore 释放规则被误改。

**修复方案：**

- 新增 `crates/graph/AGENTS.md`。
- 覆盖：调度模型、edge 分类、DataStore 引用计数、OutputCache/PureMemo 生命周期、测试清单。
- 与 root AGENTS 的 per-crate 规则保持一致。

**验证方式：**

```bash
find crates -maxdepth 2 -name AGENTS.md | sort
```

Expected: 每个 `crates/*` 都有一个 crate-local AGENTS。

### CR-P3-09 大文件职责混合影响长期维护

**影响文件：**

- `crates/dsl-compiler/src/safety.rs`
- `crates/connections/src/lib.rs`
- `crates/core/src/variables.rs`
- `crates/tauri-bindings/src/lib.rs`
- `crates/nodes-io/src/serial_trigger.rs`
- `crates/ai/src/client.rs`

**错误场景：**

多个手写文件超过 500 行，部分超过 1000 行。文件混合 DTO、状态机、验证、运行时逻辑、测试或导出聚合。后续修复容易在一个文件里叠加更多职责，审查成本上升。

**可能触发路径：**

1. 修 P1/P2 时继续在现有大文件中追加逻辑。
2. 单文件职责继续扩大。
3. 后续 review 难以确认边界和副作用。

**修复方案：**

- 不做一次性大重构；每个 P1/P2 修复顺手提取最相关的小模块。
- `connections/src/lib.rs` 可拆：types/policy/manager/validation/shared_session。
- `dsl-compiler/src/safety.rs` 可拆：state_graph/capability_rules/report。
- `ai/src/client.rs` 可拆：payload/streaming/provider_policy。
- `tauri-bindings` 如果保持聚合，应在 AGENTS 中说明例外原因。

**验证方式：**

```bash
wc -l crates/*/src/*.rs
```

Expected: 后续修复不继续扩大最重文件；拆分后模块边界可由 crate AGENTS 描述。

---

## 建议实施顺序

### Task 1: 先让 DSL 到 runtime 的 P1 问题 fail-fast

**Files:**

- Modify: `crates/dsl-compiler/src/output.rs`
- Modify: `crates/dsl-compiler/src/context.rs`
- Modify: `crates/nodes-flow/src/state_machine.rs`
- Test: `crates/dsl-compiler/tests/*`
- Test: `crates/nodes-flow/src/*`

- [x] 为重复 capability target + 不同 args 写失败测试。
- [x] 对未实现 timeout/system action 路径先 fail-fast。
- [x] 明确 DSL 表达式作用域，并补编译期校验。
- [x] 更新 DSL 示例与 crate AGENTS。

> 进度：2026-05-09 已完成 `dsl-compiler` 层面的 fail-fast 与参数绑定修复，并完成 `capabilityCall` 对 `connection_id` 继承、Modbus/MQTT/Serial/CAN 真实执行入口、CAN mock 成功路径与 script implementation fail-fast。

### Task 2: 修连接治理的并发与替换语义

**Files:**

- Modify: `crates/connections/src/lib.rs`
- Modify: `crates/nodes-io/src/can/*`
- Modify: `crates/nodes-io/src/ethercat/*`
- Test: `crates/connections/src/*`
- Test: `crates/nodes-io/src/*`

- [x] 为 shared session 并发初始化写测试。
- [x] 为 in-use 连接替换写测试。
- [x] 收紧 failure outcome 与 guard failure 统计。
- [x] 明确 mock/simulation 策略。

> 进度：2026-05-08 已完成 `connections` 共享会话合流、运行中替换保护、failure outcome 计数闭环，以及 CAN 隐式 Mock 回退。`nodes-io` 后续协议失败清理（如 EtherCAT/SLCAN 细分错误）仍按后续 P2 条目推进。

### Task 3: 修 DataStore/背压生命周期闭环

**Files:**

- Modify: `crates/graph/src/runner.rs`
- Modify: `crates/graph/src/types.rs`
- Modify: `crates/core/src/lifecycle.rs`
- Modify: `crates/core/src/variables.rs`
- Test: `crates/graph/src/*`
- Test: `crates/core/src/*`

- [x] 为 downstream/root channel closed 写 DataStore 释放测试。
- [x] 定义 ingress partial submit 语义。
- [x] 修 Data-only output result 噪声。
- [x] 统一变量事件背压日志和计数。

> 进度：2026-05-09 已完成 DataStore 引用补偿释放、Data-only result 噪声修复，以及变量事件背压日志/计数闭环。

### Task 4: 收紧脚本、AI 和类型契约

**Files:**

- Modify: `crates/scripting/src/lib.rs`
- Modify: `crates/nodes-flow/src/*`
- Modify: `crates/ai/src/*`
- Modify: `crates/tauri-bindings/src/lib.rs`
- Modify: `web/src/generated/*`

- [ ] 移除或隔离非纯脚本能力。
- [x] 修 DeepSeek provider policy 和 streaming 错误传播。
- [x] 给 `WorkflowRuntimePolicyInput` 补 `ts(optional)` 并重新导出。
- [x] 删除或废弃 stale generated 目录。

### Task 5: 文档与治理收口

**Files:**

- Create: `crates/graph/AGENTS.md`
- Modify: `crates/*/AGENTS.md`
- Modify: `docs/project-status.md`
- Modify: `README.md` if node/catalog behavior changes

- [x] 补 `crates/graph/AGENTS.md`。
- [ ] 修 `scripting`、`nodes-flow`、`dsl-core`、`ai` AGENTS 中和实现冲突的规则。
- [ ] 大文件拆分仅跟随功能修复推进，不单独做无目标重构。

---

## 全量验证清单

修复批次完成后，至少运行：

```bash
cargo fmt --all -- --check
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo test -p tauri-bindings --features ts-export export_bindings
npm --prefix web run build
```

如涉及前端节点库或 IPC 行为，再运行：

```bash
npm --prefix web run test
npm --prefix web run test:e2e
```

Expected: Rust 工作区测试、clippy、格式、ts-rs 导出和前端构建全部通过；生成物 diff 只出现在 `web/src/generated/`。
