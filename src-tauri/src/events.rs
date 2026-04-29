use std::sync::Arc;

use nazh_engine::{ContextRef, DataStore, ExecutionEvent, WorkflowContext};
use serde::Serialize;
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
) -> tauri::async_runtime::JoinHandle<()> {
    tauri::async_runtime::spawn(async move {
        while let Some(event) = event_rx.recv().await {
            // ADR-0012 Phase 2：变量变更使用独立事件通道，避免前端从节点状态流中过滤。
            if matches!(event, ExecutionEvent::VariableChanged { .. }) {
                if let Some(payload) = tauri_bindings::variable_changed_payload(event)
                    && let Err(error) = app.emit("workflow://variable-changed", payload)
                {
                    tracing::warn!(?error, "workflow://variable-changed 事件转发失败");
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
