//! 单节点异步执行循环与事件发射。
//!
//! [`run_node`] 在独立的 Tokio 任务中运行，持续从输入通道接收 [`ContextRef`]，
//! 从 [`DataStore`] 读取数据，调用节点的 [`transform`](nazh_core::NodeTrait::transform)
//! 方法，将输出写回 `DataStore`，并将新的 [`ContextRef`] 分发到下游或结果流。
//!
//! 输出走三分支路径（ADR-0014 + ADR-0015）：
//! - **Data 引脚**：仅写 [`OutputCache`] 槽位，不推 ContextRef（下游拉取）。
//! - **Reactive 引脚**：写 [`OutputCache`] 槽位 **+** 推 ContextRef（Data + Exec 并集）。
//! - **Exec 引脚**：仅推 ContextRef，不写缓存。
//!
//! 节点返回的
//! [`NodeOutput::metadata`](nazh_core::NodeOutput::metadata) 不进入 payload，
//! 而是通过 [`ExecutionEvent::Completed`] 事件独立传递给前端。

use std::{collections::HashMap, collections::HashSet, sync::Arc, time::Duration};

use tokio::sync::mpsc;
use tracing::Instrument;

use nazh_core::{
    ContextRef, DataStore, EngineError, ExecutionEvent, NodeDispatch, NodeTrait, OutputCache,
    event::{emit_event, emit_failure},
    guard::guarded_execute,
};

use super::pull::{EdgesByConsumer, PureMemo};
use super::types::DownstreamTarget;

/// 单节点的异步执行循环：接收 [`ContextRef`] → 读取数据 → 执行 → 写入输出 → 分发。
#[allow(clippy::too_many_lines, clippy::too_many_arguments)]
pub(crate) async fn run_node(
    node: Arc<dyn NodeTrait>,
    timeout: Option<Duration>,
    mut input_rx: mpsc::Receiver<ContextRef>,
    downstream_senders: Vec<DownstreamTarget>,
    result_tx: mpsc::Sender<ContextRef>,
    event_tx: mpsc::Sender<ExecutionEvent>,
    store: Arc<dyn DataStore>,
    output_cache: Arc<OutputCache>,
    data_output_pin_ids: HashSet<String>,
    // ADR-0014 Phase 3：拉路径所需
    edges_by_consumer: Arc<EdgesByConsumer>,
    nodes_index: Arc<HashMap<String, Arc<dyn NodeTrait>>>,
    output_caches_index: Arc<HashMap<String, Arc<OutputCache>>>,
    node_timeouts_index: Arc<HashMap<String, Option<Duration>>>,
    // ADR-0014 Phase 4：Pure memo
    pure_memo: Arc<PureMemo>,
    // ADR-0015 Phase 1：Reactive 边三分支 dispatch
    reactive_output_pin_ids: HashSet<String>,
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

        // ADR-0014 Phase 3：transform 之前先把所有 Data 输入引脚的最新值拉到
        // payload 里。pull collector 不动 Exec 路径——若节点没声明 Data 输入则
        // edges_by_consumer.for_consumer 返回空，merge_payload 直接返回原 payload。
        let payload = match super::pull::pull_data_inputs(
            &node_id,
            node.as_ref(),
            payload,
            &edges_by_consumer,
            &nodes_index,
            &output_caches_index,
            &node_timeouts_index,
            &pure_memo,
            trace_id,
        )
        .await
        {
            Ok(merged) => merged,
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
                    // Data + Reactive 缓存写（不 push）。Reactive = Data（写缓存）+ Exec（推 ContextRef），
                    // 缓存写在统一路径完成（ADR-0015 Phase 1）。
                    if !data_output_pin_ids.is_empty() || !reactive_output_pin_ids.is_empty() {
                        let cache_pins_to_write: Vec<&String> = match &node_output.dispatch {
                            NodeDispatch::Broadcast => data_output_pin_ids
                                .iter()
                                .chain(reactive_output_pin_ids.iter())
                                .collect(),
                            NodeDispatch::Route(ports) => ports
                                .iter()
                                .filter(|p| {
                                    data_output_pin_ids.contains(*p)
                                        || reactive_output_pin_ids.contains(*p)
                                })
                                .collect(),
                        };
                        for pin_id in cache_pins_to_write {
                            let _ = output_cache.write_now(
                                pin_id,
                                data_cache_value_for_pin(pin_id, &node_output.payload),
                                trace_id,
                            );
                        }
                    }

                    // Exec + Reactive 路径：仅排除纯 Data 输出 pin 的下游 sender。
                    // Reactive 源 pin 不在 data_output_pin_ids 中，自然通过过滤器——
                    // 获得 ContextRef 推送（ADR-0015 Phase 1 三分支语义）。
                    let matching_targets = match &node_output.dispatch {
                        NodeDispatch::Broadcast => downstream_senders
                            .iter()
                            .filter(|target| {
                                target
                                    .source_port_id
                                    .as_ref()
                                    .is_none_or(|port| !data_output_pin_ids.contains(port))
                            })
                            .collect::<Vec<_>>(),
                        NodeDispatch::Route(port_ids) => downstream_senders
                            .iter()
                            .filter(|target| {
                                target.source_port_id.as_ref().is_some_and(|port_id| {
                                    !data_output_pin_ids.contains(port_id)
                                        && port_ids.iter().any(|candidate| candidate == port_id)
                                })
                            })
                            .collect::<Vec<_>>(),
                    };

                    for (key, value) in node_output.metadata {
                        merged_metadata.insert(key, value);
                    }

                    let consumer_count = if matching_targets.is_empty() {
                        1
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

        // ADR-0014 Phase 5：trace 完成后清理 PureMemo（释放内存）
        pure_memo.clear_trace(trace_id);
    }
}

fn data_cache_value_for_pin(pin_id: &str, payload: &serde_json::Value) -> serde_json::Value {
    if let serde_json::Value::Object(map) = payload
        && let Some(value) = map.get(pin_id)
    {
        return value.clone();
    }
    payload.clone()
}
