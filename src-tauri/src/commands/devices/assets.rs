//! 设备资产 CRUD 命令。

use nazh_dsl_core::parse_device_yaml;
use tauri::AppHandle;

use crate::asset_files::{
    DeviceSnapshotMeta, SnapshotReason, append_device_snapshot, device_asset_latest_path,
    file_modified_at, list_device_asset_version_files, list_device_asset_yaml_files,
    next_device_asset_version, write_device_asset_yaml,
};

use super::types::{DeviceAssetDetail, DeviceAssetSummary, DeviceConnectionRef};

/// 列出所有设备资产摘要。
#[tauri::command]
pub(crate) async fn list_device_assets(
    app: AppHandle,
    workspace_path: Option<String>,
) -> Result<Vec<DeviceAssetSummary>, String> {
    let files = list_device_asset_yaml_files(&app, workspace_path.as_deref()).await?;
    let mut summaries = Vec::with_capacity(files.len());
    for file in files {
        let detail = load_device_detail_from_path(&app, workspace_path.as_deref(), &file).await?;
        let connection = detail
            .spec_json
            .get("connection")
            .and_then(|value| value.as_object())
            .map(|map| DeviceConnectionRef {
                connection_type: map
                    .get("type")
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or("")
                    .to_owned(),
                id: map
                    .get("id")
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or("")
                    .to_owned(),
                unit: map
                    .get("unit")
                    .and_then(serde_json::Value::as_u64)
                    .and_then(|v| u8::try_from(v).ok()),
            });
        summaries.push(DeviceAssetSummary {
            id: detail.id,
            name: detail.name,
            device_type: detail.device_type,
            version: detail.version,
            updated_at: detail.updated_at,
            connection,
        });
    }
    summaries.sort_by(|left, right| left.id.cmp(&right.id));
    Ok(summaries)
}

/// 加载指定设备资产详情。
#[tauri::command]
pub(crate) async fn load_device_asset(
    app: AppHandle,
    id: String,
    workspace_path: Option<String>,
) -> Result<Option<DeviceAssetDetail>, String> {
    let path = device_asset_latest_path(&app, workspace_path.as_deref(), &id)?;
    if !path.exists() {
        return Ok(None);
    }
    load_device_detail_from_path(&app, workspace_path.as_deref(), &path)
        .await
        .map(Some)
}

/// 保存（或更新）设备资产。
///
/// 接收 YAML 格式的设备规格，解析校验后存储为 JSON。
/// 可选 `snapshot_label` / `snapshot_reason` 用于记录快照元数据。
#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub(crate) async fn save_device_asset(
    app: AppHandle,
    id: String,
    name: String,
    device_type: String,
    spec_yaml: String,
    workspace_path: Option<String>,
    snapshot_label: Option<String>,
    snapshot_reason: Option<String>,
) -> Result<(), String> {
    // 解析 YAML 校验合法性
    let spec = parse_device_yaml(&spec_yaml).map_err(|e| format!("设备 DSL 解析失败: {e}"))?;
    if spec.id != id {
        return Err(format!(
            "设备资产 ID `{id}` 与 Device DSL id `{}` 不一致",
            spec.id
        ));
    }
    if spec.device_type != device_type {
        return Err(format!(
            "设备类型 `{device_type}` 与 Device DSL type `{}` 不一致",
            spec.device_type
        ));
    }
    let _ = name;
    let version = next_device_asset_version(&app, workspace_path.as_deref(), &id).await?;
    write_device_asset_yaml(
        &app,
        workspace_path.as_deref(),
        &id,
        version,
        spec_yaml.trim(),
    )
    .await?;

    // 记录快照元数据
    let reason_str = snapshot_reason.as_deref().unwrap_or("edit");
    let reason = match reason_str {
        "seed" => SnapshotReason::Seed,
        "manual" => SnapshotReason::Manual,
        "import" => SnapshotReason::Import,
        "rollback" => SnapshotReason::Rollback,
        _ => SnapshotReason::Edit,
    };
    let label = snapshot_label.unwrap_or_else(|| match reason {
        SnapshotReason::Seed => "初始快照".to_owned(),
        SnapshotReason::Import => "导入快照".to_owned(),
        SnapshotReason::Rollback => "回滚前保护".to_owned(),
        SnapshotReason::Edit => format!("编辑快照 v{version}"),
        SnapshotReason::Manual => format!("手动快照 v{version}"),
    });
    let meta = DeviceSnapshotMeta {
        version,
        label,
        description: String::new(),
        reason,
        created_at: chrono::Utc::now().to_rfc3339(),
    };
    append_device_snapshot(&app, workspace_path.as_deref(), &id, meta).await?;

    Ok(())
}

/// 删除设备资产及其版本历史。
#[tauri::command]
pub(crate) async fn delete_device_asset(
    app: AppHandle,
    id: String,
    workspace_path: Option<String>,
) -> Result<(), String> {
    crate::asset_files::delete_device_asset_yaml(&app, workspace_path.as_deref(), &id).await
}

// ---- 辅助函数 ----

/// 从文件路径加载设备资产完整详情。
async fn load_device_detail_from_path(
    app: &AppHandle,
    workspace_path: Option<&str>,
    path: &std::path::Path,
) -> Result<DeviceAssetDetail, String> {
    let spec_yaml = tokio::fs::read_to_string(path)
        .await
        .map_err(|error| format!("读取设备 DSL 文件失败 `{}`: {error}", path.display()))?;
    let spec = parse_device_yaml(&spec_yaml).map_err(|e| format!("设备 DSL 解析失败: {e}"))?;
    let spec_json = serde_json::to_value(&spec).map_err(|e| format!("设备规格序列化失败: {e}"))?;
    let versions = list_device_asset_version_files(app, workspace_path, &spec.id).await?;
    let version = versions.first().map_or(1, |item| item.version);
    let updated_at = file_modified_at(path).await?;
    let created_at = versions
        .last()
        .map_or_else(|| updated_at.clone(), |item| item.created_at.clone());

    Ok(DeviceAssetDetail {
        id: spec.id.clone(),
        name: spec.model.clone().unwrap_or_else(|| spec.id.clone()),
        device_type: spec.device_type.clone(),
        version,
        spec_json,
        spec_yaml,
        yaml_file_path: Some(path.to_string_lossy().to_string()),
        created_at,
        updated_at,
    })
}
