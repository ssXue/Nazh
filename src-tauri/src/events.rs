use std::sync::Arc;

use nazh_engine::{ContextRef, DataStore, ExecutionEvent, WorkflowContext, WorkflowVariableEvent};
use serde::Serialize;
use store::{DeploymentAuditRecord, StoreHandle};
use tauri::{AppHandle, Emitter};
use tokio::sync::mpsc;

use crate::observability::SharedObservabilityStore;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct ScopedExecutionEvent {
    workflow_id: String,
    event: ExecutionEvent,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct ScopedWorkflowResult {
    workflow_id: String,
    result: WorkflowContext,
}

/// 写入变量审计记录到 `deployment_audit` 表（RFC-0003 Phase 3）。
async fn write_variable_audit(
    store: &StoreHandle,
    workflow_id: &str,
    action: &str,
    message: String,
    detail: Option<String>,
    data: serde_json::Value,
) {
    let now = chrono::Utc::now();
    let record = DeploymentAuditRecord {
        id: format!(
            "var-audit-{}-{}",
            now.timestamp_millis(),
            now.timestamp_subsec_nanos()
        ),
        workflow_id: workflow_id.to_owned(),
        action: action.to_owned(),
        level: "info".to_owned(),
        timestamp: now.to_rfc3339(),
        project_id: None,
        project_name: None,
        environment_id: None,
        environment_name: None,
        message,
        detail,
        data: Some(data),
    };
    if let Err(error) = store.insert_deployment_audit(record).await {
        tracing::debug!(?error, "变量审计记录写入失败");
    }
}

/// 变量控制事件 drain 循环。
///
/// 将 [`WorkflowVariableEvent`] 转发到 Tauri 前端事件通道
/// `workflow://variable-changed` / `workflow://variable-deleted`，
/// 同时持久化到 Store（ADR-0022）+ 写入审计记录（RFC-0003 Phase 3）。
pub(crate) fn spawn_variable_event_forwarder(
    app: AppHandle,
    _workflow_id: String,
    mut var_event_rx: mpsc::Receiver<WorkflowVariableEvent>,
    store: StoreHandle,
) -> tauri::async_runtime::JoinHandle<()> {
    tauri::async_runtime::spawn(async move {
        while let Some(event) = var_event_rx.recv().await {
            match event {
                WorkflowVariableEvent::Changed {
                    workflow_id,
                    name,
                    value,
                    updated_at,
                    updated_by,
                } => {
                    let payload = tauri_bindings::VariableChangedPayload {
                        workflow_id,
                        name,
                        value,
                        updated_at,
                        updated_by,
                    };
                    if let Err(error) = app.emit("workflow://variable-changed", &payload) {
                        tracing::warn!(?error, "workflow://variable-changed 事件转发失败");
                    }
                    if let Err(error) = store
                        .upsert_variable(
                            &payload.workflow_id,
                            &payload.name,
                            &payload.value,
                            "Any",
                            &payload.value,
                            &payload.updated_at,
                            payload.updated_by.as_deref(),
                        )
                        .await
                    {
                        tracing::debug!(?error, "变量持久化写入失败");
                    }
                    if let Err(error) = store
                        .record_history(
                            &payload.workflow_id,
                            &payload.name,
                            &payload.value,
                            &payload.updated_at,
                            payload.updated_by.as_deref(),
                        )
                        .await
                    {
                        tracing::debug!(?error, "变量历史记录写入失败");
                    }
                    write_variable_audit(
                        &store,
                        &payload.workflow_id,
                        "variable_changed",
                        format!("变量 {} 已变更", payload.name),
                        payload
                            .updated_by
                            .clone()
                            .or_else(|| "runtime".to_owned().into()),
                        serde_json::json!({
                            "variable_name": payload.name,
                            "value": payload.value,
                        }),
                    )
                    .await;
                }
                WorkflowVariableEvent::Deleted { workflow_id, name } => {
                    if let Err(error) = store.delete_variable(&workflow_id, &name).await {
                        tracing::debug!(?error, "变量持久化删除失败");
                    }
                    write_variable_audit(
                        &store,
                        &workflow_id,
                        "variable_deleted",
                        format!("变量 {name} 已删除"),
                        None,
                        serde_json::json!({
                            "variable_name": name,
                        }),
                    )
                    .await;
                    let payload = tauri_bindings::VariableDeletedPayload { workflow_id, name };
                    if let Err(error) = app.emit("workflow://variable-deleted", payload) {
                        tracing::warn!(?error, "workflow://variable-deleted 事件转发失败");
                    }
                }
            }
        }
    })
}

pub(crate) fn spawn_execution_event_forwarder(
    app: AppHandle,
    workflow_id: String,
    mut event_rx: mpsc::Receiver<ExecutionEvent>,
    observability: Option<SharedObservabilityStore>,
) -> tauri::async_runtime::JoinHandle<()> {
    tauri::async_runtime::spawn(async move {
        while let Some(event) = event_rx.recv().await {
            if let Some(store) = &observability {
                let _ = store.record_execution_event(&event).await;
            }
            let _ = app.emit(
                "workflow://node-status",
                ScopedExecutionEvent {
                    workflow_id: workflow_id.clone(),
                    event,
                },
            );
        }
    })
}

pub(crate) fn spawn_result_forwarder(
    app: AppHandle,
    workflow_id: String,
    mut result_rx: mpsc::Receiver<ContextRef>,
    store_ref: Arc<dyn DataStore>,
    observability: Option<SharedObservabilityStore>,
) -> tauri::async_runtime::JoinHandle<()> {
    tauri::async_runtime::spawn(async move {
        while let Some(ctx_ref) = result_rx.recv().await {
            let Ok(payload) = store_ref.read(&ctx_ref.data_id) else {
                continue;
            };
            store_ref.release(&ctx_ref.data_id);
            let result = WorkflowContext::from_parts(
                ctx_ref.trace_id,
                ctx_ref.timestamp,
                (*payload).clone(),
            );
            if let Some(store) = &observability {
                store.record_result(&result);
            }
            let _ = app.emit(
                "workflow://result",
                ScopedWorkflowResult {
                    workflow_id: workflow_id.clone(),
                    result,
                },
            );
        }
    })
}
