use std::path::Path;

use serde_json::Value;
use tauri::AppHandle;
use tauri_bindings::{
    PersistedDeploymentSession, PersistedDeploymentSessionCollection,
    PersistedDeploymentSessionState,
};
use tokio::fs;

use crate::state::DesktopState;

fn sort_deployment_sessions_by_freshness(sessions: &mut [PersistedDeploymentSession]) {
    sessions.sort_by(|left, right| right.deployed_at.cmp(&left.deployed_at));
}

fn normalize_deployment_sessions(
    sessions: Vec<PersistedDeploymentSession>,
) -> Vec<PersistedDeploymentSession> {
    let mut sessions = sessions;
    sort_deployment_sessions_by_freshness(&mut sessions);

    let mut seen = std::collections::HashSet::new();
    let mut normalized = Vec::new();
    for session in sessions {
        if seen.insert(session.project_id.clone()) {
            normalized.push(session);
        }
    }
    normalized
}

fn normalize_deployment_session_state(
    state: PersistedDeploymentSessionState,
) -> PersistedDeploymentSessionState {
    let sessions = normalize_deployment_sessions(state.sessions);
    let active_project_id = state
        .active_project_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .filter(|value| sessions.iter().any(|session| session.project_id == *value))
        .map(str::to_owned);

    PersistedDeploymentSessionState {
        version: 3,
        active_project_id,
        sessions,
    }
}

async fn read_deployment_sessions_from_path(
    path: &Path,
) -> Result<Vec<PersistedDeploymentSession>, String> {
    Ok(read_deployment_session_state_from_path(path)
        .await?
        .sessions)
}

async fn read_deployment_session_state_from_path(
    path: &Path,
) -> Result<PersistedDeploymentSessionState, String> {
    if !path.exists() {
        return Ok(PersistedDeploymentSessionState {
            version: 3,
            active_project_id: None,
            sessions: Vec::new(),
        });
    }

    let text = fs::read_to_string(path)
        .await
        .map_err(|error| format!("读取 deployment-session.json 失败: {error}"))?;
    let value = serde_json::from_str::<Value>(&text)
        .map_err(|error| format!("解析 deployment-session.json 失败: {error}"))?;

    if value
        .get("sessions")
        .is_some_and(serde_json::Value::is_array)
    {
        let collection = serde_json::from_value::<PersistedDeploymentSessionCollection>(value)
            .map_err(|error| format!("解析 deployment-session.json 失败: {error}"))?;
        return Ok(normalize_deployment_session_state(
            PersistedDeploymentSessionState {
                version: collection.version,
                active_project_id: collection.active_project_id,
                sessions: collection.sessions,
            },
        ));
    }

    let session = serde_json::from_value::<PersistedDeploymentSession>(value)
        .map_err(|error| format!("解析 deployment-session.json 失败: {error}"))?;
    Ok(normalize_deployment_session_state(
        PersistedDeploymentSessionState {
            version: 1,
            active_project_id: None,
            sessions: vec![session],
        },
    ))
}

async fn write_deployment_session_state_to_path(
    path: &Path,
    state: PersistedDeploymentSessionState,
) -> Result<(), String> {
    let normalized = normalize_deployment_session_state(state);
    let sessions = normalized.sessions.clone();

    if sessions.is_empty() {
        if path.exists() {
            fs::remove_file(path)
                .await
                .map_err(|error| format!("删除 deployment-session.json 失败: {error}"))?;
        }
        return Ok(());
    }

    let dir = path.parent().ok_or("无法确定部署会话文件目录")?;
    fs::create_dir_all(dir)
        .await
        .map_err(|error| format!("创建部署会话目录失败: {error}"))?;

    let payload = PersistedDeploymentSessionCollection {
        version: 3,
        active_project_id: normalized.active_project_id,
        sessions,
    };
    let text = serde_json::to_string_pretty(&payload)
        .map_err(|error| format!("序列化部署会话失败: {error}"))?;
    fs::write(path, text)
        .await
        .map_err(|error| format!("写入 deployment-session.json 失败: {error}"))?;
    Ok(())
}

#[tauri::command]
pub(crate) async fn load_deployment_session_file(
    app: AppHandle,
    workspace_path: Option<String>,
) -> Result<Option<PersistedDeploymentSession>, String> {
    let path = DesktopState::deployment_session_file_path(&app, workspace_path.as_deref())?;
    Ok(read_deployment_sessions_from_path(&path)
        .await?
        .into_iter()
        .next())
}

#[tauri::command]
pub(crate) async fn load_deployment_session_state_file(
    app: AppHandle,
    workspace_path: Option<String>,
) -> Result<PersistedDeploymentSessionState, String> {
    let path = DesktopState::deployment_session_file_path(&app, workspace_path.as_deref())?;
    read_deployment_session_state_from_path(&path).await
}

#[tauri::command]
pub(crate) async fn list_deployment_sessions_file(
    app: AppHandle,
    workspace_path: Option<String>,
) -> Result<Vec<PersistedDeploymentSession>, String> {
    let path = DesktopState::deployment_session_file_path(&app, workspace_path.as_deref())?;
    read_deployment_sessions_from_path(&path).await
}

#[tauri::command]
pub(crate) async fn save_deployment_session_file(
    app: AppHandle,
    workspace_path: Option<String>,
    session: PersistedDeploymentSession,
    active_project_id: Option<String>,
) -> Result<(), String> {
    let path = DesktopState::deployment_session_file_path(&app, workspace_path.as_deref())?;
    let mut state = read_deployment_session_state_from_path(&path).await?;
    state
        .sessions
        .retain(|current| current.project_id != session.project_id);
    state.sessions.push(session);
    if let Some(active_project_id) = active_project_id {
        let trimmed = active_project_id.trim();
        state.active_project_id = if trimmed.is_empty() {
            None
        } else {
            Some(trimmed.to_owned())
        };
    }
    write_deployment_session_state_to_path(&path, state).await
}

#[tauri::command]
pub(crate) async fn set_deployment_session_active_project_file(
    app: AppHandle,
    workspace_path: Option<String>,
    project_id: Option<String>,
) -> Result<(), String> {
    let path = DesktopState::deployment_session_file_path(&app, workspace_path.as_deref())?;
    let mut state = read_deployment_session_state_from_path(&path).await?;
    state.active_project_id = project_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_owned);
    write_deployment_session_state_to_path(&path, state).await
}

#[tauri::command]
pub(crate) async fn remove_deployment_session_file(
    app: AppHandle,
    workspace_path: Option<String>,
    project_id: String,
) -> Result<(), String> {
    let path = DesktopState::deployment_session_file_path(&app, workspace_path.as_deref())?;
    let target_project_id = project_id.trim();
    if target_project_id.is_empty() {
        return Ok(());
    }

    let mut state = read_deployment_session_state_from_path(&path).await?;
    state
        .sessions
        .retain(|session| session.project_id != target_project_id);
    if state.active_project_id.as_deref() == Some(target_project_id) {
        state.active_project_id = None;
    }
    write_deployment_session_state_to_path(&path, state).await
}

#[tauri::command]
pub(crate) async fn clear_deployment_session_file(
    app: AppHandle,
    workspace_path: Option<String>,
) -> Result<(), String> {
    let path = DesktopState::deployment_session_file_path(&app, workspace_path.as_deref())?;

    if !path.exists() {
        return Ok(());
    }

    fs::remove_file(&path)
        .await
        .map_err(|error| format!("删除 deployment-session.json 失败: {error}"))?;
    Ok(())
}
