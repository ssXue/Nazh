//! 单节点异步执行循环与事件发射。
//!
//! [`run_node`] 在独立的 Tokio 任务中运行，持续从输入通道接收 [`ContextRef`]，
//! 从 [`DataStore`] 读取数据，执行节点逻辑，将输出写回 `DataStore`，
//! 并将新的 [`ContextRef`] 分发到下游或结果流。
//! 所有执行阶段都带有 panic 隔离和可选超时保护。

use std::{sync::Arc, time::Duration};

use tokio::sync::mpsc;
use tracing::Instrument;

use nazh_core::{
    event::{emit_event, emit_failure},
    guard::guarded_execute,
    ContextRef, DataStore, EngineError, ExecutionEvent, NodeDispatch, NodeTrait,
};

use super::types::DownstreamTarget;

/// 单节点的异步执行循环：接收 [`ContextRef`] → 读取数据 → 执行 → 写入输出 → 分发。
#[allow(clippy::too_many_lines)]
pub(crate) async fn run_node(
    node: Arc<dyn NodeTrait>,
    timeout: Option<Duration>,
    mut input_rx: mpsc::Receiver<ContextRef>,
    downstream_senders: Vec<DownstreamTarget>,
    result_tx: mpsc::Sender<ContextRef>,
    event_tx: mpsc::Sender<ExecutionEvent>,
    store: Arc<dyn DataStore>,
) {
    let node_id = node.id().to_owned();

    while let Some(ctx_ref) = input_rx.recv().await {
        let trace_id = ctx_ref.trace_id;

        emit_event(
            &event_tx,
            ExecutionEvent::Started {
                stage: node_id.clone(),
                trace_id,
            },
        )
        .await;

        let span = tracing::info_span!(
            "node.execute",
            node_id = %node_id,
            trace_id = %trace_id,
        );
        let result = guarded_execute(&node_id, trace_id, timeout, node.execute(&ctx_ref, &*store))
            .instrument(span)
            .await;

        // 释放本节点对输入数据的引用
        store.release(&ctx_ref.data_id);

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

                    // 将节点输出写入 DataStore，消费者数为下游目标数
                    let consumer_count = if matching_targets.is_empty() {
                        1 // 叶节点结果
                    } else {
                        matching_targets.len()
                    };

                    let data_id = match store.write(node_output.payload, consumer_count) {
                        Ok(id) => id,
                        Err(error) => {
                            send_error = Some(error);
                            break;
                        }
                    };

                    let new_ref = ContextRef::new(
                        trace_id,
                        data_id,
                        Some(node_id.clone()),
                    );

                    let write_result = if matching_targets.is_empty() {
                        result_tx.send(new_ref).await.map_err(|_| {
                            EngineError::ChannelClosed {
                                stage: node_id.clone(),
                            }
                        })
                    } else {
                        let mut downstream_error = None;
                        for target in &matching_targets {
                            if target.sender.send(new_ref.clone()).await.is_err() {
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
