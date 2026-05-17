//! 设备资产快照元数据管理。

use tauri::AppHandle;
use tokio::fs;

use super::types::{
    DEVICES_DIR, DSL_ASSETS_DIR, DeviceSnapshotMeta, MAX_SNAPSHOTS, SNAPSHOTS_SUFFIX,
    sanitize_asset_file_stem,
};
use crate::workspace::resolve_project_workspace_dir;

/// 读取设备资产的快照元数据。文件不存在时返回空列表。
pub(crate) async fn read_device_snapshots(
    app: &AppHandle,
    workspace_path: Option<&str>,
    asset_id: &str,
) -> Result<Vec<DeviceSnapshotMeta>, String> {
    let (workspace_dir, _) = resolve_project_workspace_dir(app, workspace_path)?;
    let path = workspace_dir
        .join(DSL_ASSETS_DIR)
        .join(DEVICES_DIR)
        .join(format!(
            "{}{SNAPSHOTS_SUFFIX}",
            sanitize_asset_file_stem(asset_id)
        ));
    match fs::read_to_string(&path).await {
        Ok(text) => serde_yaml::from_str::<Vec<DeviceSnapshotMeta>>(&text)
            .map_err(|e| format!("解析快照元数据失败 `{}`: {e}", path.display())),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Vec::new()),
        Err(e) => Err(format!("读取快照元数据失败 `{}`: {e}", path.display())),
    }
}

/// 写入设备资产的快照元数据（追加一条并保留上限）。
pub(crate) async fn append_device_snapshot(
    app: &AppHandle,
    workspace_path: Option<&str>,
    asset_id: &str,
    meta: DeviceSnapshotMeta,
) -> Result<(), String> {
    let (workspace_dir, _) = resolve_project_workspace_dir(app, workspace_path)?;
    let dir = workspace_dir.join(DSL_ASSETS_DIR).join(DEVICES_DIR);
    let path = dir.join(format!(
        "{}{SNAPSHOTS_SUFFIX}",
        sanitize_asset_file_stem(asset_id)
    ));

    let mut snapshots = match fs::read_to_string(&path).await {
        Ok(text) => serde_yaml::from_str::<Vec<DeviceSnapshotMeta>>(&text).unwrap_or_default(),
        Err(_) => Vec::new(),
    };

    // 去重：同一版本号只保留最新元数据
    snapshots.retain(|s| s.version != meta.version);
    snapshots.insert(0, meta);

    // 保留上限
    snapshots.truncate(MAX_SNAPSHOTS);

    let text =
        serde_yaml::to_string(&snapshots).map_err(|e| format!("序列化快照元数据失败: {e}"))?;
    fs::write(&path, text)
        .await
        .map_err(|e| format!("写入快照元数据失败 `{}`: {e}", path.display()))?;

    Ok(())
}

/// 删除指定版本的快照元数据。
pub(crate) async fn delete_device_snapshot_meta(
    app: &AppHandle,
    workspace_path: Option<&str>,
    asset_id: &str,
    version: i64,
) -> Result<(), String> {
    let (workspace_dir, _) = resolve_project_workspace_dir(app, workspace_path)?;
    let path = workspace_dir
        .join(DSL_ASSETS_DIR)
        .join(DEVICES_DIR)
        .join(format!(
            "{}{SNAPSHOTS_SUFFIX}",
            sanitize_asset_file_stem(asset_id)
        ));

    let mut snapshots = match fs::read_to_string(&path).await {
        Ok(text) => serde_yaml::from_str::<Vec<DeviceSnapshotMeta>>(&text).unwrap_or_default(),
        Err(_) => return Ok(()),
    };

    snapshots.retain(|s| s.version != version);

    let text =
        serde_yaml::to_string(&snapshots).map_err(|e| format!("序列化快照元数据失败: {e}"))?;
    fs::write(&path, text)
        .await
        .map_err(|e| format!("写入快照元数据失败 `{}`: {e}", path.display()))?;

    Ok(())
}
