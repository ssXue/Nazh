# 2026-04-29 架构 review 总 findings

**范围**：整合 Phase B 的 5 份切片审计、Phase C 模块拆分与行数普查、Phase D 规范扫描结果。  
**结论（2026-04-30 校准）**：Phase A/B/C/D/E 已完成，架构冻结已解除；`src-tauri/src/lib.rs` 已从 2675 行拆到 132 行，前端 FlowGram 保存时丢 `mqttClient` / pure-form 节点的 P0 已修复。ADR-0016 的 `BackpressureDetected` 发射逻辑等 deferred items 仍是后续技术债，但不再阻塞常规 PR 流程。

## Phase C：行数与拆分结果

`tokei` 在当前环境不可用，本轮用等价 `find ... wc -l` 普查，排除了 `src-tauri/target` 构建产物。

### Rust > 500 行清单

| 文件 | 行数 | 决策 |
|------|------|------|
| `crates/connections/src/lib.rs` | 1243 | P1：按 guard / health / circuit-breaker / metadata 拆分；同步 `crates/connections/AGENTS.md`。 |
| `src-tauri/src/observability.rs` | 956 | P1：按 event/audit/alert/query/store 拆分；跨 IPC 类型迁入 `tauri-bindings` 时一起做。 |
| `crates/nodes-io/src/serial_trigger.rs` | 858 | P2：串口生命周期与帧解析可拆，但当前内聚，等协议节点 polish PR。 |
| `crates/nodes-io/src/mqtt_client.rs` | 730 | P2：publish / subscribe / lifecycle helper 可拆，等 Phase 6/EventBus 背压策略稳定后处理。 |
| `crates/ai/src/client.rs` | 717 | P2：流式解析、provider request、错误归一化可拆；非本轮阻塞项。 |
| `crates/core/src/variables.rs` | 698 | P1：与 `ExecutionEvent::VariableChanged` 解耦时拆 declaration/mutation/event bridge。 |
| `src-tauri/src/runtime.rs` | 630 | P1：本轮从 `lib.rs` 迁出；后续可继续拆 policy / dead-letter / dispatch。 |
| `src-tauri/src/commands/workflow.rs` | 604 | P1：本轮从 `lib.rs` 迁出；后续可把 SQL path normalization / deploy event wiring 拆子模块。 |
| `crates/nodes-io/src/bark_push.rs` | 565 | P2：请求构造 / metadata 构造可拆。 |
| `crates/core/src/pin.rs` | 557 | P2：下一次新增 pin 工厂方法时改 builder。 |
| `crates/core/src/plugin.rs` | 536 | P2：测试与 registry internals 可拆。 |
| `crates/scripting/src/lib.rs` | 532 | P2：package / ai bridge / vars bridge 已有边界，可再拆文件。 |
| `crates/nodes-io/src/http_client.rs` | 523 | P2：请求构造 / 响应 metadata 可拆。 |
| `crates/ai/src/config.rs` | 516 | P2：provider config persistence 可拆。 |
| `src/graph/pin_validator.rs` | 513 | P1：测试 helper 与 edge semantic 校验拆分；与 ADR-0020 后续一起处理。 |

### TypeScript > 500 行清单

| 文件 | 行数 | 决策 |
|------|------|------|
| `web/src/components/FlowgramCanvas.tsx` | 2024 | P1：画布渲染、事件订阅、节点卡片、minimap/selection 拆分。 |
| `web/src/components/ConnectionStudio.tsx` | 1824 | P1：连接列表、编辑器、测试面板拆分。 |
| `web/src/lib/projects.ts` | 1601 | P1：工程库、部署会话、导入导出拆分。 |
| `web/src/lib/workflow-orchestrator.ts` | 1193 | P1：runtime events、deploy/dispatch/undeploy、state reducer 拆分。 |
| `web/src/components/app/AiConfigPanel.tsx` | 1034 | P2：provider form / test panel / persistence 拆分。 |
| `web/src/components/flowgram/nodes/shared.ts` | 860 | P1：node kind、config normalize、pin helpers、settings helpers 拆分。 |
| `web/src/lib/tauri.ts` | 850 | P1：按 IPC 域拆 wrapper，并用 `tauri-bindings` 生成类型替换手写镜像。 |
| `web/src/components/app/AppIcons.tsx` | 791 | P2：按图标域拆。 |
| `web/src/components/app/RuntimeManagerPanel.tsx` | 746 | P2：runtime list 与 dead-letter 子面板拆。 |
| `web/src/components/flowgram/FlowgramNodeSettingsPanel.tsx` | 707 | P1：按 node settings registry 拆。 |
| `web/src/hooks/use-deployment-restore.ts` | 664 | P2：部署会话恢复策略拆 helper。 |
| `web/src/components/app/LogsPanel.tsx` | 659 | P2：query/filter/render 拆。 |
| `web/src/App.tsx` | 594 | P1：应用壳状态与路由拆。 |
| `web/src/hooks/use-project-library.ts` | 517 | P2：project storage actions 拆。 |

### 已完成拆分

- `src-tauri/src/lib.rs` 现在只负责模块注册、窗口效果、startup setup 与 `generate_handler!`，132 行。
- 新模块：`src-tauri/src/commands/*`、`events.rs`、`runtime.rs`、`state.rs`、`registry.rs`、`workspace.rs`、`util.rs`。
- 同步修复：`native` 节点 payload 键从 `_native_message` 改为 `native_message`，避免违反“payload 中下划线键仅 `_loop` / `_error`”约定。
- 同步修复：FlowGram 保存业务节点集合改为从 `getAllNodeDefinitions()` 派生，避免新增节点时漏维护白名单。

## Phase D：规范扫描结果

| 项 | 结果 | 说明 |
|----|------|------|
| `.unwrap()` / `.expect()` | 生产代码 0 命中 | 原始 `rg` 命中均在 `#[cfg(test)]` 或 integration tests；测试模块已有 `#[allow(clippy::unwrap_used)]`。 |
| `unsafe` | 0 命中 | `unsafe_code = "forbid"` 仍成立。 |
| 节点直接读写 `DataStore` | 0 命中 | `crates/nodes-*` 未出现 `DataStore` 或 `store.read/write`。 |
| metadata 泄漏 payload | 已修 1 项 | `_native_message` 改为 `native_message`；剩余 `_loop` / `_error` 是 AGENTS 允许的路由上下文。 |
| Rhai `max_operations` | 默认 50k | `ScriptNodeBase::new` 调 `engine.set_max_operations(max_operations)`；P1 后续补配置 clamp，避免用户传 0 或过大值。 |
| panic isolation | 符合 | Runner transform 与 ADR-0014 pull pure-form transform 均走 `guarded_execute`；`NodeHandle::emit` 不调用 transform，只做 store + channel。 |
| 直接协议依赖 | 符合 | `tokio_modbus` / `rumqttc` / `reqwest` 只出现在 `nodes-io` 与 `ai`；Ring 0 `cargo tree -p nazh-core` 无协议 crate。 |
| 稳定 type public 字段 | 既有债务 | 已在 B1-R0-07 记录；新稳定类型继续按 private + getters。 |

## 优先级汇总

### P0

| ID | 状态 | 来源 | 发现 | 建议 PR |
|----|------|------|------|---------|
| B5-FE-01 | 已在本轮修复 | Phase B5 | `FLOWGRAM_BUSINESS_NODE_TYPES` 漏 `mqttClient` / `c2f` / `minutesSince`，保存部署会丢节点。 | 已改为从 `getAllNodeDefinitions()` 派生，并补 `flowgram-to-nazh` 回归测试。 |

### P1

| ID | 来源 | 影响面 | 建议 PR 范围 |
|----|------|--------|--------------|
| B1-R0-01 / B4-IPC-04 | Phase B1/B4 | `ExecutionEvent::VariableChanged` 混入执行事件 union。 | 拆 `VariableEvent` / runtime control event，IPC 只导出独立 payload。 |
| B1-R0-02 | Phase B1 | `NodeOutput.metadata` 与 `CompletedExecutionEvent.metadata` 空值语义不对称。 | 引入核心 helper 或统一为同一空值形态。 |
| B1-R0-05 | Phase B1 | `WorkflowVariables` 持有 event sender，存储层与事件桥接耦合。 | 与变量事件拆分同 PR 或相邻 PR。 |
| B2-R1-03 / B2-R1-04 | Phase B2 | crate AGENTS 模块树过期。 | 更新 `connections` / `ai` / `core` crate AGENTS；若拆文件则同 PR 同步。 |
| B3-FAC-01 | Phase B3/C | `src/graph/` 超 ADR-0020 重评线。 | 解冻后新 ADR 决定是否拆 `crates/graph`；短期拆 `types/topology/pin_validator`。 |
| B4-IPC-02 / B4-IPC-03 | Phase B4/C | shell 私有 IPC 类型多，前端手写镜像漂移。 | 分批迁入 `tauri-bindings` 并 ts-rs 导出。 |
| C-01 | Phase C | `src-tauri/src/runtime.rs` / `commands/workflow.rs` 仍 >500 行。 | 二次拆 `runtime/{policy,dead_letter,dispatch}` 与 workflow deploy helpers。 |
| C-02 | Phase C | 前端大文件集中在画布 / project / orchestrator。 | 按域拆组件与 hook，先补测试保护。 |
| D-01 | Phase D | Rhai `max_operations` 没有统一 clamp。 | 在 config normalize / deploy validation 加下限与上限，补测试。 |
| E-01 | Phase E | 已偿还：Phase A/B/C/D/E 全部完成，架构冻结已解除。 | 后续只跟踪 ADR-0016 deferred items，不再阻塞常规 PR。 |

### P2

| ID | 来源 | 影响面 | 建议 |
|----|------|--------|------|
| B1-R0-03 | Phase B1 | `OutputCache` 使用 `DashMap` 可能过度。 | ADR-0014 Phase 4 后按缓存策略重评。 |
| B1-R0-04 | Phase B1 | `PinDefinition` 工厂方法接近膨胀阈值。 | 下一次新增工厂方法时改 builder。 |
| B1-R0-06 | Phase B1 | `AiGenerationParams` provider 扩展字段有膨胀压力。 | 新 provider 专属字段前评估 `provider_options`。 |
| B4-IPC-05 | Phase B4 | `UndeployResponse.aborted_timer_count` 是历史字段名。 | 不改 wire 字段；文档标注 legacy name。 |
| B4-IPC-06 | Phase B4 | `copilot://stream/{id}` payload 无生成类型。 | 稳定后补 `CopilotStreamChunkPayload`。 |
| B5-FE-03 | Phase B5 | generated index 注释里的导出命令过期。 | 下次 ts-rs 生成或后处理时修正。 |
| B5-FE-05 | Phase B5 | 子图多桥接只取第一个。 | 明确非法并加保存前 validation，或设计多 port mapping。 |

## 派生 PR 列表

1. `fix(runtime-events): 拆分变量控制事件与执行事件`
2. `refactor(tauri-bindings): 迁移 runtime / dead-letter / scoped event IPC 类型`
3. `refactor(graph): 拆分 graph types/topology/pin-validator 并重评 ADR-0020`
4. `docs(agents): 同步 crate AGENTS 模块表与 root/README crate 计数`
5. `fix(scripting): clamp Rhai max_operations 并补部署期校验`
6. `refactor(src-tauri): 二次拆 runtime.rs 与 workflow command helpers`
7. `refactor(web): 拆 FlowgramCanvas / workflow-orchestrator / tauri IPC wrapper`
8. ~~`feat(adr-0014): Phase 3b lookup + mixed input`~~ 已完成。
9. ~~`feat(adr-0014): Phase 4 cache lifecycle`~~ 已完成。
10. ~~`feat(adr-0014): Phase 5 visual + AI prompt`~~ 已完成。
11. ~~`feat(rfc-0002): Phase 6 EventBus + EdgeBackpressure`~~ 已完成修订；EventBus broadcast / EdgeBackpressure deferred。
12. ~~`feat(adr-0015/0016): reactive data pin + edge observability`~~ 已完成主体；ADR-0016 deferred items 继续跟踪。

## 验证

- `npm --prefix web run test`：25 files / 235 tests passed。
- `cargo fmt --all -- --check`：通过。
- `cargo clippy --workspace --all-targets -- -D warnings`：通过。
- `cargo test --workspace`：通过。
- `cargo deny check`：exit 0；输出既有 SPDX/duplicate warnings（`unescaper` license 表达式、若干重复依赖），`advisories/bans/licenses/sources ok`。

## 解冻状态

`docs/superpowers/plans/2026-04-28-architecture-review.md` 的退出标准 5 项已全勾，`AGENTS.md` freeze 段已删除，架构冻结于 2026-04-30 解除。本文档剩余 P1/P2 条目按正常 PR 流程推进。
