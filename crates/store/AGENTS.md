# crates/store — 持久化 Ring 1 crate（ADR-0022 + RFC-0004 Phase 1-2）

## 定位

SQLite 持久化层，实现 RFC-0003 Phase 1 子集 + RFC-0004 设备/能力资产存储。存储七类数据：

- **工作流变量**：按 `workflow_id + key` 存储当前值与声明初值
- **变量历史**：每次写入追加一条变更记录（时间序列）
- **全局变量**：按 `namespace + key` 跨工作流共享
- **设备资产**：按 `id` 存储设备 DSL 规格，自动版本化
- **设备资产来源**：AI 抽取的字段级来源追溯
- **能力资产**：按 `id` 存储能力 DSL 规格，绑定设备资产，自动版本化（Phase 2）
- **能力来源**：能力字段的来源追溯（Phase 2）

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

// device_assets.rs（RFC-0004 Phase 1）
pub struct DeviceAssetSummary { id, name, device_type, version, updated_at }
pub struct StoredDeviceAsset { id, name, device_type, version, spec_json, created_at, updated_at }
pub struct StoredAssetVersion { asset_id, version, spec_json, source_summary, created_at }
pub struct AssetVersionSummary { version, created_at, source_summary }
pub struct FieldSource { field_path, source_text, confidence }

pub fn save_device_asset(id, name, device_type, spec_json) -> Result<(), StoreError>;
pub fn load_device_asset(id) -> Result<Option<StoredDeviceAsset>, StoreError>;
pub fn list_device_assets() -> Result<Vec<DeviceAssetSummary>, StoreError>;
pub fn delete_device_asset(id) -> Result<(), StoreError>;     // 级联删除版本+来源
pub fn list_asset_versions(asset_id) -> Result<Vec<AssetVersionSummary>, StoreError>;
pub fn load_asset_version(asset_id, version) -> Result<Option<StoredAssetVersion>, StoreError>;
pub fn save_asset_sources(asset_id, sources: &[FieldSource]) -> Result<(), StoreError>;
pub fn load_asset_sources(asset_id) -> Result<Vec<FieldSource>, StoreError>;

// capabilities.rs（RFC-0004 Phase 2）
pub struct CapabilitySummary { id, device_id, name, description, version, updated_at }
pub struct StoredCapability { id, device_id, name, description, version, spec_json, created_at, updated_at }
pub struct StoredCapabilityVersion { capability_id, version, spec_json, source_summary, created_at }
pub struct CapabilityVersionSummary { version, created_at, source_summary }
pub struct CapabilitySource { field_path, source_text, confidence }

pub fn save_capability(id, device_id, name, description, spec_json) -> Result<(), StoreError>;
pub fn load_capability(id) -> Result<Option<StoredCapability>, StoreError>;
pub fn list_capabilities(device_id: Option<&str>) -> Result<Vec<CapabilitySummary>, StoreError>;
pub fn delete_capability(id) -> Result<(), StoreError>;           // 级联删除版本+来源
pub fn list_capability_versions(capability_id) -> Result<Vec<CapabilityVersionSummary>, StoreError>;
pub fn load_capability_version(capability_id, version) -> Result<Option<StoredCapabilityVersion>, StoreError>;
pub fn save_capability_sources(capability_id, sources: &[CapabilitySource]) -> Result<(), StoreError>;
pub fn load_capability_sources(capability_id) -> Result<Vec<CapabilitySource>, StoreError>;
```

## 内部约定

- **线程安全**：`Connection` 由 `std::sync::Mutex` 保护，`db()` 返回 `MutexGuard`。所有公开方法内部获取锁，调用方无需额外同步。
- **Migration**：内联 SQL，`001` 建变量表、`002` 建设备资产表、`003` 建能力资产表。后续新增 migration 在 `migrations.rs` 追加即可。
- **类型**：所有值以 `serde_json::Value` 存取，JSON TEXT 列。`var_type` / `updated_by` 纯字符串，不关联 Rust 枚举。
- **`clippy::too_many_arguments`**：`upsert_variable` / `record_history` / `query_history_range` / `upsert_global` 已 `#[allow]`，参数为 persistence 必要字段，不宜封装为 struct（与 IPC 层类型解耦）。

## 修改本 crate 时

- 新增 migration：在 `migrations.rs` 的 `MIGRATIONS` 数组追加 `(version, sql)` 元组。
- 新增表/查询：新建 `src/xxx.rs` 模块，在 `lib.rs` re-export。同步更新 `AGENTS.md` 的「对外暴露」列表。
- 测试：每个模块内 `#[cfg(test)] mod tests`，使用 `Store::open_in_memory()` 避免文件副作用。
