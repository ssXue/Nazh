use nazh_dsl_core::{ConnectionProtocol, parse_connection_yaml_validated};
use nazh_engine::ConnectionRecord;
use tauri::{AppHandle, State};
use tokio::fs;

use crate::asset_files::{
    connection_asset_latest_path, delete_connection_asset_yaml, file_modified_at,
    list_connection_asset_yaml_files, next_connection_asset_version, write_connection_asset_yaml,
};
use crate::state::DesktopState;

/// 连接资产摘要（IPC 响应）。
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ConnectionAssetSummary {
    pub(crate) id: String,
    pub(crate) protocol_type: String,
    pub(crate) description: Option<String>,
    pub(crate) version: i64,
    pub(crate) updated_at: String,
}

/// 连接资产完整详情（IPC 响应）。
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ConnectionAssetDetail {
    pub(crate) id: String,
    pub(crate) protocol_type: String,
    pub(crate) description: Option<String>,
    pub(crate) version: i64,
    pub(crate) spec_json: serde_json::Value,
    pub(crate) spec_yaml: String,
    pub(crate) yaml_file_path: Option<String>,
    pub(crate) created_at: String,
    pub(crate) updated_at: String,
}

#[tauri::command]
pub(crate) async fn list_connections(
    state: State<'_, DesktopState>,
) -> Result<Vec<ConnectionRecord>, String> {
    let connections = state.connection_manager.list().await;
    Ok(connections)
}

#[tauri::command]
pub(crate) async fn list_connection_assets(
    app: AppHandle,
    workspace_path: Option<String>,
) -> Result<Vec<ConnectionAssetSummary>, String> {
    let files = list_connection_asset_yaml_files(&app, workspace_path.as_deref()).await?;
    let mut summaries = Vec::with_capacity(files.len());
    for file in files {
        let detail =
            load_connection_asset_detail_from_path(&app, workspace_path.as_deref(), &file).await?;
        summaries.push(ConnectionAssetSummary {
            id: detail.id,
            protocol_type: detail.protocol_type,
            description: detail.description,
            version: detail.version,
            updated_at: detail.updated_at,
        });
    }
    summaries.sort_by(|left, right| left.id.cmp(&right.id));
    Ok(summaries)
}

#[tauri::command]
pub(crate) async fn load_connection_asset(
    app: AppHandle,
    id: String,
    workspace_path: Option<String>,
) -> Result<Option<ConnectionAssetDetail>, String> {
    let path = connection_asset_latest_path(&app, workspace_path.as_deref(), &id)?;
    if !path.exists() {
        return Ok(None);
    }
    load_connection_asset_detail_from_path(&app, workspace_path.as_deref(), &path)
        .await
        .map(Some)
}

#[tauri::command]
pub(crate) async fn save_connection_asset(
    app: AppHandle,
    id: String,
    spec_yaml: String,
    workspace_path: Option<String>,
) -> Result<(), String> {
    let spec = parse_connection_yaml_validated(&spec_yaml)
        .map_err(|e| format!("连接 DSL 解析失败: {e}"))?;
    if spec.id != id {
        return Err(format!(
            "连接资产 ID `{id}` 与 Connection DSL id `{}` 不一致",
            spec.id
        ));
    }
    let version = next_connection_asset_version(&app, workspace_path.as_deref(), &id).await?;
    write_connection_asset_yaml(
        &app,
        workspace_path.as_deref(),
        &id,
        version,
        spec_yaml.trim(),
    )
    .await
    .map(|_| ())
}

#[tauri::command]
pub(crate) async fn delete_connection_asset(
    app: AppHandle,
    id: String,
    workspace_path: Option<String>,
) -> Result<(), String> {
    delete_connection_asset_yaml(&app, workspace_path.as_deref(), &id).await
}

#[tauri::command]
pub(crate) async fn save_connection_secret(
    state: State<'_, DesktopState>,
    connection_id: String,
    secret_key: String,
    value: String,
) -> Result<(), String> {
    let connection_id = connection_id.trim();
    let secret_key = secret_key.trim();
    if connection_id.is_empty() {
        return Err("连接 ID 不能为空".to_owned());
    }
    if secret_key.is_empty() {
        return Err("连接密钥名不能为空".to_owned());
    }
    let updated_at = chrono::Utc::now().to_rfc3339();
    state
        .store_handle()?
        .upsert_connection_secret(connection_id, secret_key, value.as_str(), &updated_at, None)
        .await
        .map_err(|error| format!("保存连接密钥失败: {error}"))
}

#[tauri::command]
pub(crate) async fn delete_connection_secret(
    state: State<'_, DesktopState>,
    connection_id: String,
    secret_key: String,
) -> Result<(), String> {
    state
        .store_handle()?
        .delete_connection_secret(connection_id.trim(), secret_key.trim())
        .await
        .map_err(|error| format!("删除连接密钥失败: {error}"))
}

#[tauri::command]
pub(crate) async fn reset_connection_circuit_breaker(
    state: State<'_, DesktopState>,
    connection_id: String,
) -> Result<(), String> {
    state
        .connection_manager
        .reset_circuit_breaker(&connection_id)
        .await
        .map_err(|e| e.to_string())
}

async fn load_connection_asset_detail_from_path(
    app: &AppHandle,
    workspace_path: Option<&str>,
    path: &std::path::Path,
) -> Result<ConnectionAssetDetail, String> {
    let spec_yaml = fs::read_to_string(path)
        .await
        .map_err(|error| format!("读取连接 DSL 文件失败 `{}`: {error}", path.display()))?;
    let spec = parse_connection_yaml_validated(&spec_yaml)
        .map_err(|e| format!("连接 DSL 解析失败: {e}"))?;
    let spec_json = serde_json::to_value(&spec).map_err(|e| format!("连接规格序列化失败: {e}"))?;
    let version = next_connection_asset_version(app, workspace_path, &spec.id)
        .await?
        .saturating_sub(1)
        .max(1);
    let updated_at = file_modified_at(path).await?;
    let created_at = updated_at.clone();

    Ok(ConnectionAssetDetail {
        id: spec.id.clone(),
        protocol_type: protocol_type(&spec.protocol).to_owned(),
        description: spec.description.clone(),
        version,
        spec_json,
        spec_yaml,
        yaml_file_path: Some(path.to_string_lossy().to_string()),
        created_at,
        updated_at,
    })
}

fn protocol_type(protocol: &ConnectionProtocol) -> &'static str {
    match protocol {
        ConnectionProtocol::ModbusTcp { .. } => "modbus-tcp",
        ConnectionProtocol::Serial { .. } => "serial",
        ConnectionProtocol::Mqtt { .. } => "mqtt",
        ConnectionProtocol::Http { .. } => "http",
        ConnectionProtocol::Bark { .. } => "bark",
        ConnectionProtocol::CanSlcan { .. } => "can-slcan",
        ConnectionProtocol::Ethercat { .. } => "ethercat",
    }
}
