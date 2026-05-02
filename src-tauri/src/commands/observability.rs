use tauri::AppHandle;
use tauri_bindings::ObservabilityQueryResult;

use crate::{
    observability::query_observability as query_workspace_observability,
    workspace::resolve_project_workspace_dir,
};

#[tauri::command]
pub(crate) async fn query_observability(
    app: AppHandle,
    workspace_path: Option<String>,
    trace_id: Option<String>,
    search: Option<String>,
    limit: Option<usize>,
) -> Result<ObservabilityQueryResult, String> {
    let (workspace_dir, _) = resolve_project_workspace_dir(&app, workspace_path.as_deref())?;
    query_workspace_observability(workspace_dir, trace_id, search, limit.unwrap_or(240)).await
}
