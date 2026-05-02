//! HITL 审批 IPC 命令。

use tauri::State;

use crate::state::DesktopState;
use nazh_engine::{HumanLoopResponse, ResponseAction};

/// 人工响应审批。
#[tauri::command]
pub(crate) async fn respond_human_loop(
    state: State<'_, DesktopState>,
    approval_id: String,
    action: String,
    form_data: serde_json::Value,
    comment: Option<String>,
    responded_by: Option<String>,
) -> Result<(), String> {
    let response_action = match action.as_str() {
        "approved" => ResponseAction::Approved,
        "rejected" => ResponseAction::Rejected,
        other => return Err(format!("未知动作: {other}")),
    };
    let response = HumanLoopResponse {
        action: response_action,
        form_data,
        comment,
        responded_by,
    };
    let approval_uuid =
        uuid::Uuid::parse_str(&approval_id).map_err(|e| format!("无效的 approval_id: {e}"))?;
    state
        .approval_registry
        .respond(approval_uuid, response)
        .map_err(|e| e.to_string())
}

/// 列出 pending 审批。
#[tauri::command]
pub(crate) async fn list_pending_approvals(
    state: State<'_, DesktopState>,
    workflow_id: Option<String>,
) -> Result<Vec<serde_json::Value>, String> {
    let summaries = state.approval_registry.list_pending(workflow_id.as_deref());
    Ok(summaries
        .into_iter()
        .map(|s| serde_json::to_value(s).unwrap_or(serde_json::Value::Null))
        .collect())
}
