//! 统一的执行生命周期事件与事件发射辅助。
//!
//! [`ExecutionEvent`] 覆盖 DAG 工作流和线性流水线两种执行模式，
//! 替代原先独立的 `WorkflowEvent` 和 `PipelineEvent`。

use serde::{Deserialize, Serialize};
use tokio::sync::mpsc;
use ts_rs::TS;
use uuid::Uuid;

use crate::error::EngineError;

/// 统一的执行生命周期事件。
///
/// DAG 工作流和线性流水线共享同一事件类型，
/// 前端只需注册一个事件监听器即可处理所有执行模式。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[ts(export)]
pub enum ExecutionEvent {
    /// 阶段/节点开始执行。
    Started { stage: String, trace_id: Uuid },
    /// 阶段/节点执行完成。
    Completed { stage: String, trace_id: Uuid },
    /// 阶段/节点执行失败。
    Failed {
        stage: String,
        trace_id: Uuid,
        error: String,
    },
    /// 叶节点产出最终结果（仅 DAG 工作流模式下发出）。
    Output { stage: String, trace_id: Uuid },
    /// 整条流水线执行完毕（仅线性流水线模式下发出）。
    Finished { trace_id: Uuid },
}

/// 向事件通道发送执行事件（忽略发送失败）。
pub async fn emit_event(tx: &mpsc::Sender<ExecutionEvent>, event: ExecutionEvent) {
    let _ = tx.send(event).await;
}

/// 向事件通道发送失败事件。
pub async fn emit_failure(
    tx: &mpsc::Sender<ExecutionEvent>,
    stage: &str,
    trace_id: Uuid,
    error: &EngineError,
) {
    emit_event(
        tx,
        ExecutionEvent::Failed {
            stage: stage.to_owned(),
            trace_id,
            error: error.to_string(),
        },
    )
    .await;
}
