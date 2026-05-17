# Project Status（2026-05-16）

从 `AGENTS.md` 拆出的项目状态追踪。本文件随 ADR 落地、技术债偿还、路线图推进而更新。

**Phases 1-5 complete** (crate extraction, DataStore, ConnectionGuard, Ring 1 split, Plugin system). See `docs/rfcs/0002-分层内核与插件架构.md`.

**Architecture review batch**（2026-04-29）：
- 2026-04-29 architecture review 的 Phase B/C/D/E 已完成收尾；历史 working docs 已清理，长期结论沉淀在本文件、根 `AGENTS.md` 与对应 crate `AGENTS.md`。
- `src-tauri/src/lib.rs` 已按 IPC 命令域拆到 `src-tauri/src/commands/*`，`lib.rs` 只保留 setup + handler 注册（132 行）。
- 规范扫描结论：生产代码 `.unwrap()` / `.expect()` 0 命中、`unsafe` 0 命中、节点不直接读写 `DataStore`；`native` 节点 payload 键从 `_native_message` 修正为 `native_message`。
- **已解冻**：架构审阅 Phase A/B/C/D/E 全部完成（2026-04-30）；原 ARCHITECTURE FREEZE 段已删除。ADR-0016 仍有 deferred items，但不再阻塞常规 PR 流程。
- **P1/P2 技术债批量偿还**（2026-05-03，commit 2e428a2）：变量事件独立通道（`WorkflowVariableEvent`）+ `NodeOutput.metadata` 改 `Option<Map>` + Rhai `default_max_operations` 统一 + `workflow.rs` 拆为 `workflow_deploy/dispatch/undeploy` 三模块 + FlowgramCanvas 988 行 / ConnectionStudio 1372 行 + core/connections/ai crate AGENTS.md 同步 + 17 IPC 类型迁入 `tauri-bindings`。详见下文"Immediate known tech debt"。

## Architecture / ADR status（updated 2026-05-16）

- ADR-0008 (metadata separation) — **accepted / landed**
- ADR-0017 (IPC + ts-rs 迁出 Ring 0) — **已实施**（2026-04-24，见 `crates/tauri-bindings/`）
- ADR-0011 (节点能力标签 `NodeCapabilities`) — **已实施**（2026-04-24，位图落在 `crates/core/src/node.rs`，前端常量表 `web/src/lib/node-capabilities.ts`）
- ADR-0009 (节点生命周期钩子) — **已实施**（2026-04-26，`crates/core/src/lifecycle.rs` + Timer / Serial / MQTT 三类节点 `on_deploy` + `WorkflowDeployment::shutdown`；壳层 ~1000 行回收）
- ADR-0010 (Pin 声明系统) — **已实施 Phase 1 + Phase 2 + Phase 3 + Phase 4 + Phase 4.1**（Phase 1: 2026-04-26，Ring 0 类型 + 部署期校验器 + `if`/`switch`/`loop`/`tryCatch` 四个分支节点声明具体输出 pin；Phase 3: 2026-04-26，`modbusRead` / `sqlWriter` / `httpClient` / `mqttClient` 四协议节点 input/output 收紧到 `Json`（mqttClient 按 mode 实例方法切换）+ 兼容矩阵合约 fixture `tests/fixtures/pin_compat_matrix.jsonc` 作为前后端共享真值源 + 反向兼容性集成测试；Phase 2: 2026-04-26，IPC `describe_node_pins` + `web/src/lib/{pin-compat,pin-schema-cache,pin-validator}.ts` + FlowGram `canAddLine` 钩子接入连接期校验 + branch ports 按 PinType 着色。Phase 4: 2026-04-27，pin tooltip + AI 脚本生成 prompt 携带 pin schema。Phase 4.1: 2026-05-13，httpClient / serialTrigger / mqttClient / capabilityCall 补齐 `useDynamicPort`，输出端口 PinType/PinKind 着色和 tooltip 与 modbusRead/canRead 对齐。`Custom` 类型 + row-formatter 节点仍 defer——触发条件（≥2 真实协议级类型隔离场景）2026-05-13 复评仍未满足。两层防御=UI 拦截+部署期 backstop）
- ADR-0019 (AI 能力依赖反转) — **已实施**（2026-04-26，`AiService` trait + 请求/响应类型上移到 `crates/core/src/ai.rs`；`ai` crate 改为纯实现 + 配置型；`scripting` / `nodes-flow` 不再依赖 `ai`）
- ADR-0018 (`nodes-io` 按协议 feature 门控) — **已实施**（2026-04-26，`io-sql/io-http/io-mqtt/io-modbus/io-serial/io-notify` + 元 feature `io-all`；facade 转传；`debug/native/timer/template` 永远启用）
- ADR-0012 (工作流变量) — **已实施 Phase 1+2+3**（Phase 1: 2026-04-27 / Phase 2: 2026-04-27 / Phase 3: 2026-05-03。Phase 3 含 reset/delete/history IPC + 变量持久化 `crates/store/`（ADR-0022）+ 部署时恢复 + 历史曲线 + 全局变量 CRUD + 删除确认弹窗 + React Testing Library 组件测试）
- ADR-0014（执行边与数据边分离 → 重命名为「引脚求值语义二分」）— **已实施 Phase 1 + Phase 2 + Phase 3 + Phase 3b + Phase 4 + Phase 5**（2026-04-30）。Phase 5：节点头部 capability 自动着色 + CSS 变量化 + AI prompt PinKind + watch channel 替代 Notify + PureMemo trace 清理。Phase 6 EventBus（RFC-0002）已完成修订（否决 broadcast，改为 try_send 修复）。ADR-0015 已实施。
- ADR-0013（子图与宏系统）— **已实施 子图核心**（2026-04-28，merge 68ab709 时丢失的 ADR-0013 改动恢复完成）。前端 `subgraph` 容器 + `subgraphInput` / `subgraphOutput` 桥接 + 设置面板 + AI 编排器扩展全部就位；`web/src/lib/flowgram.ts` 的 `flattenSubgraphs` 完整实现（递归展平 + 参数替换 `{{name}}` + 8 层深度上限 + 循环引用检测）；Rust `crates/nodes-flow/src/passthrough.rs` 已注册（`mod passthrough` + `subgraphInput` / `subgraphOutput` 通过 `NodeCapabilities::empty()` 在 `FlowPlugin::register` 内注册）；`tests/workflow.rs` `passthrough_nodes_forward_payload` 集成测试通过；`vitest.config.ts` 新增 `setupFiles: ['./vitest.setup.ts']` polyfill `navigator` 让 FlowGram SDK 在 node 环境正常 import；顺手修了 pre-existing 的 `flowgram-shortcuts.test.ts` 失败；loop 容器恢复已并入当前 `main`。
- ADR-0015（反应式数据引脚）— **Phase 1+2+3 已实施**（2026-04-30）。Phase 1: PinKind::Reactive + Runner 三分支 dispatch + 集成测试。Phase 2: WorkflowVariables watch channel + `subscribe_reactive_pin` IPC + `ReactiveUpdatePayload` ts-rs。Phase 3: 前端 isKindCompatible 三分支 + Reactive 端口 CSS + reactive-update 事件解析 + 合约 fixtures 扩展。
- ADR-0016（边级可观测性）— **已实施**（2026-05-13）。`EdgeTransmitSummary` 类型 + Runner `EdgeWindow` 100ms 定时窗口累计器 + `BackpressureDetected` 背压检测（队列深度 ≥80% 容量时发射，每窗口限频一次）+ ts-rs 导出 + 前端解析 + 边热力图 CSS（`flowgram-line--heat-{1-4}` 渐变着色 + `flowgram-line--backpressure` 红色闪烁）。`payload_bytes`/`received_at`/`queue_depth` 精确统计仍为未来可选项。
- ADR-0020 — **已实施**（2026-05-01：`src/graph/` 拆分为 `crates/graph/`）。见 `docs/adr/0020-graph-编排层长期归属.md`。
- ADR-0022 (工作流变量持久化) — **已实施**（2026-05-03，`crates/store/` Ring 1 SQLite crate + 壳层持久化钩子 + 部署时恢复）
- ADR-0024 (设备信号读取与事件触发节点) — **已实施 Phase 1+2+3**（Phase 1: 2026-05-15，`deviceSignalRead` 节点 + `signal_decode.rs` 共享解码模块；Phase 2: 2026-05-15，`deviceEventTrigger` 事件监听节点（MQTT + CAN）；Phase 3: 2026-05-16，全协议覆盖——`deviceSignalRead` 支持 CanFrame/Topic/EthercatPdo/SerialCommand，`deviceEventTrigger` 支持 Modbus 定时轮询和 Serial 帧监听；前端节点库卡片就位。注册表合约测试更新至 29 种节点）
- RFC-0003 Phase 2 / Phase 3 子集 — **已实施**（2026-05-16）：`observability_records` SQLite 索引表接入 `observability.rs`，事件/审计/告警双写 Store + JSONL，`query_observability` 优先查 Store、失败/空结果回退 JSONL；新增 `deployment_audit` 表并写入 deploy / undeploy 生命周期动作。批量 writer、变量变更审计、部署 ast_hash 版本管理与审计查询 IPC 仍待后续。
- RFC-0004 Phase 3 (Workflow DSL 编译器) — **已实施**（2026-05-03，`crates/dsl-compiler/` 编译器 + `stateMachine` + `capabilityCall` 节点类型 + 一致性测试 + 集成测试；2026-05-09 `capabilityCall` 已接入 `connection_id` 继承与 Modbus/MQTT/Serial/CAN 执行入口，`script` implementation 未接入执行器时 fail-fast）
- RFC-0004 资产落盘与 AI 编辑挂接 — **已实施**（2026-05-05，Device / Capability 仅以工程工作路径 `dsl/devices` / `dsl/capabilities` YAML 文件持久化；SQLite 资产表逻辑已移除；新增 `load_ai_asset_context` IPC；画布内 AI 编辑读取已审查资产并可生成 `capabilityCall`）

## Immediate known tech debt

- **IPC surface 契约测试**（2026-05-17）：`src-tauri/tests/ipc_surface_contract.rs` 已建立，从 `generate_handler!` 块提取实际注册命令并与硬编码预期列表比对（82 个），同时验证每个 handler 条目都有对应的 `#[tauri::command]` 函数。增删 IPC 命令时必须同步更新预期列表，否则测试失败。
- **[SECURITY] AI API key 明文收敛**（2026-05-16）：API key 仍以明文存储于 `app_local_data_dir()/ai-config.json`，但当前决策是不接入 OS keychain / 自建加密 vault，改为收敛传播面：`load_ai_config` 永不返回明文 key，`load_ai_api_key` 走集中校验后按需返回，保存/加载时尽量将配置文件权限收敛到当前用户读写，敏感 extra headers 不保存不回传，前端默认关闭 key 读取调试日志。后续若产品安全边界升级，再另开 ADR 评估密钥后端。
- **Architecture review 派生 P1/P2**（2026-04-29，2026-05-09 复核）：~~变量控制事件从 `ExecutionEvent` 拆出~~（已偿还：`WorkflowVariableEvent` 独立枚举 + 独立通道，B1-R0-01/B1-R0-05）；~~`src/graph/` 触发 ADR-0020 重评~~（已偿还，2026-05-01 拆为 `crates/graph/`）；~~Rhai `max_operations` 增加统一 clamp~~（已偿还：`scripting::default_max_operations()` 统一，D-01）；~~`NodeOutput.metadata` 显式三值语义~~（已偿还：`Map` → `Option<Map>`，B1-R0-02）；~~前端大文件拆分~~（已偿还：FlowgramCanvas 2025→988 行 / ConnectionStudio 1372 行，C-02）；~~`workflow.rs` 单文件过大~~（已偿还：拆为 `workflow_deploy/dispatch/undeploy` 三模块，C-01）；~~runtime / dead-letter / scoped event 等 IPC 类型迁入 `tauri-bindings`~~（已偿还，壳层改用生成类型）。**剩余**：B4-IPC-06 `copilot://stream/{id}` payload 生成类型、以及 `src-tauri/src/runtime.rs` 二次拆分等持续治理项。~~ADR-0016 deferred items~~（已偿还：2026-05-13 BackpressureDetected 发射 + 定时窗口 + 前端热力图）。
- **Crates 审阅修复收口**（2026-05-09）：除 CR-P3-09 作为持续治理项不做无目标大重构外，其余 crates 审阅问题已完成代码修复与验证；stale AI 生成物目录已删除并由 `crates/ai/AGENTS.md` 固化 root `web/src/generated/` 的唯一真值源。大文件拆分的后续跟踪见 `docs/plans/2026-05-09-crates-large-file-split-plan.md`。
- **设备/连接节点边界收口**（2026-05-05，2026-05-09 部分收口，2026-05-13 完成阶段 1+2）：评审结论见 `docs/specs/2026-05-05-node-architecture-boundary-review.md`。当前连接资源层方向正确；`capabilityCall` 已成为 DSL 高级动作入口并接入真实协议执行；前端节点库新增"设备能力"分组并把 capabilityCall 列为业务编排首选，`serialTrigger` / `modbusRead` / `canRead` / `canWrite` / `mqttClient` 文案降级为调试/适配器，capabilityCall 连接绑定 UI 补齐，AI copilot system prompt 同步调整；阶段 1 完成 `canRead` / `canWrite` 引入显式 `simulation` 开关 + 默认 fail-fast + `on_deploy` 双层防御，对齐 `modbusRead` 标杆，工业现场漏配时不再静默给出假数据。EtherCAT 三件套维持原状。设备信号读取/事件入口（`deviceSignalRead` / `deviceEventTrigger`）已由 ADR-0024 实施完成（2026-05-16），支持全协议覆盖。
- **安全配置校验收口**（2026-05-16）：连接批量 upsert/replace 已复用 `validate_connection_definition`；`deviceEventTrigger` 追加部署期协议校验，单节点只允许一种监听协议，MQTT / Modbus / Serial / CAN 分别校验连接类型和现场必填字段，移除 Modbus unit 与 Serial baud/delimiter 的后台静默默认。
- ~~**ADR-0016 deferred items**（2026-04-30）~~ **已偿还**（2026-05-13）。`BackpressureDetected` 发射逻辑 + 100ms 定时窗口 flush + 前端边热力图已实施。`payload_bytes`/`received_at`/`queue_depth` 精确统计仍为未来可选项。
- ~~**ADR-0013 子图实施 deployment 断链**（2026-04-28 发现）~~ **已偿还（2026-04-28）**。merge 68ab709 解决冲突时丢失的 ADR-0013 改动重写恢复——`flattenSubgraphs` + Rust `mod passthrough` 注册 + `FlowgramCanvas` 容器/桥接渲染 + 设置面板全部到位，三件套全绿。loop 容器恢复已并入当前 `main`。
- ~~MQTT subscriber / Timer / Serial root lifecycle is owned by the Tauri shell.~~ **已偿还（2026-04-26，ADR-0009 已实施）**。三类触发器节点现自持 `on_deploy` + `LifecycleGuard`；壳层不再监督触发器任务。**语义变化**：触发器节点走 `NodeHandle::emit` 而非 `dispatch_router` 的 trigger lane，失去 backpressure / DLQ / retry / metrics 防御能力，等 ADR-0014 / ADR-0016 引擎级背压能力补回。
- ~~IPC response types in `crates/core/` contradict Ring 0 purity. ADR-0017 plans to extract `crates/tauri-bindings/`.~~ **已偿还（2026-04-24，ADR-0017 已实施）**
- ~~`cargo clippy --workspace --all-targets -- -D warnings` 在 `src-tauri` 与 observability 上失败~~ **已偿还（2026-04-26）**。`crates/nodes-io/src/http_client.rs` / `bark_push.rs` 的 `too_many_lines` 现以 `#[allow]` 抑制。

## RFC-0004 Phase 3

**已完成**（2026-05-03）。Workflow DSL 编译器全部就位：
- `crates/dsl-compiler/` — `WorkflowSpec` → `WorkflowGraph` JSON 编译器（引用校验 + 语义校验 + JSON 生成）
- `crates/nodes-flow/src/state_machine.rs` — `stateMachine` 节点（动态 output pins + Rhai 条件评估 + `NodeDispatch::Route`）
- `crates/nodes-io/src/capability_call.rs` — `capabilityCall` 节点（编译期快照 + 模板解析 + `connection_id` 继承 + Modbus/MQTT/Serial/CAN 协议执行；未接入执行器的 `script` implementation fail-fast）
- 4 个一致性测试（`WorkflowGraph::from_json()` 守护 schema 漂移）+ 集成测试
- 标准注册表当前节点总数 27（Flow 8 + IO 16 + Pure 3）；RFC-0004 Phase 3 新增 `stateMachine` / `capabilityCall`，其后又陆续合入 `humanLoop`（HITL 审批）+ EtherCAT 三件套（`ethercatPdoRead` / `ethercatPdoWrite` / `ethercatStatus`，2026-05-06）+ ADR-0024 双节点（`deviceSignalRead` / `deviceEventTrigger`，2026-05-15/16），注册表合约测试随之更新至 29 种

## RFC-0004 Phase 4

**已完成 4A + 画布资产上下文挂接**（2026-05-05）。AI 生成管道对接：
- 4A：设备/能力 AI 结构化提取提案 — `extract_device_proposal` / `extract_device_proposal_stream`（JSON 输出含 uncertainties + warnings）+ 前端 proposal 流程
- 资产落盘：设备/能力保存后写入工程工作路径 `dsl/devices/*.device.yaml`、`dsl/devices/versions/*.device.yaml`、`dsl/devices/sources/*.sources.yaml`、`dsl/capabilities/*.capability.yaml`、`dsl/capabilities/versions/*.capability.yaml`、`dsl/capabilities/sources/*.sources.yaml`，不再进入 SQLite
- AI 编辑挂接：新增 `load_ai_asset_context` IPC，画布内 `AiWorkflowComposer` 在编辑/生成前加载已审查 Device / Capability YAML，上下文进入 prompt；前端节点库补齐 `capabilityCall`
- 4B/4C 的 DSL 编辑器页面和 AI 编排控制台页面已在 2026-05-04 移除——设计评估结论是与核心画布创作能力冲突。DSL 编辑器是编译器中间态的裸露调试界面（功能闭环缺失），AI 编排的一次性生成器能力已由画布内 `AiWorkflowComposer` 覆盖。对应的 4 个 IPC 命令（`compile_workflow_dsl` / `load_compiler_asset_snapshot` / `ai_generate_workflow_dsl` / `ai_generate_workflow_dsl_stream`）和前端组件/hooks/CSS 全部清除。`crates/dsl-core/` 和 `crates/dsl-compiler/` 库 crate 保留。
- 4D 已完成：安全编译器对接（随 Phase 5 一起完成）
- 看板页 AI 新建画布入口已于 2026-05-05 移除——将创建画布的仪式感交还给使用者。移除 `BoardsPanel` 的 AI 编排按钮、`openCreate()` 函数及整条 prop 链。画布内 AI 编辑（`openEdit()`）保留。

## 设备建模页优化（2026-05-04）

设备建模前端全面优化 + PDF 说明书录入支持：
- **导入抽屉**：独立右侧滑入抽屉（`DeviceImportDrawer.tsx`），支持文本粘贴和 PDF 文件拖拽上传两种模式，多阶段进度条缓解等待焦虑
- **PDF 文本提取**：Rust 后端 `pdf-extract` crate + base64 IPC 传输；新增 `extract_text_from_pdf` / `extract_device_from_pdf` 两个 IPC 命令
- **设备列表优化**：卡片式列表 + 搜索过滤 + 类型徽章 + 空状态 CTA
- **详情视图优化**：徽章行 + section 卡片 + 表格样式
- **AI 抽取健壮性**：`resolve_provider_id()` 回退到 active_provider_id；AI 响应容错解析（`#[serde(default)]` + body preview）；`max_tokens: None` 尊重 per-provider 配置；`ai=info` 加入默认 tracing filter

## RFC-0004 Phase 5

**已完成**（2026-05-03）。安全编译器 6 条规则全部就位：
- `crates/dsl-compiler/src/safety.rs` — `SafetyDiagnostic` / `SafetyReport` / `DiagnosticLevel` 类型 + 6 条规则 + 单元测试
- 规则 1 `unit_consistency`：单位一致性校验（Warning 级）
- 规则 2 `range_boundary`：量程边界校验（Error/Warning 级）
- 规则 3 `precondition_reachability`：前置条件可达性校验（Error/Warning 级）
- 规则 4 `state_machine_completeness`：状态机完整性校验——不可达/死胡同/循环（Error/Warning 级）
- 规则 5 `dangerous_action_approval`：危险动作审批校验（Warning 级）
- 规则 6 `mechanical_interlock`：机械互锁校验（Warning 级）
- `compile_with_safety()` 新增编译入口（原有 `compile()` 不变）
- 编译器库 crate 内部集成安全诊断；前端页面已移除，IPC 透传已清除（2026-05-04）

## ADR-0012 Phase 3

**已完成**（2026-05-03）。全部候选项已实施：reset/delete IPC、删除确认弹窗、React Testing Library + 组件测试、变量持久化 `crates/store/`（ADR-0022）、历史曲线 IPC + recharts 折线图、全局变量 CRUD + 前端面板。

## ADR Execution Order

（2026-04-24 共识，依赖与独立性已分析过）

> 0. ✅ **ADR-0017** IPC + ts-rs 迁出 Ring 0 — 已实施（独立支线，crate 卫生）
> 1. ✅ **ADR-0011** 节点能力标签 — 已实施（首发第一阶段：`NodeCapabilities` 位图、`NodeRegistry::register_with_capabilities`、IPC `NodeTypeEntry.capabilities` 透传、前端 badges；`NodeTrait::capabilities()` 已在 review 中移除，能力查询走注册表；Runner 侧 `spawn_blocking` / 缓存等调度决策按 ADR 后续阶段推进）
> 2. ✅ **ADR-0009** 生命周期钩子（`on_deploy` + `LifecycleGuard`）— **已实施**（2026-04-26，Ring 0 lifecycle 模块 + Runner 两阶段部署 + Timer/Serial/MQTT 三类节点迁回；壳层 `src-tauri/src/lib.rs` 由 3609 → 2498 行）
> 3. ✅ **ADR-0010** Pin 声明系统 — **Phase 1 + Phase 2 + Phase 3 + Phase 4 部分已实施**（Phase 1: Ring 0 类型 + 部署期校验器 + 4 分支节点；Phase 3: 4 协议节点 input/output 收紧到 `Json`（保守方案，不引入 `Custom`）+ 兼容矩阵合约 fixture 前后端共享；Phase 2: IPC `describe_node_pins` + 前端 pin-compat/cache/validator 三件套 + FlowGram `canAddLine` 接入连接期校验 + branch ports 按 PinType 着色；Phase 4: pin tooltip + AI prompt 携带 pin schema。协议节点端口着色 / `Custom` 类型 + row-formatter 节点 deferred）
> 4. ✅ **ADR-0018 / ADR-0019**（独立支线，**已实施**，2026-04-26）— `nodes-io` 协议 feature 门控 + AI 依赖反转。`nazh-core::ai` 现为 trait + 类型源头；`nodes-io` 协议 dep 全部 optional
> 5. ✅ **ADR-0012** 工作流变量 — Phase 1+2+3 已实施（2026-04-27 / Phase 3: 2026-05-03）
> 6. ✅ **ADR-0013** 子图与宏（依赖 0010）— 子图核心已实施；loop 容器恢复已并入当前 `main`
> 7. ✅ **Phase 6 (RFC-0002)** EventBus + EdgeBackpressure + ConcurrencyPolicy — **已完成修订**（2026-04-16）。EventBus broadcast 否决，ConcurrencyPolicy/EdgeBackpressure 推迟；实际修复：`emit_event` 改 `try_send` + 错误日志。详见 RFC-0002 Phase 6 段。
> 8. ✅ **ADR-0014** Pin 求值语义二分 — **Phase 1 + Phase 2 + Phase 3 + Phase 3b + Phase 4 + Phase 5 已实施**（2026-04-30）。Phase 5：capability 着色 + PinKind prompt + watch channel + PureMemo trace 清理。Phase 6 EventBus 已完成修订。
> 9. ✅ **ADR-0015** 反应式数据引脚 — **Phase 1+2+3 已实施**（2026-04-30）。全部完成。
> 9b. ✅ **ADR-0016** 边级可观测性 — **已实施**（2026-05-13）。`EdgeTransmitSummary` + `BackpressureDetected` 已发射；100ms 定时窗口 flush；前端边热力图 UI。
> 9c. ✅ **ADR-0024** 设备信号读取与事件触发节点 — **Phase 1+2+3 已实施**（2026-05-16）。`deviceSignalRead`（全协议轮询读取）+ `deviceEventTrigger`（MQTT/CAN/Modbus/Serial 事件监听）+ `signal_decode.rs` 共享解码模块。
> 10. 真实协议驱动扩展（OPC-UA、Kafka 消费者等）
> 11. AI 能力扩展（embeddings、vision，未来 ADR）

## 评估性 ADR

- ADR-0020 `src/graph/` 编排层归属 — **已实施**（2026-05-01，拆为 `crates/graph/` Ring 1 crate，依赖 `nazh-core` + `connections`）。
- ADR-0023 EtherCAT TX/RX 任务终止后的恢复策略 — **方案 B 已实施**（2026-05-13）。约束来自 ethercrab 0.7：`PduStorage::try_split` 一次性消费 + `TxRxFut` 错误分支不归还 (tx, rx)，进程内无法软恢复。诊断守卫（`ensure_maindevice` 检测 `tx_handle.is_finished()`）已落地，命中时前端弹出确认对话框，用户可一键重启 nazh-desktop（IPC `restart_app` → `AppHandle::restart()`）。方案 C（vendor patch）和方案 D（切库）仍预研归档，若现场对一键重启仍不满意再评估。详见 `docs/adr/0023-ethercat-tx-rx-恢复策略-暂缓.md`。
