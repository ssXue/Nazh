//! 工程工作路径下的 DSL 资产文件存储。
//!
//! Connection / Device / Capability 资产以 `YAML` 文件为唯一持久化真值源。
//! `SQLite` 仅服务变量、历史、全局变量和本机私有配置，不保存工程资产索引。

pub(crate) mod io;
pub(crate) mod snapshots;
pub(crate) mod sources;
pub(crate) mod types;
pub(crate) mod versioning;

pub(crate) use io::*;
pub(crate) use snapshots::*;
pub(crate) use sources::*;
pub(crate) use types::{AssetFieldSource, DeviceSnapshotMeta, SnapshotReason, file_modified_at};
pub(crate) use versioning::*;

#[cfg(test)]
mod tests {
    use super::types::sanitize_asset_file_stem;

    #[test]
    fn 资产文件名只保留安全字符() {
        assert_eq!(sanitize_asset_file_stem(" press/轴 1 "), "press___1");
        assert_eq!(sanitize_asset_file_stem("../"), "asset");
    }
}
