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
    PinKind,
    event::{emit_event, emit_failure},
    guard::guarded_execute,
};

use super::edge_window::{EdgeKey, EdgeWindow, EDGE_WINDOW_DURATION};
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
    let has_non_data_output_pin = node
        .output_pins()
        .iter()
        .any(|pin| pin.kind != PinKind::Data);

    // ADR-0016：初始化边传输累计窗口。
    let mut edge_windows: HashMap<EdgeKey, EdgeWindow> = downstream_senders
        .iter()
        .map(|target| {
            let from_pin = target
                .source_port_id
                .clone()
                .unwrap_or_else(|| "out".to_owned());
            let to_pin = target
                .target_port_id
                .clone()
                .unwrap_or_else(|| "in".to_owned());
            let key = (
                from_pin.clone(),
                target.target_node_id.clone(),
                to_pin.clone(),
            );
            let window = EdgeWindow::new(
                from_pin,
                target.target_node_id.clone(),
                to_pin,
                target.edge_kind,
                target.sender.max_capacity(),
            );
            (key, window)
        })
        .collect();

    let mut edge_flush_interval = tokio::time::interval(EDGE_WINDOW_DURATION);
    edge_flush_interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);

    loop {
        let ctx_ref = tokio::select! {
            maybe_ctx_ref = input_rx.recv() => {
                let Some(ctx_ref) = maybe_ctx_ref else {
                    break;
                };
                ctx_ref
            }
            _ = edge_flush_interval.tick() => {
                for window in edge_windows.values_mut() {
                    window.flush_if_ready(&node_id, &event_tx);
                }
                continue;
            }
        };
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
                // 合并元数据：`None` = 无节点产出元数据；有节点返回 `Some(_)` 时升级为 `Some(Map)`。
                // 约定（B1-R0-02）：`None` 不升级为 `Some(empty)`，`Some(empty)` 不降级为 `None`。
                let mut merged_metadata: Option<serde_json::Map<String, serde_json::Value>> = None;

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

                    // 合并节点输出元数据到事件级 metadata。
                    // `None` → 跳过；`Some(Map)` → 逐 key 合并。
                    if let Some(meta) = node_output.metadata {
                        let merged = merged_metadata.get_or_insert_with(serde_json::Map::new);
                        for (key, value) in meta {
                            merged.insert(key, value);
                        }
                    }

                    if output_is_data_only(
                        &node_output.dispatch,
                        &matching_targets,
                        &data_output_pin_ids,
                        &reactive_output_pin_ids,
                        has_non_data_output_pin,
                    ) {
                        continue;
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
                        if result_tx.send(new_ref).await.is_err() {
                            store.release(&data_id);
                            Err(EngineError::ChannelClosed {
                                stage: node_id.clone(),
                            })
                        } else {
                            Ok(())
                        }
                    } else {
                        let mut downstream_error = None;
                        let consumer_count = matching_targets.len();
                        for (sent_count, target) in matching_targets.iter().enumerate() {
                            if target.sender.send(new_ref.clone()).await.is_err() {
                                downstream_error = Some(EngineError::ChannelClosed {
                                    stage: node_id.clone(),
                                });
                                for _ in sent_count..consumer_count {
                                    store.release(&data_id);
                                }
                                break;
                            }
                            // ADR-0016：记录边传输统计。
                            let from_pin = target.source_port_id.as_deref().unwrap_or("out");
                            let to_pin = target.target_port_id.as_deref().unwrap_or("in");
                            let key = (
                                from_pin.to_owned(),
                                target.target_node_id.clone(),
                                to_pin.to_owned(),
                            );
                            if let Some(window) = edge_windows.get_mut(&key) {
                                let queue_depth = target
                                    .sender
                                    .max_capacity()
                                    .saturating_sub(target.sender.capacity());
                                window.record(queue_depth, &node_id, &event_tx);
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
                    ExecutionEvent::Completed(nazh_core::CompletedExecutionEvent {
                        stage: node_id.clone(),
                        trace_id,
                        metadata: merged_metadata,
                    }),
                );
            }
            Err(error) => {
                emit_failure(&event_tx, &node_id, trace_id, &error);
            }
        }

        // ADR-0014 Phase 5：trace 完成后清理 PureMemo（释放内存）
        pure_memo.clear_trace(trace_id);

        // ADR-0016：刷新满足 100ms 窗口条件的边传输统计。
        for window in edge_windows.values_mut() {
            window.flush_if_ready(&node_id, &event_tx);
        }
    }

    // ADR-0016：循环退出时无条件 flush 剩余窗口。
    for window in edge_windows.values_mut() {
        window.force_flush(&node_id, &event_tx);
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

fn output_is_data_only(
    dispatch: &NodeDispatch,
    matching_targets: &[&DownstreamTarget],
    data_output_pin_ids: &HashSet<String>,
    reactive_output_pin_ids: &HashSet<String>,
    has_non_data_output_pin: bool,
) -> bool {
    if !matching_targets.is_empty() {
        return false;
    }

    match dispatch {
        NodeDispatch::Broadcast => {
            !data_output_pin_ids.is_empty()
                && reactive_output_pin_ids.is_empty()
                && !has_non_data_output_pin
        }
        NodeDispatch::Route(port_ids) => {
            !port_ids.is_empty()
                && port_ids
                    .iter()
                    .all(|port_id| data_output_pin_ids.contains(port_id))
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
#[path = "runner_tests.rs"]
mod tests;
