use tauri::{AppHandle, Emitter, State};
use tokio::fs;

use crate::{
    runtime::{DEAD_LETTER_DIR, DEAD_LETTER_FILE, DeadLetterRecord, RuntimeWorkflowSummary},
    state::DesktopState,
    workspace::resolve_project_workspace_dir,
};

#[tauri::command]
pub(crate) async fn list_runtime_workflows(
    state: State<'_, DesktopState>,
) -> Result<Vec<RuntimeWorkflowSummary>, String> {
    let active_workflow_id = state.active_workflow_id.lock().await.clone();
    let workflows = state.workflows.lock().await;
    let mut summaries = workflows
        .values()
        .map(|workflow| {
            workflow.summary(active_workflow_id.as_deref() == Some(workflow.workflow_id.as_str()))
        })
        .collect::<Vec<_>>();
    summaries.sort_by(|left, right| right.deployed_at.cmp(&left.deployed_at));
    Ok(summaries)
}

#[tauri::command]
pub(crate) async fn set_active_runtime_workflow(
    app: AppHandle,
    state: State<'_, DesktopState>,
    workflow_id: String,
) -> Result<RuntimeWorkflowSummary, String> {
    let workflow_id = workflow_id.trim();
    if workflow_id.is_empty() {
        return Err("workflow_id 不能为空".to_owned());
    }

    let summary = {
        let workflows = state.workflows.lock().await;
        let workflow = workflows
            .get(workflow_id)
            .ok_or_else(|| format!("运行中的工作流 `{workflow_id}` 不存在"))?;
        workflow.summary(true)
    };

    {
        let mut active_workflow_id = state.active_workflow_id.lock().await;
        *active_workflow_id = Some(workflow_id.to_owned());
    }

    if let Some(workflow) = state.workflows.lock().await.get(workflow_id)
        && let Some(store) = &workflow.observability
    {
        let _ = store
            .record_audit(
                "info",
                "runtime",
                "已切换当前工作流",
                Some(format!("workflow_id={workflow_id}")),
                None,
                None,
            )
            .await;
    }

    let _ = app.emit("workflow://runtime-focus", summary.clone());
    Ok(summary)
}

#[tauri::command]
pub(crate) async fn list_dead_letters(
    app: AppHandle,
    workspace_path: Option<String>,
    workflow_id: Option<String>,
    limit: Option<usize>,
) -> Result<Vec<DeadLetterRecord>, String> {
    let (workspace_dir, _) = resolve_project_workspace_dir(&app, workspace_path.as_deref())?;
    let file_path = workspace_dir.join(DEAD_LETTER_DIR).join(DEAD_LETTER_FILE);
    if !file_path.exists() {
        return Ok(Vec::new());
    }

    let text = fs::read_to_string(&file_path)
        .await
        .map_err(|error| format!("读取 dead-letter 文件失败: {error}"))?;
    let workflow_filter = workflow_id
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty());
    let max_items = limit.unwrap_or(120).clamp(1, 1_000);
    let mut records = text
        .lines()
        .filter_map(|line| serde_json::from_str::<DeadLetterRecord>(line).ok())
        .filter(|record| {
            workflow_filter
                .as_ref()
                .is_none_or(|filter| record.workflow_id == *filter)
        })
        .collect::<Vec<_>>();

    records.sort_by(|left, right| right.timestamp.cmp(&left.timestamp));
    records.truncate(max_items);
    Ok(records)
}
