use tauri::{AppHandle, State};
use tauri_bindings::ObservabilityQueryResult;

use crate::{
    observability::{
        clear_observability as clear_workspace_observability, clear_observability_store,
        query_observability as query_workspace_observability,
    },
    state::DesktopState,
    workspace::resolve_project_workspace_dir,
};

#[tauri::command]
pub(crate) async fn query_observability(
    app: AppHandle,
    state: State<'_, DesktopState>,
    workspace_path: Option<String>,
    trace_id: Option<String>,
    search: Option<String>,
    limit: Option<usize>,
) -> Result<ObservabilityQueryResult, String> {
    let (workspace_dir, _) = resolve_project_workspace_dir(&app, workspace_path.as_deref())?;
    let store = state.store_handle().ok();
    query_workspace_observability(workspace_dir, store, trace_id, search, limit.unwrap_or(240))
        .await
}

#[tauri::command]
pub(crate) async fn clear_observability(
    app: AppHandle,
    state: State<'_, DesktopState>,
    workspace_path: Option<String>,
) -> Result<(), String> {
    let (workspace_dir, _) = resolve_project_workspace_dir(&app, workspace_path.as_deref())?;
    clear_workspace_observability(workspace_dir).await?;
    clear_observability_store(state.store_handle().ok()).await;
    Ok(())
}
