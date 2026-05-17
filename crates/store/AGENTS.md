# crates/store — 持久化 Ring 1 crate（ADR-0022）

## 定位

SQLite 持久化层，实现 RFC-0003 Phase 1/2 与 Phase 3 部署审计子集。Device / Capability DSL 资产不再进入本 crate；Tauri 壳层直接以工程工作路径下的 YAML 文件作为唯一真值源。当前存储五类数据：

- **工作流变量**：按 `workflow_id + key` 存储当前值与声明初值
- **变量历史**：每次写入追加一条变更记录（时间序列）
- **全局变量**：按 `namespace + key` 跨工作流共享
- **可观测性索引**：`observability_records` 保存事件/审计/告警的查询索引 + 原始 JSON payload（唯一存储后端，JSONL 双写已移除）
- **部署审计**：`deployment_audit` 记录 deploy / undeploy 生命周期动作
Ring 1 crate，仅依赖 `rusqlite` / `serde` / `chrono` / `tracing`，不依赖 `nazh-core` 或任何其他 workspace crate。
`tokio` 仅用于 `StoreHandle` 的 `spawn_blocking` async 边界，不进入 SQL/schema 逻辑。

## 对外暴露

```rust
pub struct Store { /* SQLite Connection，Mutex 保护 */ }
pub struct StoreHandle { /* Arc<Store> + spawn_blocking async 边界 */ }

impl Store {
    pub fn open(path: &Path) -> Result<Store, StoreError>;
    pub fn open_unpersisted() -> Result<Store, StoreError>;     // 内存模式，不写盘
    #[cfg(test)]
    pub fn open_in_memory() -> Result<Store, StoreError>;      // 同 open_unpersisted
}

impl StoreHandle {
    pub fn new(store: Store) -> StoreHandle;
    pub async fn run_blocking(...) -> Result<T, StoreError>;
    // async CRUD mirrors: load_variables / upsert_variable / query_latest / upsert_global / ...
    // observability mirrors: insert_observability_record / query_observability_records / clear_observability_records
    // deployment audit mirrors: insert_deployment_audit / list_deployment_audit
}

// variables.rs
pub fn upsert_variable(...) -> Result<(), StoreError>;
pub fn load_variables(workflow_id) -> Result<Vec<StoredVariable>, StoreError>;
pub fn delete_variable(workflow_id, key) -> Result<(), StoreError>;
pub fn delete_all_variables(workflow_id) -> Result<(), StoreError>;

// history.rs
pub fn record_history(...) -> Result<(), StoreError>;
pub fn query_latest(workflow_id, key, limit) -> Result<Vec<HistoryEntry>, StoreError>;
pub fn query_history_range(...) -> Result<Vec<HistoryEntry>, StoreError>;

// global_variables.rs
pub fn upsert_global(...) -> Result<(), StoreError>;
pub fn load_global(namespace, key) -> Result<Option<StoredGlobalVariable>, StoreError>;
pub fn list_globals(namespace?) -> Result<Vec<StoredGlobalVariable>, StoreError>;
pub fn delete_global(namespace, key) -> Result<(), StoreError>;

// observability.rs
pub fn insert_observability_record(...) -> Result<(), StoreError>;
pub fn query_observability_records(...) -> Result<Vec<StoredObservabilityRecord>, StoreError>;
pub fn clear_observability_records() -> Result<(), StoreError>;

// deployment_audit.rs
pub fn insert_deployment_audit(record) -> Result<(), StoreError>;
pub fn list_deployment_audit(workflow_id, limit) -> Result<Vec<DeploymentAuditRecord>, StoreError>;

```

## 内部约定

- **线程安全**：`Connection` 由 `std::sync::Mutex` 保护，`db()` 返回 `MutexGuard`。所有公开方法内部获取锁，调用方无需额外同步。async 调用方必须经 `StoreHandle`，不要在 async worker 上直接调用同步 CRUD。
- **Migration**：内联 SQL，`001` 建变量表、变量历史表与全局变量表；`004/006` 建 copilot 对话与 thinking 字段；`007` 建可观测性索引与部署审计表。migration 在事务中执行；只有 `schema_version` 表不存在时才 bootstrap，其它 `rusqlite` 错误必须原样返回。后续新增 migration 在 `migrations.rs` 追加即可。
- **类型**：所有值以 `serde_json::Value` 存取，JSON TEXT 列。`var_type` / `updated_by` 纯字符串，不关联 Rust 枚举。
- **资产边界**：不要把 Device / Capability DSL 表重新加回本 crate；设备/能力资产由 `src-tauri/src/asset_files.rs` 读写工作路径 YAML。
- **`clippy::too_many_arguments`**：`upsert_variable` / `record_history` / `query_history_range` / `upsert_global` 已 `#[allow]`，参数为 persistence 必要字段，不宜封装为 struct（与 IPC 层类型解耦）。

## 修改本 crate 时

- 新增 migration：在 `migrations.rs` 的 `MIGRATIONS` 数组追加 `(version, sql)` 元组。
- 新增表/查询：新建 `src/xxx.rs` 模块，在 `lib.rs` re-export。同步更新 `AGENTS.md` 的「对外暴露」列表。
- 测试：每个模块内 `#[cfg(test)] mod tests`，使用 `Store::open_in_memory()` 避免文件副作用。
- Tauri / runtime async 路径新增 Store 调用时，同步 `Store` 方法和 `StoreHandle` async 包装要一起补齐，并增加 async 边界测试。
