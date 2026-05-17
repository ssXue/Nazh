use tauri::{AppHandle, State};
use tauri_bindings::{DeploymentAuditEntry, DeploymentAuditQueryResult, ObservabilityQueryResult};

use crate::{
    observability::{
        clear_observability_store, query_observability as query_workspace_observability,
    },
    state::DesktopState,
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
    let _ = (&app, workspace_path);
    let store = state.store_handle().ok();
    query_workspace_observability(store, trace_id, search, limit.unwrap_or(240)).await
}

#[tauri::command]
pub(crate) async fn clear_observability(
    app: AppHandle,
    state: State<'_, DesktopState>,
    workspace_path: Option<String>,
) -> Result<(), String> {
    let _ = (&app, workspace_path);
    clear_observability_store(state.store_handle().ok()).await;
    Ok(())
}

/// 查询指定工作流的部署审计记录（RFC-0003 Phase 3）。
#[tauri::command]
pub(crate) async fn query_deployment_audit(
    state: State<'_, DesktopState>,
    workflow_id: String,
    limit: Option<usize>,
) -> Result<DeploymentAuditQueryResult, String> {
    let store = state.store_handle()?;
    let records = store
        .list_deployment_audit(&workflow_id, limit.unwrap_or(50))
        .await
        .map_err(|e| format!("查询部署审计失败: {e}"))?;
    Ok(DeploymentAuditQueryResult {
        records: records
            .into_iter()
            .map(|r| DeploymentAuditEntry {
                id: r.id,
                workflow_id: r.workflow_id,
                action: r.action,
                level: r.level,
                timestamp: r.timestamp,
                project_id: r.project_id,
                project_name: r.project_name,
                environment_id: r.environment_id,
                environment_name: r.environment_name,
                message: r.message,
                detail: r.detail,
                data: r.data,
            })
            .collect(),
    })
}
