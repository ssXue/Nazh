//! 设备资产快照管理命令。

use tauri::AppHandle;

use crate::asset_files::{
    DeviceSnapshotMeta, SnapshotReason, append_device_snapshot, delete_device_snapshot_meta,
    device_asset_latest_path, next_device_asset_version, read_device_snapshots,
    write_device_asset_yaml,
};

use super::types::DeviceSnapshotSummary;

/// 列出设备资产的所有快照。
#[tauri::command]
pub(crate) async fn list_device_snapshots(
    app: AppHandle,
    asset_id: String,
    workspace_path: Option<String>,
) -> Result<Vec<DeviceSnapshotSummary>, String> {
    let snapshots = read_device_snapshots(&app, workspace_path.as_deref(), &asset_id).await?;
    Ok(snapshots
        .into_iter()
        .map(|s| DeviceSnapshotSummary {
            version: s.version,
            label: s.label,
            description: s.description,
            reason: match s.reason {
                SnapshotReason::Seed => "seed",
                SnapshotReason::Manual => "manual",
                SnapshotReason::Import => "import",
                SnapshotReason::Edit => "edit",
                SnapshotReason::Rollback => "rollback",
            }
            .to_owned(),
            created_at: s.created_at,
        })
        .collect())
}

/// 创建手动快照（保存当前最新内容为新版本，附带标签）。
#[tauri::command]
pub(crate) async fn create_device_snapshot(
    app: AppHandle,
    asset_id: String,
    label: Option<String>,
    description: Option<String>,
    workspace_path: Option<String>,
) -> Result<(), String> {
    // 读取当前最新内容
    let path = device_asset_latest_path(&app, workspace_path.as_deref(), &asset_id)?;
    if !path.exists() {
        return Err(format!("设备资产 `{asset_id}` 不存在"));
    }
    let yaml = tokio::fs::read_to_string(&path)
        .await
        .map_err(|e| format!("读取设备 DSL 失败: {e}"))?;

    // 保存为新版本
    let version = next_device_asset_version(&app, workspace_path.as_deref(), &asset_id).await?;
    write_device_asset_yaml(
        &app,
        workspace_path.as_deref(),
        &asset_id,
        version,
        yaml.trim(),
    )
    .await?;

    let meta = DeviceSnapshotMeta {
        version,
        label: label.unwrap_or_else(|| format!("手动快照 v{version}")),
        description: description.unwrap_or_default(),
        reason: SnapshotReason::Manual,
        created_at: chrono::Utc::now().to_rfc3339(),
    };
    append_device_snapshot(&app, workspace_path.as_deref(), &asset_id, meta).await
}

/// 回滚到指定快照版本。先保存当前状态为保护快照，再恢复目标版本内容。
#[tauri::command]
pub(crate) async fn rollback_device_snapshot(
    app: AppHandle,
    asset_id: String,
    target_version: i64,
    workspace_path: Option<String>,
) -> Result<(), String> {
    // 读取当前最新
    let latest_path = device_asset_latest_path(&app, workspace_path.as_deref(), &asset_id)?;
    let current_yaml = tokio::fs::read_to_string(&latest_path)
        .await
        .map_err(|e| format!("读取当前设备 DSL 失败: {e}"))?;

    // 保护快照
    let protection_version =
        next_device_asset_version(&app, workspace_path.as_deref(), &asset_id).await?;
    write_device_asset_yaml(
        &app,
        workspace_path.as_deref(),
        &asset_id,
        protection_version,
        current_yaml.trim(),
    )
    .await?;
    let protection_meta = DeviceSnapshotMeta {
        version: protection_version,
        label: format!("回滚前保护（→ v{target_version}）"),
        description: format!("回滚到 v{target_version} 前自动保留的快照。"),
        reason: SnapshotReason::Rollback,
        created_at: chrono::Utc::now().to_rfc3339(),
    };
    append_device_snapshot(&app, workspace_path.as_deref(), &asset_id, protection_meta).await?;

    // 恢复目标版本
    let Some((target_yaml, _)) = crate::asset_files::load_device_asset_version_yaml(
        &app,
        workspace_path.as_deref(),
        &asset_id,
        target_version,
    )
    .await?
    else {
        return Err(format!("快照 v{target_version} 不存在"));
    };
    let restore_version =
        next_device_asset_version(&app, workspace_path.as_deref(), &asset_id).await?;
    write_device_asset_yaml(
        &app,
        workspace_path.as_deref(),
        &asset_id,
        restore_version,
        target_yaml.trim(),
    )
    .await?;

    let restore_meta = DeviceSnapshotMeta {
        version: restore_version,
        label: format!("回滚到 v{target_version}"),
        description: String::new(),
        reason: SnapshotReason::Rollback,
        created_at: chrono::Utc::now().to_rfc3339(),
    };
    append_device_snapshot(&app, workspace_path.as_deref(), &asset_id, restore_meta).await
}

/// 删除指定快照的元数据（不删除版本文件本身）。
#[tauri::command]
pub(crate) async fn delete_device_snapshot(
    app: AppHandle,
    asset_id: String,
    version: i64,
    workspace_path: Option<String>,
) -> Result<(), String> {
    delete_device_snapshot_meta(&app, workspace_path.as_deref(), &asset_id, version).await
}
