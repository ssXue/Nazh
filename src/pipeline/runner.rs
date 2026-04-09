//! 单阶段异步执行循环与事件发射。
//!
//! [`run_stage`] 在独立的 Tokio 任务中运行，持续从输入通道接收上下文，
//! 执行阶段处理器，并将结果转发到下一阶段或最终结果通道。
//! 所有执行阶段都带有 panic 隔离和可选超时保护。

use std::panic::AssertUnwindSafe;

use futures_util::FutureExt;
use tokio::sync::mpsc;

use super::types::{PipelineEvent, PipelineStage};
use crate::{EngineError, WorkflowContext};

/// 单阶段的异步执行循环。
pub(crate) async fn run_stage(
    stage: PipelineStage,
    mut input_rx: mpsc::Receiver<WorkflowContext>,
    output_tx: Option<mpsc::Sender<WorkflowContext>>,
    result_tx: mpsc::Sender<WorkflowContext>,
    event_tx: mpsc::Sender<PipelineEvent>,
) {
    while let Some(ctx) = input_rx.recv().await {
        let trace_id = ctx.trace_id;
        let stage_name = stage.name.clone();

        emit_event(
            &event_tx,
            PipelineEvent::StageStarted {
                stage: stage_name.clone(),
                trace_id,
            },
        )
        .await;

        let execution = AssertUnwindSafe((stage.handler)(ctx)).catch_unwind();

        let result = if let Some(timeout) = stage.timeout {
            match tokio::time::timeout(timeout, execution).await {
                Ok(Ok(outcome)) => outcome,
                Ok(Err(_)) => Err(EngineError::StagePanicked {
                    stage: stage_name.clone(),
                    trace_id,
                }),
                Err(_) => Err(EngineError::StageTimeout {
                    stage: stage_name.clone(),
                    trace_id,
                    timeout_ms: timeout.as_millis(),
                }),
            }
        } else {
            match execution.await {
                Ok(outcome) => outcome,
                Err(_) => Err(EngineError::StagePanicked {
                    stage: stage_name.clone(),
                    trace_id,
                }),
            }
        };

        match result {
            Ok(next_ctx) => {
                let forward_result = if let Some(tx) = &output_tx {
                    tx.send(next_ctx)
                        .await
                        .map_err(|_| EngineError::ChannelClosed {
                            stage: stage_name.clone(),
                        })
                } else {
                    result_tx
                        .send(next_ctx)
                        .await
                        .map_err(|_| EngineError::ChannelClosed {
                            stage: stage_name.clone(),
                        })
                };

                match forward_result {
                    Ok(()) => {
                        emit_event(
                            &event_tx,
                            PipelineEvent::StageCompleted {
                                stage: stage_name.clone(),
                                trace_id,
                            },
                        )
                        .await;

                        if output_tx.is_none() {
                            emit_event(&event_tx, PipelineEvent::PipelineCompleted { trace_id })
                                .await;
                        }
                    }
                    Err(error) => {
                        emit_failure(&event_tx, &stage_name, trace_id, &error).await;
                        break;
                    }
                }
            }
            Err(error) => {
                emit_failure(&event_tx, &stage_name, trace_id, &error).await;
            }
        }
    }
}

async fn emit_failure(
    event_tx: &mpsc::Sender<PipelineEvent>,
    stage: &str,
    trace_id: uuid::Uuid,
    error: &EngineError,
) {
    emit_event(
        event_tx,
        PipelineEvent::StageFailed {
            stage: stage.to_owned(),
            trace_id,
            error: error.to_string(),
        },
    )
    .await;
}

async fn emit_event(event_tx: &mpsc::Sender<PipelineEvent>, event: PipelineEvent) {
    let _ = event_tx.send(event).await;
}
