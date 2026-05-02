# crates/tauri-bindings — IPC 契约与 ts-rs 导出汇总

> **Ring**: IPC 层（Tauri 壳层与前端的契约边界，不是 Ring 0 / Ring 1 的一部分）
> **对外 crate 名**: `tauri-bindings`
> **职责**: 定义 Tauri IPC 命令的请求/响应类型 + 作为 ts-rs 全工作区导出的汇总入口
>
> 根 `AGENTS.md` 的约束对本 crate 同样适用。

## 这个 crate 做什么

本 crate 是 **Ring 0 与 Tauri 壳层之间的适配层**（ADR-0017 的直接产物）。存在意义有二：

1. **IPC 响应类型**：`DeployResponse` / `DispatchResponse` / `UndeployResponse` / `NodeTypeEntry` / `ListNodeTypesResponse` / `DescribeNodePinsRequest` / `DescribeNodePinsResponse` —— 它们只服务于桌面壳层与前端的契约，不属于引擎运行时。从前住在 `nazh-core::ipc`，ADR-0017 决策后迁出，避免污染 Ring 0。`DescribeNodePinsRequest/Response` 由 ADR-0010 Phase 2 引入（2026-04-26），服务前端连接期 pin 类型校验（FlowGram `canAddLine` 钩子读 pin schema 缓存）。
2. **ts-rs 汇总入口**：`export_all()` 按 feature `ts-export` 触发全工作区（`nazh-core` / `connections` / `ai` / `nazh-engine` + 本 crate）的 TypeScript 类型导出到 `web/src/generated/`。CI 用 `cargo test -p tauri-bindings --features ts-export export_bindings` 验证类型契约。

还提供辅助函数：`list_node_types_response(&NodeRegistry) -> ListNodeTypesResponse`，负责从注册表读节点类型名称、排序、携带能力标签位图，封装给前端。

## 对外暴露

```text
crates/tauri-bindings/src/
└── lib.rs    # IPC 类型 + list_node_types_response + export_all
```

关键类型与函数：
- `DeployResponse` / `DispatchResponse` / `UndeployResponse` — `src/lib.rs:17+`
- `NodeTypeEntry` / `ListNodeTypesResponse` — `src/lib.rs:59+`
- `SnapshotWorkflowVariablesRequest` / `SnapshotWorkflowVariablesResponse` — ADR-0012 Phase 1 引入，供 `snapshot_workflow_variables` IPC 命令序列化；包含对 `TypedVariableSnapshot` / `VariableDeclaration`（来自 `nazh-core`，ADR-0012 Phase 1 引入）的透传
- `SetWorkflowVariableRequest` / `SetWorkflowVariableResponse`（ADR-0012 Phase 2）— 供 `set_workflow_variable` IPC 命令序列化；`Response` 含写入后读回的 `TypedVariableSnapshot`（类型不匹配 / 变量未声明通过 `Err(String)` 上抛）
- `DeleteWorkflowVariableRequest` / `ResetWorkflowVariableRequest` / `QueryVariableHistoryRequest` / `SetGlobalVariableRequest` / `GetGlobalVariableRequest` / `ListGlobalVariablesRequest` / `DeleteGlobalVariableRequest` 及对应响应类型（ADR-0012 Phase 3 / ADR-0022）— 供变量删除、重置、历史查询与全局变量 CRUD IPC 命令序列化
- `VariableChangedPayload`（ADR-0012 Phase 2）— `workflow://variable-changed` 事件载荷；包含 `workflow_id` / `name` / `value` / `updated_at` / `updated_by`；`updated_by` 加 `#[serde(skip_serializing_if = "Option::is_none")]` 与 ts(optional) 契约对齐
- `list_node_types_response(&NodeRegistry)` — `src/lib.rs:85`
- `export_all()`（仅 `ts-export` feature） — `src/lib.rs:107`

## 内部约定

1. **这里只放「跨进程类型」**。如果某类型只是 Rust 内部用（不会序列化发给前端），放回对应业务 crate，不要堆在本 crate。
2. **所有对外类型都有 `#[cfg_attr(feature = "ts-export", derive(TS), ts(export))]`**。加新类型时不漏 attribute；漏了 `export_all()` 不会导出，前端拿不到。
3. **`export_all()` 是 single source of truth**。所有要导出的类型（包括依赖 crate 的）都要在此函数里点名调用 `::export(&cfg)?`；`cfg` 来自 `ts_rs::Config::from_env()`，以便遵守 CI/本地设置的导出目录。若只加了 `derive(TS)` 忘了加到此函数，CI 的 `git diff --exit-code -- web/src/generated/` 会挂。
4. **camelCase 序列化**：所有 struct 用 `#[serde(rename_all = "camelCase")]` 对齐前端命名。
5. **`NodeTypeEntry.capabilities` 是 `u32` 位图**。ts 类型为 `number`，前端 `web/src/lib/node-capabilities.ts` 解位。
6. **`Option` 字段标 `#[cfg_attr(feature = "ts-export", ts(optional))]`**，让前端的 `undefined` 而不是 `null` 成为约定。

## 依赖约束

- 允许：`nazh-core`、`connections`、`ai`、`nazh-engine`（facade）、`serde`、`ts-rs`（optional）
- 禁止：协议 crate（`reqwest` / `rumqttc` / `rusqlite` / `tokio-modbus`）、`nodes-*`

这个 crate 是**出口**，是唯一允许同时依赖 `nazh-core` / `connections` / `ai` / 引擎 facade 的 crate。这种"汇总"定位让 ts-rs 导出能一步到位，同时避免业务 crate 互相引用污染 Ring 纯度。

## 修改本 crate 时

| 改动 | 必须同步 |
|------|----------|
| 新增 IPC 响应类型 | 加 `#[cfg_attr(..., derive(TS), ts(export))]` + 在 `export_all()` 里点名 + Tauri 壳层 `src-tauri/src/lib.rs` 新 IPC 命令 + 前端调用方 |
| 改 `NodeTypeEntry` 字段 | ts-rs 重新生成 + 前端 `types.ts` / `PluginPanel` 等消费者 |
| 改 ts-export feature 传递链 | 根 `Cargo.toml` `ts-export` feature 清单 + 相关 crate 的 feature 配置 |
| 迁出/迁入类型 | 开 ADR 讨论边界变动（参考 ADR-0017 做法） |

测试：
```bash
cargo test -p tauri-bindings
cargo test -p tauri-bindings --features ts-export export_bindings
```

导出完毕后 `git diff web/src/generated/` 看生成物是否符合预期；前端跑 `tsc --noEmit` 确认消费端接得住。

## 关联 ADR / RFC

- **ADR-0003** Tauri IPC 不用 HTTP（本 crate 存在的前提）
- **ADR-0007** ts-rs 前后端类型契约守卫
- **ADR-0017** IPC + ts-rs 迁出 Ring 0（本 crate 的直接来源）
