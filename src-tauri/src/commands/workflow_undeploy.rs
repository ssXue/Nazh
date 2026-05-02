use serde_json::json;
use tauri::{AppHandle, Emitter, State};
use tauri_bindings::UndeployResponse;

use crate::state::DesktopState;

#[tauri::command]
pub(crate) async fn undeploy_workflow(
    app: AppHandle,
    state: State<'_, DesktopState>,
    workflow_id: Option<String>,
) -> Result<UndeployResponse, String> {
    let target_workflow_id = state.resolve_workflow_id(workflow_id.as_deref()).await?;
    tracing::info!(workflow_id = ?target_workflow_id, "收到停止运行请求");
    let Some(target_workflow_id) = target_workflow_id else {
        let response = UndeployResponse {
            had_workflow: false,
            aborted_timer_count: 0,
            workflow_id: None,
        };
        let _ = app.emit("workflow://undeployed", response.clone());
        return Ok(response);
    };

    let active_before = state.active_workflow_id.lock().await.clone();
    let (existing_workflow, removed_observability) = {
        let mut workflows = state.workflows.lock().await;
        let removed = workflows.remove(&target_workflow_id);
        let observability = removed
            .as_ref()
            .and_then(|workflow| workflow.observability.clone());
        (removed, observability)
    };

    let response = if let Some(mut workflow) = existing_workflow {
        let aborted = workflow.shutdown_runtime().await;
        state
            .approval_registry
            .cleanup_workflow(&target_workflow_id);
        UndeployResponse {
            had_workflow: true,
            aborted_timer_count: aborted,
            workflow_id: Some(target_workflow_id.clone()),
        }
    } else {
        UndeployResponse {
            had_workflow: false,
            aborted_timer_count: 0,
            workflow_id: Some(target_workflow_id.clone()),
        }
    };

    let remaining_workflow_count = state.workflows.lock().await.len();
    if remaining_workflow_count == 0 {
        state
            .connection_manager
            .mark_all_idle("运行已停止，连接会话已回收到空闲态")
            .await;
    }

    let mut fallback_summary = None;
    if active_before.as_deref() == Some(target_workflow_id.as_str()) {
        let fallback_active = state.choose_fallback_active_workflow().await;
        let mut active_workflow_id = state.active_workflow_id.lock().await;
        (*active_workflow_id).clone_from(&fallback_active);
        drop(active_workflow_id);

        if let Some(fallback_workflow_id) = fallback_active {
            let workflows = state.workflows.lock().await;
            fallback_summary = workflows
                .get(&fallback_workflow_id)
                .map(|workflow| workflow.summary(true));
        }
    }

    if let Some(store) = removed_observability {
        let _ = store
            .record_audit(
                if response.had_workflow {
                    "warn"
                } else {
                    "info"
                },
                "workflow",
                if response.had_workflow {
                    "运行已停止"
                } else {
                    "停止请求未命中已部署工作流"
                },
                Some(format!(
                    "workflow_id={} · 已中止 {} 个根触发任务",
                    target_workflow_id, response.aborted_timer_count
                )),
                None,
                Some(json!({
                    "workflow_id": target_workflow_id,
                    "remaining_workflow_count": remaining_workflow_count,
                })),
            )
            .await;
    }

    if active_before.as_deref() == Some(target_workflow_id.as_str()) {
        let _ = app.emit("workflow://undeployed", response.clone());
        if let Some(summary) = fallback_summary {
            let _ = app.emit("workflow://runtime-focus", summary);
        }
    }
    Ok(response)
}
