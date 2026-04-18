//! 单阶段异步执行循环与事件发射。
//!
//! [`run_stage`] 在独立的 Tokio 任务中运行，持续从输入通道接收上下文，
//! 执行阶段处理器，并将结果转发到下一阶段或最终结果通道。
//! 所有执行阶段都带有 panic 隔离和可选超时保护。

use tokio::sync::mpsc;
use tracing::Instrument;

use super::types::PipelineStage;
use nazh_core::{
    EngineError, WorkflowContext,
    event::{ExecutionEvent, emit_event, emit_failure},
    guard::guarded_execute,
};

/// 单阶段的异步执行循环。
pub(crate) async fn run_stage(
    stage: PipelineStage,
    mut input_rx: mpsc::Receiver<WorkflowContext>,
    output_tx: Option<mpsc::Sender<WorkflowContext>>,
    result_tx: mpsc::Sender<WorkflowContext>,
    event_tx: mpsc::Sender<ExecutionEvent>,
) {
    let stage_name = stage.name.clone();
    while let Some(ctx) = input_rx.recv().await {
        let trace_id = ctx.trace_id;

        emit_event(
            &event_tx,
            ExecutionEvent::Started {
                stage: stage_name.clone(),
                trace_id,
            },
        );

        let span = tracing::info_span!(
            "stage.execute",
            stage = %stage_name,
            trace_id = %trace_id,
        );
        let result = guarded_execute(&stage_name, trace_id, stage.timeout, (stage.handler)(ctx))
            .instrument(span)
            .await;

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
                            ExecutionEvent::Completed(nazh_core::CompletedExecutionEvent {
                                stage: stage_name.clone(),
                                trace_id,
                                metadata: None,
                            }),
                        );

                        if output_tx.is_none() {
                            emit_event(&event_tx, ExecutionEvent::Finished { trace_id });
                        }
                    }
                    Err(error) => {
                        emit_failure(&event_tx, &stage_name, trace_id, &error);
                        break;
                    }
                }
            }
            Err(error) => {
                emit_failure(&event_tx, &stage_name, trace_id, &error);
            }
        }
    }
}
