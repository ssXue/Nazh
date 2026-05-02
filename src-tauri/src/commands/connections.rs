use nazh_engine::{ConnectionDefinition, ConnectionRecord};
use tauri::{AppHandle, State};
use tauri_bindings::ConnectionDefinitionsLoadResult;
use tokio::fs;

use crate::state::DesktopState;

#[tauri::command]
pub(crate) async fn list_connections(
    state: State<'_, DesktopState>,
) -> Result<Vec<ConnectionRecord>, String> {
    let connections = state.connection_manager.list().await;
    Ok(connections)
}

#[tauri::command]
pub(crate) async fn load_connection_definitions(
    app: AppHandle,
    state: State<'_, DesktopState>,
    workspace_path: Option<String>,
) -> Result<ConnectionDefinitionsLoadResult, String> {
    let path = DesktopState::connections_file_path(&app, workspace_path.as_deref())?;
    let file_exists = path.exists();
    if !path.exists() {
        state
            .connection_manager
            .replace_connections(Vec::<ConnectionDefinition>::new())
            .await;
        return Ok(ConnectionDefinitionsLoadResult {
            definitions: Vec::new(),
            file_exists,
        });
    }
    let text = fs::read_to_string(&path)
        .await
        .map_err(|e| format!("读取 connections.json 失败: {e}"))?;
    let defs = serde_json::from_str::<Vec<ConnectionDefinition>>(&text)
        .map_err(|e| format!("解析 connections.json 失败: {e}"))?;
    state
        .connection_manager
        .replace_connections(defs.clone())
        .await;
    Ok(ConnectionDefinitionsLoadResult {
        definitions: defs,
        file_exists,
    })
}

#[tauri::command]
pub(crate) async fn save_connection_definitions(
    app: AppHandle,
    state: State<'_, DesktopState>,
    workspace_path: Option<String>,
    definitions: Vec<ConnectionDefinition>,
) -> Result<(), String> {
    let path = DesktopState::connections_file_path(&app, workspace_path.as_deref())?;
    let dir = path.parent().ok_or("无法确定连接文件目录")?;
    fs::create_dir_all(dir)
        .await
        .map_err(|e| format!("创建连接文件目录失败: {e}"))?;
    let text = serde_json::to_string_pretty(&definitions)
        .map_err(|e| format!("序列化连接定义失败: {e}"))?;
    fs::write(&path, text)
        .await
        .map_err(|e| format!("写入 connections.json 失败: {e}"))?;
    state
        .connection_manager
        .replace_connections(definitions)
        .await;
    Ok(())
}
