# ADR-0026 Phase 1：部署期引用收集 + 按需解析连接

**Goal:** 部署工作流时，只解析工作流实际引用的连接资产；缺失引用时 fail fast 并给出可定位错误；部署审计记录连接解析摘要。

**Architecture:** `crates/graph/` 新增 `connection_collector` 模块，从 `WorkflowGraph` + 设备/能力快照中收集 `connection_id` 集合；`src-tauri/src/connection_resolver.rs` 的 `resolve_connection_definitions` 增加 ID 过滤参数；`workflow_deploy` 调整调用链路。

**Tech Stack:** Rust（`crates/graph/` + `src-tauri/`）

**关联：** ADR-0026、`docs/specs/2026-05-23-资产连接绑定收口.md` Phase 2

## 前置条件

- [x] `DeviceSpec.connection: Option<ConnectionRef>` 已存在（`crates/dsl-core/src/device.rs`）
- [x] `resolve_connection_definitions` 已能从工作区解析连接资产（`src-tauri/src/connection_resolver.rs`）
- [x] `capabilityCall` 编译期已从设备继承 `connection_id`（`crates/dsl-compiler/src/output/builder.rs`）

## 步骤

- [ ] **1. 新增 `crates/graph/src/connection_collector.rs`**
  - 公开函数 `collect_referenced_connection_ids(graph: &WorkflowGraph, node_configs: &[Value]) -> Vec<String>`
  - 遍历节点配置，提取 `connection_id`（低层节点的显式连接）
  - 提取 `device_id` → 查设备快照 → 取 `connection.id`（高级节点的设备继承）
  - 去重，返回有序 `Vec<String>`
  - 单元测试：纯显式连接、纯设备继承、混合、重复引用去重、空图

- [ ] **2. `resolve_connection_definitions` 增加 ID 过滤**
  - 新增参数 `connection_ids: Option<&[String]>`
  - `Some(ids)` 时只解析 ID 在集合内的连接资产文件，跳过其余
  - `None` 时保持当前行为（解析全部），保证向后兼容
  - 返回值新增 skipped 计数或由调用方比对
  - 更新现有测试 + 新增过滤测试

- [ ] **3. `deploy_workflow` 调整调用链路**
  - 编译/解析 `WorkflowGraph` 后、注册连接前，调用 `collect_referenced_connection_ids`
  - 将收集到的 ID 集合传给 `resolve_connection_definitions(..., Some(&ids))`
  - 收集到引用但解析不到对应资产时，返回 fail-fast 错误，包含：节点 ID、设备 ID、缺失的 connection_id
  - 部署审计写入 `referenced` / `resolved` / `skipped` 连接数量

- [ ] **4. `deploy_workflow_and_restore_variables` 同步调整**
  - 复用同一引用收集逻辑

- [ ] **5. 测试**
  - `crates/graph/` 单元测试：引用收集覆盖各种场景
  - `src-tauri/tests/` 或 `tests/workflow.rs` 集成测试：部署时工作区存在 2 个连接资产，工作流只引用 1 个，另一个无关坏连接不阻塞部署
  - 集成测试：引用不存在的 connection_id 时部署 fail fast，错误信息包含可定位的 ID

- [ ] **6. 文档同步**
  - 更新 `crates/graph/AGENTS.md`：新增 connection_collector 模块说明
  - 更新 `docs/project-status.md`：ADR-0026 Phase 1 状态
