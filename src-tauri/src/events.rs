use std::sync::Arc;

use nazh_engine::{ContextRef, DataStore, ExecutionEvent, WorkflowContext};
use serde::Serialize;
use store::Store;
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

pub(crate) fn spawn_execution_event_forwarder(
    app: AppHandle,
    workflow_id: String,
    mut event_rx: mpsc::Receiver<ExecutionEvent>,
    observability: Option<SharedObservabilityStore>,
    store: Arc<Store>,
) -> tauri::async_runtime::JoinHandle<()> {
    tauri::async_runtime::spawn(async move {
        while let Some(event) = event_rx.recv().await {
            // ADR-0012 Phase 2：变量变更使用独立事件通道，避免前端从节点状态流中过滤。
            // ADR-0022：同时持久化到 Store。
            if matches!(event, ExecutionEvent::VariableChanged { .. }) {
                if let Some(payload) = tauri_bindings::variable_changed_payload(event) {
                    if let Err(error) = app.emit("workflow://variable-changed", &payload) {
                        tracing::warn!(?error, "workflow://variable-changed 事件转发失败");
                    }
                    if let Err(error) = store.upsert_variable(
                        &payload.workflow_id,
                        &payload.name,
                        &payload.value,
                        "Any",
                        &payload.value,
                        &payload.updated_at,
                        payload.updated_by.as_deref(),
                    ) {
                        tracing::debug!(?error, "变量持久化写入失败");
                    }
                    if let Err(error) = store.record_history(
                        &payload.workflow_id,
                        &payload.name,
                        &payload.value,
                        &payload.updated_at,
                        payload.updated_by.as_deref(),
                    ) {
                        tracing::debug!(?error, "变量历史记录写入失败");
                    }
                }
                continue;
            }

            // ADR-0012 Phase 3：变量删除走独立事件通道。
            if matches!(event, ExecutionEvent::VariableDeleted { .. }) {
                if let Some(payload) = tauri_bindings::variable_deleted_payload(event) {
                    if let Err(error) = store.delete_variable(&payload.workflow_id, &payload.name) {
                        tracing::debug!(?error, "变量持久化删除失败");
                    }
                    if let Err(error) = app.emit("workflow://variable-deleted", payload) {
                        tracing::warn!(?error, "workflow://variable-deleted 事件转发失败");
                    }
                }
                continue;
            }

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
                let _ = store.record_result(&result).await;
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
