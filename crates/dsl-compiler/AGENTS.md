# dsl-compiler — Workflow DSL 编译器

RFC-0004 Phase 3 + Phase 5 核心组件：将 `WorkflowSpec`（状态机 YAML）编译为 `WorkflowGraph` JSON（部署管道输入格式），并执行安全编译器校验。

## 对外暴露

- `compile(ctx: &CompilerContext, spec: &WorkflowSpec) -> Result<Value, CompileError>` — 编译入口（不含安全校验）
- `compile_with_safety(ctx: &CompilerContext, spec: &WorkflowSpec) -> Result<(Value, SafetyReport), CompileError>` — 编译 + 安全校验（RFC-0004 Phase 5）
- `run_safety_checks(ctx: &CompilerContext, spec: &WorkflowSpec, initial_state: &str) -> SafetyReport` — 独立运行安全校验
- `CompilerContext` — 持有设备/能力资产快照，提供引用校验；重复 device/capability id 会作为引用错误保留，不能静默覆盖
- `CompileError` — `Reference` / `StateMachine` / `CapabilityCall` / `Safety` / `OutputBuild`
- `SafetyDiagnostic` / `SafetyReport` / `DiagnosticLevel` — 安全编译器诊断类型

## 依赖约束

- **可依赖**：`nazh-dsl-core`, `serde`, `serde_json`, `thiserror`
- **禁止依赖**：`nazh-core`, `nazh-graph`, `nazh-engine`（避免循环依赖；一致性由 dev-dependency 测试守护）

## 编译流程

1. `context::CompilerContext::validate_references()` — 设备/能力引用存在性
2. `validate::validate_workflow_spec()` — 状态机语义校验（6 条规则）
3. `output::validate_supported_runtime_features()` — 拒绝当前运行时尚未闭环的 DSL 特性：`timeout/on_timeout`、`ActionTarget::Action`，以及 transition 条件中的裸变量
4. `output::validate_sanitized_ids()` — 拒绝 sanitize 后重复的 node id / action port id，并在错误中列出原始 id 来源
5. `validate::determine_initial_state()` — 初始状态选择（idle 优先 → 字典序）
6. `safety::run_safety_checks()` — 安全编译器校验（6 条规则，仅 `compile_with_safety` 调用）
7. `output::GraphBuilder` — 收集 actions → 生成 stateMachine 节点 → capabilityCall 节点 → edges → variables

## 文件结构

```text
crates/dsl-compiler/src/
├── context.rs
├── error.rs
├── output.rs
├── safety.rs           # 安全编译器入口与规则编排
├── safety/
│   ├── report.rs       # SafetyReport / SafetyDiagnostic 与诊断写入 helper
│   └── template.rs     # action 参数模板分类 helper
└── validate.rs
```

## 当前运行时支持边界

- transition `when` 条件必须显式引用 `payload.*` 或使用字面量/布尔表达式。不要生成 `start_button == true` 这类裸变量；stateMachine 运行时只注入 `payload`。
- `timeout` / `on_timeout` 仍由 `dsl-core` 建模，但 stateMachine 运行时尚未实现超时 tick/触发闭环；编译器必须 fail-fast。
- `action: <id>` system action 仍由 `dsl-core` 建模，但当前没有可执行节点承接；编译器必须 fail-fast，不能生成缺少 `script` 的伪 `code` 节点。

## 安全编译器规则（Phase 5）

| 规则 | 标识 | 级别 | 描述 |
|------|------|------|------|
| 1 | `unit_consistency` | Warning | 数值/变量引用的单位无法静态校验，提醒人工确认 |
| 2 | `range_boundary` | Error/Warning | 参数值超出能力输入量程（Error）；无法静态校验（Warning） |
| 3 | `precondition_reachability` | Error/Warning | 前置条件引用不存在的信号（Error）；不可读信号（Error）；运行时变量（Warning） |
| 4 | `state_machine_completeness` | Error/Warning | 从 initial 真实遍历后的不可达状态（Warning）；死胡同状态（Warning）；无条件自激循环（Error）；有条件业务回路（Warning） |
| 5 | `dangerous_action_approval` | Warning | High 安全等级 + requires_approval 的能力使用提醒 |
| 6 | `mechanical_interlock` | Warning | 同设备同寄存器的 ModbusWrite 冲突 |

## 一致性测试

`lib.rs` 中 4 个 conformance test：编译输出经 `serde_json::to_string()` → `WorkflowGraph::from_json()` 验证，守护 schema 契约不漂移。

## 节点 ID / 端口 ID 命名

- stateMachine 节点：`sm_{spec.id}`（经 sanitize）
- capabilityCall 节点：`cap_{target_id}_{port_id}`（经 sanitize）
- entry port：`entry_{state}_{index}`
- exit port：`exit_{state}_{index}`
- transition port：`trans_{from}_{to}_{transition_index}`
- sanitize 规则：`.` / `-` / ` ` → `_`
- 编译前必须检查 sanitize 后的 action port id 与 capabilityCall node id 是否碰撞；碰撞错误必须列出 sanitize 后 id 和原始 state / transition / target 来源。

## 变量类型推断

`Value::Number` 整数→`{ "kind": "integer" }`，浮点→`{ "kind": "float" }`，`String`→`{ "kind": "string" }`，`Bool`→`{ "kind": "bool" }`，其余→`{ "kind": "any" }`。

## Capability implementation 映射

`output.rs::capability_impl_to_json` 必须输出 `nodes-io::CapabilityImplSnapshot` 能反序列化的格式：`modbus-write.value` → `value_template`，`mqtt-publish.payload` → `payload_template`，`serial-command.command` → `command_template`，`can-write.data` → `data_template`，`script.content` 保持 `content`。

## 修改本 crate 时

- 编译输出格式变更必须同步更新 `WorkflowGraph` serde 格式 + 更新 conformance test
- 新增校验规则须同步 `validate.rs` 的 tests 模块
- 新增安全校验规则须同步 `safety.rs` 的 tests 模块；诊断结构放在 `safety/report.rs`，模板分类 helper 放在 `safety/template.rs`
- `output.rs` 的 `build_*` 方法变更须确认生成的 JSON 仍可被 `stateMachine` / `capabilityCall` 节点反序列化
- 改变 DSL 支持边界时，同步更新本文件和 `dsl-core` 示例，避免 AI/前端继续生成能解析但不能运行的工作流
