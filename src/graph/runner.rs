//! 单节点异步执行循环与事件发射。
//!
//! [`run_node`] 在独立的 Tokio 任务中运行，持续从输入通道接收 [`ContextRef`]，
//! 从 [`DataStore`] 读取数据，调用节点的 [`transform`](nazh_core::NodeTrait::transform)
//! 方法，将输出写回 `DataStore`，并将新的 [`ContextRef`] 分发到下游或结果流。
//!
//! ## 元数据通道
//!
//! 节点返回的 [`nazh_core::NodeOutput::metadata`] 不进入 payload，而是通过
//! [`ExecutionEvent::Completed`] 事件独立传递给前端，实现业务数据与执行元数据的完全分离。

use std::{sync::Arc, time::Duration};

use tokio::sync::mpsc;
use tracing::Instrument;

use nazh_core::{
    ContextRef, DataStore, EngineError, ExecutionEvent, NodeDispatch, NodeTrait,
    event::{emit_event, emit_failure},
    guard::guarded_execute,
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
        );

        let payload_result = store.read_mut(&ctx_ref.data_id);
        store.release(&ctx_ref.data_id);

        let payload = match payload_result {
            Ok(p) => p,
            Err(error) => {
                emit_failure(&event_tx, &node_id, trace_id, &error);
                continue;
            }
        };

        let span = tracing::info_span!(
            "node.transform",
            node_id = %node_id,
            trace_id = %trace_id,
        );
        let result = guarded_execute(
            &node_id,
            trace_id,
            timeout,
            node.transform(trace_id, payload),
        )
        .instrument(span)
        .await;

        match result {
            Ok(output) => {
                let mut send_error = None;
                let mut merged_metadata = serde_json::Map::new();

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

                    for (key, value) in node_output.metadata {
                        merged_metadata.insert(key, value);
                    }

                    // 将纯业务 payload 写入 DataStore
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

                    let new_ref = ContextRef::new(trace_id, data_id, Some(node_id.clone()));

                    let write_result = if matching_targets.is_empty() {
                        result_tx
                            .send(new_ref)
                            .await
                            .map_err(|_| EngineError::ChannelClosed {
                                stage: node_id.clone(),
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
                                );
                            }
                        }
                        Err(error) => {
                            send_error = Some(error);
                            break;
                        }
                    }
                }

                if let Some(error) = send_error {
                    emit_failure(&event_tx, &node_id, trace_id, &error);
                    break;
                }

                emit_event(
                    &event_tx,
                    ExecutionEvent::Completed(crate::CompletedExecutionEvent {
                        stage: node_id.clone(),
                        trace_id,
                        metadata: if merged_metadata.is_empty() {
                            None
                        } else {
                            Some(merged_metadata)
                        },
                    }),
                );
            }
            Err(error) => {
                emit_failure(&event_tx, &node_id, trace_id, &error);
            }
        }
    }
}
