use tauri::{AppHandle, Emitter, State};
use tauri_bindings::{DeadLetterRecord, RuntimeWorkflowSummary};
use tokio::fs;

use crate::{
    runtime::{DEAD_LETTER_DIR, DEAD_LETTER_FILE},
    state::DesktopState,
    workspace::resolve_project_workspace_dir,
};

/// 订阅指定工作流节点的 Reactive 输出引脚值变更（ADR-0015 Phase 2）。
///
/// 后台启动 watch task：OutputCache slot 值变化时通过
/// `workflow://reactive-update/{workflow_id}/{node_id}/{pin_id}` 推送到前端。
#[tauri::command]
pub(crate) async fn subscribe_reactive_pin(
    state: State<'_, DesktopState>,
    app: tauri::AppHandle,
    workflow_id: String,
    node_id: String,
    pin_id: String,
) -> Result<(), String> {
    let cache = {
        let workflows = state.workflows.lock().await;
        let deployment = workflows
            .get(&workflow_id)
            .ok_or_else(|| format!("工作流 `{workflow_id}` 未部署"))?;
        deployment
            .output_cache(&node_id)
            .ok_or_else(|| format!("节点 `{node_id}` 无 OutputCache"))?
    };

    let rx = cache
        .subscribe(&pin_id)
        .ok_or_else(|| format!("引脚 `{pin_id}` 无缓存槽位"))?;

    let event_channel = format!("workflow://reactive-update/{workflow_id}/{node_id}/{pin_id}");

    tokio::spawn(async move {
        let mut rx = rx;
        while rx.changed().await.is_ok() {
            let snapshot = rx.borrow().clone();
            if let Some(cached) = snapshot {
                let payload = tauri_bindings::ReactiveUpdatePayload {
                    workflow_id: workflow_id.clone(),
                    node_id: node_id.clone(),
                    pin_id: pin_id.clone(),
                    value: cached.value,
                    updated_at: cached.produced_at.to_rfc3339(),
                };
                let _ = app.emit(&event_channel, payload);
            }
        }
    });

    Ok(())
}

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
