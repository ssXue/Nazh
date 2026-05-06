# 2026-04-29 IPC 边界审计 findings

**范围**：`src-tauri/src/lib.rs`、`src-tauri/src/observability.rs`、`crates/tauri-bindings/src/lib.rs`、前端 `web/src/lib/tauri.ts` IPC wrapper。

**结论**：`workflow://*` 事件 channel 命名基本一致，`VariableChangedPayload` 已在 tauri-bindings 中独立建模；但 Tauri 命令和跨 IPC 类型远多于 root AGENTS 当前描述，且大量稳定请求/响应类型仍留在 `src-tauri/src/lib.rs` 私有 struct 中，ts-rs 单一契约边界不完整。

## IPC 命令清单

取证位置：`src-tauri/src/lib.rs:2573` 的 `tauri::generate_handler!`。

| 域 | 命令 |
|----|------|
| 工作流生命周期 | `deploy_workflow`, `dispatch_payload`, `undeploy_workflow`, `list_runtime_workflows`, `set_active_runtime_workflow`, `list_dead_letters` |
| 节点 / pin | `list_node_types`, `describe_node_pins` |
| 变量 | `snapshot_workflow_variables`, `set_workflow_variable` |
| 连接 | `list_connections`, `load_connection_definitions`, `save_connection_definitions` |
| 可观测性 | `query_observability` |
| 部署会话文件 | `load_deployment_session_file`, `load_deployment_session_state_file`, `list_deployment_sessions_file`, `save_deployment_session_file`, `set_deployment_session_active_project_file`, `remove_deployment_session_file`, `clear_deployment_session_file` |
| 串口 | `list_serial_ports`, `test_serial_connection` |
| 工程库 / 导出 | `load_project_board_files`, `save_project_board_files`, `save_flowgram_export_file` |
| AI | `load_ai_config`, `save_ai_config`, `test_ai_provider`, `copilot_complete`, `copilot_complete_stream` |

当前总数：30 个。

## 事件 channel 清单

| channel | payload 来源 | 评估 |
|---------|--------------|------|
| `workflow://node-status` | `ScopedExecutionEvent { workflow_id, event }` | 命名一致；不包含 `VariableChanged`。 |
| `workflow://result` | `ScopedWorkflowResult { workflow_id, result }` | 命名一致。 |
| `workflow://deployed` | `DeployResponse` | 命名一致。 |
| `workflow://undeployed` | `UndeployResponse` | 命名一致。 |
| `workflow://runtime-focus` | `RuntimeWorkflowSummary` | 命名一致。 |
| `workflow://variable-changed` | `VariableChangedPayload` | 正确与 node-status 分离。 |
| `copilot://stream/{stream_id}` | dynamic stream chunk / error payload | 单独 namespace 合理，但 root AGENTS 未列。 |

## 主要 findings

| ID | 优先级 | 位置 | 发现 | 建议动作 |
|----|--------|------|------|----------|
| B4-IPC-01 | P1 | `AGENTS.md` IPC Surface 段 | root AGENTS 写 “~24 commands” 且列出旧 AI 命令名（如 `list_ai_providers` / `save_ai_provider` / `delete_ai_provider`），实际 `generate_handler!` 是 30 个命令且 AI surface 为 `load_ai_config` / `save_ai_config` / `test_ai_provider` / `copilot_complete` / `copilot_complete_stream`。 | Phase E 更新 root AGENTS + README IPC 表。 |
| B4-IPC-02 | P1 | `src-tauri/src/lib.rs:75` 起 | 多个前端可见 IPC 类型仍在 shell 私有文件：`RuntimeWorkflowPolicy*`、`RuntimeWorkflowSummary`、`DeadLetterRecord`、`ProjectWorkspace*`、`PersistedDeploymentSession*`、`ConnectionDefinitionsLoadResult`、`SerialPortInfo`、`TestSerialResult`、`ScopedExecutionEvent`、`ScopedWorkflowResult`。 | 分批迁入 `crates/tauri-bindings` 并 ts-rs 导出；优先迁移 runtime summary / scoped events / dead letters。 |
| B4-IPC-03 | P1 | `src-tauri/src/observability.rs:25` | `ObservabilityContextInput` / `ObservabilityQueryResult` 等跨 IPC 类型在 `observability.rs`，前端手写镜像类型。 | 迁入 `tauri-bindings` 或独立 observability bindings，减少手写漂移。 |
| B4-IPC-04 | P1 | `crates/core/src/event.rs:24`, `crates/tauri-bindings/src/lib.rs:144` | `VariableChangedPayload` 已是独立事件 payload，但 `ExecutionEvent` generated union 仍包含 `VariableChanged`。前端 `parseWorkflowEventPayload` 忽略该 variant，因为它不会走 `workflow://node-status`。 | 与 B1-R0-01 一起拆核心事件；短期在前端/AGENTS 明确 “node-status 不发送 VariableChanged”。 |
| B4-IPC-05 | P2 | `crates/tauri-bindings/src/lib.rs:52` | `UndeployResponse.aborted_timer_count` 字段名沿用历史语义，实际是 shutdown lifecycle guard 数。代码注释说明了兼容原因，但 TS 侧读者仍易误解。 | 不改字段；在 generated consumer 文档或 README IPC 表标注 “legacy name”。 |
| B4-IPC-06 | P2 | `src-tauri/src/lib.rs:1744` | `copilot_complete_stream` 通过动态 event name 返回 chunk，payload 没有 tauri-bindings 类型，错误 payload 与正常 chunk shape 混合。 | 若流式接口稳定，新增 `CopilotStreamChunkPayload` 到 tauri-bindings。 |

## 应迁入 `tauri-bindings` 的类型优先级

| 优先级 | 类型 | 原因 |
|--------|------|------|
| P1 | `RuntimeWorkflowSummary`, `WorkflowRuntimePolicy`, `DispatchLaneSnapshot` | 前端 runtime 列表和 focus event 都依赖；属于稳定 IPC surface。 |
| P1 | `ScopedExecutionEvent`, `ScopedWorkflowResult` | `workflow://node-status` / `workflow://result` payload；当前前端手写 wrapper。 |
| P1 | `DeadLetterRecord` | `list_dead_letters` 返回值；字段较多，手写漂移风险高。 |
| P1 | `ProjectWorkspaceStorageInfo`, `ProjectWorkspaceLoadResult`, `ProjectWorkspaceBoardFile`, `SavedWorkspaceFile` | 工程库核心 IPC 类型。 |
| P2 | `PersistedDeploymentSession*` | 文件持久化 schema + IPC 类型混合；迁移前先确认版本字段策略。 |
| P2 | `SerialPortInfo`, `TestSerialResult` | 小类型，迁移成本低。 |
| P2 | `Observability*` | 体量较大，建议独立 PR。 |

## 与 `AGENTS.md` 对照

root AGENTS 当前事件列表已包含 6 个 `workflow://*` channel，基本准确；命令列表和 AI 命令名明显过期。该项应作为 Phase E 文档同步任务，不建议在 Phase B 改代码。
