//! 单节点异步执行循环与事件发射。
//!
//! [`run_node`] 在独立的 Tokio 任务中运行，持续从输入通道接收上下文，
//! 执行节点逻辑，并根据 [`NodeDispatch`] 将输出分发到下游或结果流。
//! 所有执行阶段都带有 panic 隔离和可选超时保护。

use std::{sync::Arc, time::Duration};

use tokio::sync::mpsc;

use super::types::DownstreamTarget;
use crate::{
    event::{emit_event, emit_failure, ExecutionEvent},
    guard::guarded_execute,
    EngineError, NodeDispatch, NodeTrait, WorkflowContext,
};

/// 单节点的异步执行循环：接收 → 执行 → 分发 → 发射事件。
pub(crate) async fn run_node(
    node: Arc<dyn NodeTrait>,
    timeout: Option<Duration>,
    mut input_rx: mpsc::Receiver<WorkflowContext>,
    downstream_senders: Vec<DownstreamTarget>,
    result_tx: mpsc::Sender<WorkflowContext>,
    event_tx: mpsc::Sender<ExecutionEvent>,
) {
    let node_id = node.id().to_owned();

    while let Some(ctx) = input_rx.recv().await {
        let trace_id = ctx.trace_id;

        emit_event(
            &event_tx,
            ExecutionEvent::Started {
                stage: node_id.clone(),
                trace_id,
            },
        )
        .await;

        let result = guarded_execute(&node_id, trace_id, timeout, node.execute(ctx)).await;

        match result {
            Ok(output) => {
                let mut send_error = None;

                for node_output in output.outputs {
                    let matching_targets = match &node_output.dispatch {
                        NodeDispatch::Broadcast => downstream_senders.iter().collect::<Vec<_>>(),
                        NodeDispatch::Route(port_ids) => downstream_senders
                            .iter()
                            .filter(|target| {
                                target.source_port_id.as_ref().is_some_and(|port_id| {
                                    port_ids.iter().any(|candidate| candidate == port_id)
                                })
                            })
                            .collect::<Vec<_>>(),
                    };

                    let write_result = if matching_targets.is_empty() {
                        result_tx.send(node_output.ctx).await.map_err(|_| {
                            EngineError::ChannelClosed {
                                stage: node_id.clone(),
                            }
                        })
                    } else {
                        let mut downstream_error = None;
                        for target in &matching_targets {
                            if target.sender.send(node_output.ctx.clone()).await.is_err() {
                                downstream_error = Some(EngineError::ChannelClosed {
                                    stage: node_id.clone(),
                                });
                                break;
                            }
                        }
                        if let Some(error) = downstream_error {
                            Err(error)
                        } else {
                            Ok(())
                        }
                    };

                    match write_result {
                        Ok(()) => {
                            if matching_targets.is_empty() {
                                emit_event(
                                    &event_tx,
                                    ExecutionEvent::Output {
                                        stage: node_id.clone(),
                                        trace_id,
                                    },
                                )
                                .await;
                            }
                        }
                        Err(error) => {
                            send_error = Some(error);
                            break;
                        }
                    }
                }

                if let Some(error) = send_error {
                    emit_failure(&event_tx, &node_id, trace_id, &error).await;
                    break;
                }

                emit_event(
                    &event_tx,
                    ExecutionEvent::Completed {
                        stage: node_id.clone(),
                        trace_id,
                    },
                )
                .await;
            }
            Err(error) => {
                emit_failure(&event_tx, &node_id, trace_id, &error).await;
            }
        }
    }
}
