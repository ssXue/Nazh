//! DSL 资产文件相关类型与常量。

use std::{
    path::{Path, PathBuf},
    time::SystemTime,
};

use chrono::{DateTime, Utc};
use tokio::fs;

pub(crate) const DSL_ASSETS_DIR: &str = "dsl";
pub(crate) const DEVICES_DIR: &str = "devices";
pub(crate) const CAPABILITIES_DIR: &str = "capabilities";
pub(crate) const VERSIONS_DIR: &str = "versions";
pub(crate) const SOURCES_DIR: &str = "sources";
pub(crate) const SNAPSHOTS_SUFFIX: &str = ".snapshots.yaml";
pub(crate) const MAX_SNAPSHOTS: usize = 20;

pub(crate) struct AssetFilePaths {
    pub(crate) latest: PathBuf,
    pub(crate) versioned: PathBuf,
}

#[derive(Debug, Clone)]
pub(crate) struct AssetVersionFile {
    pub(crate) version: i64,
    pub(crate) created_at: String,
}

#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
pub(crate) struct AssetFieldSource {
    pub(crate) field_path: String,
    pub(crate) source_text: String,
    pub(crate) confidence: f64,
}

/// 快照创建原因。
#[derive(Debug, Clone, Default, serde::Deserialize, serde::Serialize)]
pub(crate) enum SnapshotReason {
    #[serde(rename = "seed")]
    Seed,
    #[serde(rename = "manual")]
    Manual,
    #[serde(rename = "import")]
    Import,
    #[serde(rename = "edit")]
    #[default]
    Edit,
    #[serde(rename = "rollback")]
    Rollback,
}

/// 设备资产快照元数据条目。
#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
pub(crate) struct DeviceSnapshotMeta {
    /// 对应的版本号（与 `.v{version}.device.yaml` 文件对应）。
    pub(crate) version: i64,
    /// 快照显示标签。
    pub(crate) label: String,
    /// 快照描述。
    #[serde(default)]
    pub(crate) description: String,
    /// 创建原因。
    #[serde(default)]
    pub(crate) reason: SnapshotReason,
    /// 创建时间（RFC3339）。
    pub(crate) created_at: String,
}

/// 资产文件名安全化：只保留 ASCII 字母数字、`_`、`-`、`.`。
pub(crate) fn sanitize_asset_file_stem(raw: &str) -> String {
    let stem = raw
        .trim()
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || matches!(ch, '_' | '-' | '.') {
                ch
            } else {
                '_'
            }
        })
        .collect::<String>();
    let stem = stem
        .trim_matches(|ch| matches!(ch, '_' | '-' | '.'))
        .to_owned();
    if stem.is_empty() {
        "asset".to_owned()
    } else {
        stem
    }
}

/// 读取文件修改时间（RFC3339）。
pub(crate) async fn file_modified_at(path: &Path) -> Result<String, String> {
    let modified = fs::metadata(path)
        .await
        .map_err(|error| format!("读取 DSL 资产文件元数据失败 `{}`: {error}", path.display()))?
        .modified()
        .unwrap_or_else(|_| SystemTime::now());
    let dt: DateTime<Utc> = modified.into();
    Ok(dt.to_rfc3339())
}
