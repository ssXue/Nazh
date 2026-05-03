# Project Status（2026-05-03）

从 `AGENTS.md` 拆出的项目状态追踪。本文件随 ADR 落地、技术债偿还、路线图推进而更新。

**Phases 1-5 complete** (crate extraction, DataStore, ConnectionGuard, Ring 1 split, Plugin system). See `docs/rfcs/0002-分层内核与插件架构.md`.

**Architecture review batch**（2026-04-29）：
- `docs/superpowers/plans/2026-04-28-architecture-review.md` 的 Phase B/C/D/E 已完成本轮收尾，整合 findings 见 `docs/superpowers/specs/2026-04-29-architecture-review-findings.md`。
- `src-tauri/src/lib.rs` 已按 IPC 命令域拆到 `src-tauri/src/commands/*`，`lib.rs` 只保留 setup + handler 注册（132 行）。
- 规范扫描结论：生产代码 `.unwrap()` / `.expect()` 0 命中、`unsafe` 0 命中、节点不直接读写 `DataStore`；`native` 节点 payload 键从 `_native_message` 修正为 `native_message`。
- **已解冻**：`docs/superpowers/plans/2026-04-28-architecture-review.md` 的 Phase A/B/C/D/E 全部完成（2026-04-30）；原 ARCHITECTURE FREEZE 段已删除。ADR-0016 仍有 deferred items，但不再阻塞常规 PR 流程。
- **P1/P2 技术债批量偿还**（2026-05-03，commit 2e428a2）：变量事件独立通道（`WorkflowVariableEvent`）+ `NodeOutput.metadata` 改 `Option<Map>` + Rhai `default_max_operations` 统一 + `workflow.rs` 拆为 `workflow_deploy/dispatch/undeploy` 三模块 + FlowgramCanvas 988 行 / ConnectionStudio 1372 行 + core/connections/ai crate AGENTS.md 同步 + 17 IPC 类型迁入 `tauri-bindings`。详见下文"Immediate known tech debt"。

## Current batch of ADRs (2026-04-17 to 2026-04-29)

- ADR-0008 (metadata separation) — **accepted / landed**
- ADR-0017 (IPC + ts-rs 迁出 Ring 0) — **已实施**（2026-04-24，见 `crates/tauri-bindings/`）
- ADR-0011 (节点能力标签 `NodeCapabilities`) — **已实施**（2026-04-24，位图落在 `crates/core/src/node.rs`，前端常量表 `web/src/lib/node-capabilities.ts`）
- ADR-0009 (节点生命周期钩子) — **已实施**（2026-04-26，`crates/core/src/lifecycle.rs` + Timer / Serial / MQTT 三类节点 `on_deploy` + `WorkflowDeployment::shutdown`；壳层 ~1000 行回收）
- ADR-0010 (Pin 声明系统) — **已实施 Phase 1 + Phase 2 + Phase 3 + Phase 4 (部分)**（Phase 1: 2026-04-26，Ring 0 类型 + 部署期校验器 + `if`/`switch`/`loop`/`tryCatch` 四个分支节点声明具体输出 pin；Phase 3: 2026-04-26，`modbusRead` / `sqlWriter` / `httpClient` / `mqttClient` 四协议节点 input/output 收紧到 `Json`（mqttClient 按 mode 实例方法切换）+ 兼容矩阵合约 fixture `tests/fixtures/pin_compat_matrix.jsonc` 作为前后端共享真值源 + 反向兼容性集成测试；Phase 2: 2026-04-26，IPC `describe_node_pins` + `web/src/lib/{pin-compat,pin-schema-cache,pin-validator}.ts` + FlowGram `canAddLine` 钩子接入连接期校验 + branch ports 按 PinType 着色。Phase 4: 2026-04-27，pin tooltip + AI 脚本生成 prompt 携带 pin schema；协议节点端口着色 / `Custom` 类型 + row-formatter 节点 deferred。两层防御=UI 拦截+部署期 backstop）
- ADR-0019 (AI 能力依赖反转) — **已实施**（2026-04-26，`AiService` trait + 请求/响应类型上移到 `crates/core/src/ai.rs`；`ai` crate 改为纯实现 + 配置型；`scripting` / `nodes-flow` 不再依赖 `ai`）
- ADR-0018 (`nodes-io` 按协议 feature 门控) — **已实施**（2026-04-26，`io-sql/io-http/io-mqtt/io-modbus/io-serial/io-notify` + 元 feature `io-all`；facade 转传；`debug/native/timer/template` 永远启用）
- ADR-0012 (工作流变量) — **已实施 Phase 1+2+3**（Phase 1: 2026-04-27 / Phase 2: 2026-04-27 / Phase 3: 2026-05-03。Phase 3 含 reset/delete/history IPC + 变量持久化 `crates/store/`（ADR-0022）+ 部署时恢复 + 历史曲线 + 全局变量 CRUD + 删除确认弹窗 + React Testing Library 组件测试）
- ADR-0014（执行边与数据边分离 → 重命名为「引脚求值语义二分」）— **已实施 Phase 1 + Phase 2 + Phase 3 + Phase 3b + Phase 4 + Phase 5**（2026-04-30）。Phase 5：节点头部 capability 自动着色 + CSS 变量化 + AI prompt PinKind + watch channel 替代 Notify + PureMemo trace 清理。Phase 6 EventBus（RFC-0002）已完成修订（否决 broadcast，改为 try_send 修复）。ADR-0015 已实施。
- ADR-0013（子图与宏系统）— **已实施 子图核心**（2026-04-28，merge 68ab709 时丢失的 ADR-0013 改动恢复完成）。前端 `subgraph` 容器 + `subgraphInput` / `subgraphOutput` 桥接 + 设置面板 + AI 编排器扩展全部就位；`web/src/lib/flowgram.ts` 的 `flattenSubgraphs` 完整实现（递归展平 + 参数替换 `{{name}}` + 8 层深度上限 + 循环引用检测）；Rust `crates/nodes-flow/src/passthrough.rs` 已注册（`mod passthrough` + `subgraphInput` / `subgraphOutput` 通过 `NodeCapabilities::empty()` 在 `FlowPlugin::register` 内注册）；`tests/workflow.rs` `passthrough_nodes_forward_payload` 集成测试通过；`vitest.config.ts` 新增 `setupFiles: ['./vitest.setup.ts']` polyfill `navigator` 让 FlowGram SDK 在 node 环境正常 import；顺手修了 pre-existing 的 `flowgram-shortcuts.test.ts` 失败；loop 容器恢复已并入当前 `main`。
- ADR-0015（反应式数据引脚）— **Phase 1+2+3 已实施**（2026-04-30）。Phase 1: PinKind::Reactive + Runner 三分支 dispatch + 集成测试。Phase 2: WorkflowVariables watch channel + `subscribe_reactive_pin` IPC + `ReactiveUpdatePayload` ts-rs。Phase 3: 前端 isKindCompatible 三分支 + Reactive 端口 CSS + reactive-update 事件解析 + 合约 fixtures 扩展。设计 spec：`docs/superpowers/specs/2026-04-30-adr-0015-reactive-data-pin-design.md`。
- ADR-0016（边级可观测性）— **已接受，部分实施**（2026-04-30）。`EdgeTransmitSummary` 类型 + Runner `EdgeWindow` 累计器 + 每执行周期 flush + ts-rs 导出 + 前端解析。`BackpressureDetected` 类型就位，发射逻辑 deferred。
- ADR-0020 — **已实施**（2026-05-01：`src/graph/` 拆分为 `crates/graph/`）。见 `docs/adr/0020-graph-编排层长期归属.md`。
- ADR-0022 (工作流变量持久化) — **已实施**（2026-05-03，`crates/store/` Ring 1 SQLite crate + 壳层持久化钩子 + 部署时恢复）
- RFC-0004 Phase 3 (Workflow DSL 编译器) — **已实施**（2026-05-03，`crates/dsl-compiler/` 编译器 + `stateMachine` + `capabilityCall` 节点类型 + 一致性测试 + 集成测试）

## Immediate known tech debt

- **Architecture review 派生 P1/P2**（2026-04-29，~~已偿还~~ 2026-05-03）：~~变量控制事件从 `ExecutionEvent` 拆出~~（已偿还：`WorkflowVariableEvent` 独立枚举 + 独立通道，B1-R0-01/B1-R0-05）；~~`src/graph/` 触发 ADR-0020 重评~~（已偿还，2026-05-01 拆为 `crates/graph/`）；~~Rhai `max_operations` 增加统一 clamp~~（已偿还：`scripting::default_max_operations()` 统一，D-01）；~~`NodeOutput.metadata` 显式三值语义~~（已偿还：`Map` → `Option<Map>`，B1-R0-02）；~~前端大文件拆分~~（已偿还：FlowgramCanvas 2025→988 行 / ConnectionStudio 1824→1372 行，C-02）；~~`workflow.rs` 单文件过~~大（已偿还：拆为 `workflow_deploy/dispatch/undeploy` 三模块，C-01）。**剩余**：runtime / dead-letter / scoped event 等 IPC 类型迁入 `tauri-bindings`（B4-IPC-02/03，17 类型已迁入定义，壳层 import 替换待后续）；core/connections/ai crate AGENTS.md 已同步（B2-R1-03/04）。详见 `docs/superpowers/specs/2026-04-29-architecture-review-findings.md`。
- **ADR-0016 deferred items**（2026-04-30）：`BackpressureDetected` 发射逻辑；`payload_bytes` 统计（需序列化测量）；`received_at` 精确测量（需 instrument 接收端）；100ms 定时窗口 flush（当前每执行周期 flush）；`queue_depth` 精确值（需共享 channel 状态）；前端边热力图 UI。
- ~~**ADR-0013 子图实施 deployment 断链**（2026-04-28 发现）~~ **已偿还（2026-04-28）**。merge 68ab709 解决冲突时丢失的 ADR-0013 改动重写恢复——`flattenSubgraphs` + Rust `mod passthrough` 注册 + `FlowgramCanvas` 容器/桥接渲染 + 设置面板全部到位，三件套全绿。loop 容器恢复已并入当前 `main`。
- ~~MQTT subscriber / Timer / Serial root lifecycle is owned by the Tauri shell.~~ **已偿还（2026-04-26，ADR-0009 已实施）**。三类触发器节点现自持 `on_deploy` + `LifecycleGuard`；壳层不再监督触发器任务。**语义变化**：触发器节点走 `NodeHandle::emit` 而非 `dispatch_router` 的 trigger lane，失去 backpressure / DLQ / retry / metrics 防御能力，等 ADR-0014 / ADR-0016 引擎级背压能力补回。
- ~~IPC response types in `crates/core/` contradict Ring 0 purity. ADR-0017 plans to extract `crates/tauri-bindings/`.~~ **已偿还（2026-04-24，ADR-0017 已实施）**
- ~~`cargo clippy --workspace --all-targets -- -D warnings` 在 `src-tauri` 与 observability 上失败~~ **已偿还（2026-04-26，见 `docs/superpowers/plans/2026-04-25-cargo-clippy-workspace-fixes.md`）**。`crates/nodes-io/src/http_client.rs` / `bark_push.rs` 的 `too_many_lines` 现以 `#[allow]` 抑制（同上）。

## RFC-0004 Phase 3

**已完成**（2026-05-03）。Workflow DSL 编译器全部就位：
- `crates/dsl-compiler/` — `WorkflowSpec` → `WorkflowGraph` JSON 编译器（引用校验 + 语义校验 + JSON 生成）
- `crates/nodes-flow/src/state_machine.rs` — `stateMachine` 节点（动态 output pins + Rhai 条件评估 + `NodeDispatch::Route`）
- `crates/nodes-io/src/capability_call.rs` — `capabilityCall` 节点（编译期快照 + 模板解析 + 协议执行）
- 4 个一致性测试（`WorkflowGraph::from_json()` 守护 schema 漂移）+ 集成测试
- 节点总数 22（+2），注册表合约测试已更新

## RFC-0004 Phase 4

**已完成 4A/4B/4C**（2026-05-03）。AI 生成管道对接：
- 4A：设备/能力 AI 结构化提取提案 — `extract_device_proposal` / `extract_device_proposal_stream`（JSON 输出含 uncertainties + warnings）+ 前端 proposal 流程
- 4B：DSL 编译器 IPC — `compile_workflow_dsl` / `load_compiler_asset_snapshot` + 前端 DSL 编辑器（YAML textarea + 编译反馈 + 资产快照）
- 4C：AI 编排控制台 — `ai_generate_workflow_dsl` / `ai_generate_workflow_dsl_stream`（NL 目标 → Workflow DSL + 自动编译 + 不确定项标记）+ 前端三栏编排页面 + `use-dsl-orchestrator` hook
- 4D 待完成：多轮优化、安全编译器对接（待 Phase 5）

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
> 9b. ✅ **ADR-0016** 边级可观测性 — **已接受，部分实施**（2026-04-30）。`EdgeTransmitSummary` 已发射；`BackpressureDetected` 发射 deferred。
> 10. 真实协议驱动扩展（OPC-UA、Kafka 消费者等）
> 11. AI 能力扩展（embeddings、vision，未来 ADR）

## 评估性 ADR

- ADR-0020 `src/graph/` 编排层归属 — **已实施**（2026-05-01，拆为 `crates/graph/` Ring 1 crate，依赖 `nazh-core` + `connections`）。
