//! 设备资产版本管理命令。

use nazh_dsl_core::parse_device_yaml;
use tauri::AppHandle;

use crate::asset_files::{list_device_asset_version_files, load_device_asset_version_yaml};

use super::types::{AssetVersionSummary, StoredAssetVersion};

/// 列出设备资产的所有版本摘要。
#[tauri::command]
pub(crate) async fn list_asset_versions(
    app: AppHandle,
    asset_id: String,
    workspace_path: Option<String>,
) -> Result<Vec<AssetVersionSummary>, String> {
    let versions =
        list_device_asset_version_files(&app, workspace_path.as_deref(), &asset_id).await?;
    Ok(versions
        .into_iter()
        .map(|version| AssetVersionSummary {
            version: version.version,
            created_at: version.created_at,
            source_summary: None,
        })
        .collect())
}

/// 加载特定版本的设备资产。
#[tauri::command]
pub(crate) async fn load_asset_version(
    app: AppHandle,
    asset_id: String,
    version: i64,
    workspace_path: Option<String>,
) -> Result<Option<StoredAssetVersion>, String> {
    let Some((yaml, created_at)) =
        load_device_asset_version_yaml(&app, workspace_path.as_deref(), &asset_id, version).await?
    else {
        return Ok(None);
    };
    let spec = parse_device_yaml(&yaml).map_err(|e| format!("设备 DSL 解析失败: {e}"))?;
    let spec_json = serde_json::to_value(&spec).map_err(|e| format!("设备规格序列化失败: {e}"))?;
    Ok(Some(StoredAssetVersion {
        asset_id,
        version,
        spec_json,
        source_summary: None,
        created_at,
    }))
}
