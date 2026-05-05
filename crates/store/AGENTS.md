# crates/store — 持久化 Ring 1 crate（ADR-0022）

## 定位

SQLite 持久化层，实现 RFC-0003 Phase 1 子集与 ADR-0022 工作流变量持久化。Device / Capability DSL 资产不再进入本 crate；Tauri 壳层直接以工程工作路径下的 YAML 文件作为唯一真值源。当前存储三类数据：

- **工作流变量**：按 `workflow_id + key` 存储当前值与声明初值
- **变量历史**：每次写入追加一条变更记录（时间序列）
- **全局变量**：按 `namespace + key` 跨工作流共享
Ring 1 crate，仅依赖 `rusqlite` / `serde` / `chrono` / `tracing`，不依赖 `nazh-core` 或任何其他 workspace crate。

## 对外暴露

```rust
pub struct Store { /* SQLite Connection，Mutex 保护 */ }

impl Store {
    pub fn open(path: &Path) -> Result<Store, StoreError>;
    pub fn open_unpersisted() -> Store;                        // 内存模式，不写盘
    #[cfg(test)]
    pub fn open_in_memory() -> Result<Store, StoreError>;      // 同 open_unpersisted
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

```

## 内部约定

- **线程安全**：`Connection` 由 `std::sync::Mutex` 保护，`db()` 返回 `MutexGuard`。所有公开方法内部获取锁，调用方无需额外同步。
- **Migration**：内联 SQL，`001` 建变量表、变量历史表与全局变量表。后续新增 migration 在 `migrations.rs` 追加即可。
- **类型**：所有值以 `serde_json::Value` 存取，JSON TEXT 列。`var_type` / `updated_by` 纯字符串，不关联 Rust 枚举。
- **资产边界**：不要把 Device / Capability DSL 表重新加回本 crate；设备/能力资产由 `src-tauri/src/asset_files.rs` 读写工作路径 YAML。
- **`clippy::too_many_arguments`**：`upsert_variable` / `record_history` / `query_history_range` / `upsert_global` 已 `#[allow]`，参数为 persistence 必要字段，不宜封装为 struct（与 IPC 层类型解耦）。

## 修改本 crate 时

- 新增 migration：在 `migrations.rs` 的 `MIGRATIONS` 数组追加 `(version, sql)` 元组。
- 新增表/查询：新建 `src/xxx.rs` 模块，在 `lib.rs` re-export。同步更新 `AGENTS.md` 的「对外暴露」列表。
- 测试：每个模块内 `#[cfg(test)] mod tests`，使用 `Store::open_in_memory()` 避免文件副作用。
