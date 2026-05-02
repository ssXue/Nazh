use nazh_engine::{EngineError, WorkflowContext};
use serde_json::json;
use tauri::State;
use tauri_bindings::DispatchResponse;

use crate::{
    state::DesktopState,
    util::stringify_error,
};

#[tauri::command]
pub(crate) async fn dispatch_payload(
    state: State<'_, DesktopState>,
    payload: serde_json::Value,
    workflow_id: Option<String>,
) -> Result<DispatchResponse, String> {
    let target_workflow_id = state
        .resolve_workflow_id(workflow_id.as_deref())
        .await?
        .ok_or_else(|| stringify_error(&EngineError::WorkflowUnavailable))?;
    let (dispatch_router, observability_store) = {
        let workflows = state.workflows.lock().await;
        let workflow = workflows
            .get(&target_workflow_id)
            .ok_or_else(|| stringify_error(&EngineError::WorkflowUnavailable))?;
        (
            workflow.dispatch_router.clone(),
            workflow.observability.clone(),
        )
    };

    let ctx = WorkflowContext::new(payload);
    let trace_id = ctx.trace_id.to_string();
    tracing::info!(workflow_id = %target_workflow_id, trace_id = %trace_id, "收到测试载荷提交请求");
    if let Err(error) = dispatch_router.submit_manual(ctx, "manual-dispatch").await {
        if let Some(store) = &observability_store {
            let _ = store
                .record_audit(
                    "error",
                    "dispatch",
                    "提交测试载荷失败",
                    Some(error.clone()),
                    Some(trace_id.clone()),
                    Some(json!({
                        "workflow_id": target_workflow_id,
                    })),
                )
                .await;
        }
        return Err(error);
    }

    if let Some(store) = &observability_store {
        let _ = store
            .record_audit(
                "info",
                "dispatch",
                "已提交测试载荷",
                Some(format!(
                    "workflow_id={target_workflow_id} · trace_id={trace_id}"
                )),
                Some(trace_id.clone()),
                Some(json!({
                    "workflow_id": target_workflow_id,
                })),
            )
            .await;
    }
    Ok(DispatchResponse {
        trace_id,
        workflow_id: Some(target_workflow_id),
    })
}
