use std::sync::atomic::{AtomicU64, Ordering};

use serde_json::Value;
use tokio::sync::mpsc;

/// 工作流变量控制事件（从执行事件中分离，B1-R0-01/B1-R0-05）。
///
/// 变量控制属于配置平面（ADR-0012），与执行可观测性（节点状态、边传输等）是不同的关注点。
/// 本类型拥有独立的事件通道，通过 Tauri shell 独立转发到
/// `workflow://variable-changed` / `workflow://variable-deleted` 前端事件。
#[derive(Debug, Clone, PartialEq)]
pub enum WorkflowVariableEvent {
    /// 变量值变更（ADR-0012 Phase 2，write-on-change 语义）。
    ///
    /// 仅当 `set` / `compare_and_swap` 检测到 `entry.value != new` 时 emit；
    /// 写入相同值不触发本事件（避免轮询脚本制造事件刷屏）。
    Changed {
        workflow_id: String,
        name: String,
        value: Value,
        updated_at: String,
        updated_by: Option<String>,
    },
    /// 变量被删除（ADR-0012 Phase 3）。
    Deleted { workflow_id: String, name: String },
}

/// 非阻塞发送变量控制事件。
///
/// 使用 `try_send`（非阻塞），保证变量写入不阻塞。
/// 通道满或关闭时通过 `tracing::error!` 报告——事件丢失即丢帧，不可接受。
pub fn emit_variable_event(tx: &mpsc::Sender<WorkflowVariableEvent>, event: WorkflowVariableEvent) {
    if let Err(err) = tx.try_send(event) {
        record_variable_event_send_failure(err, None, None, None, "unknown");
    }
}

pub(super) struct EventSink {
    pub(super) workflow_id: String,
    pub(super) sender: mpsc::Sender<WorkflowVariableEvent>,
    pub(super) dropped_events: AtomicU64,
}

impl std::fmt::Debug for EventSink {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("EventSink")
            .field("workflow_id", &self.workflow_id)
            .finish_non_exhaustive()
    }
}

impl EventSink {
    pub(super) fn new(workflow_id: String, sender: mpsc::Sender<WorkflowVariableEvent>) -> Self {
        Self {
            workflow_id,
            sender,
            dropped_events: AtomicU64::new(0),
        }
    }

    pub(super) fn emit(
        &self,
        variable_name: &str,
        event_kind: &'static str,
        event: WorkflowVariableEvent,
    ) {
        if let Err(err) = self.sender.try_send(event) {
            record_variable_event_send_failure(
                err,
                Some(&self.dropped_events),
                Some(&self.workflow_id),
                Some(variable_name),
                event_kind,
            );
        }
    }
}

fn record_variable_event_send_failure(
    err: mpsc::error::TrySendError<WorkflowVariableEvent>,
    dropped_events: Option<&AtomicU64>,
    workflow_id: Option<&str>,
    variable_name: Option<&str>,
    event_kind: &'static str,
) {
    let dropped_total =
        dropped_events.map_or(0, |counter| counter.fetch_add(1, Ordering::Relaxed) + 1);
    match err {
        mpsc::error::TrySendError::Full(dropped) => {
            tracing::error!(
                workflow_id = workflow_id.unwrap_or("<unknown>"),
                variable_name = variable_name.unwrap_or("<unknown>"),
                event_kind,
                dropped_total,
                ?dropped,
                "变量事件通道已满，事件被丢弃"
            );
        }
        mpsc::error::TrySendError::Closed(dropped) => {
            tracing::error!(
                workflow_id = workflow_id.unwrap_or("<unknown>"),
                variable_name = variable_name.unwrap_or("<unknown>"),
                event_kind,
                dropped_total,
                ?dropped,
                "变量事件通道已关闭，事件消费者可能已崩溃"
            );
        }
    }
}
