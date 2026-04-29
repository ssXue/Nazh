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

use std::{collections::HashMap, collections::HashSet, sync::Arc, time::Duration, time::Instant};

use tokio::sync::mpsc;
use tracing::Instrument;

use nazh_core::{
    ContextRef, DataStore, EdgeTransmitSummary, EngineError, ExecutionEvent, NodeDispatch,
    NodeTrait, OutputCache, PinKind,
    event::{emit_event, emit_failure},
    guard::guarded_execute,
};

use super::pull::{EdgesByConsumer, PureMemo};
use super::types::DownstreamTarget;

/// ADR-0016：单条边的传输累计窗口。
///
/// 每次 `record()` 累加一次传输统计；窗口满（≥100ms）时 `flush()` 发出
/// [`EdgeTransmitSummary`] 事件并重置计数。
struct EdgeWindow {
    from_pin: String,
    to_node: String,
    to_pin: String,
    edge_kind: PinKind,
    transmit_count: usize,
    max_queue_depth: usize,
    window_start: Instant,
}

impl EdgeWindow {
    fn new(from_pin: String, to_node: String, to_pin: String, edge_kind: PinKind) -> Self {
        Self {
            from_pin,
            to_node,
            to_pin,
            edge_kind,
            transmit_count: 0,
            max_queue_depth: 0,
            window_start: Instant::now(),
        }
    }

    fn record(&mut self, queue_depth: usize) {
        self.transmit_count += 1;
        self.max_queue_depth = self.max_queue_depth.max(queue_depth);
    }

    /// 若窗口已满（≥100ms）或有数据待发，构造并发出 [`EdgeTransmitSummary`]，
    /// 然后重置计数。无数据时不发事件。
    fn flush(&mut self, from_node: &str, event_tx: &mpsc::Sender<ExecutionEvent>) {
        if self.transmit_count == 0 {
            return;
        }
        let now = Instant::now();
        emit_event(
            event_tx,
            ExecutionEvent::EdgeTransmitSummary(EdgeTransmitSummary {
                from_node: from_node.to_owned(),
                from_pin: self.from_pin.clone(),
                to_node: self.to_node.clone(),
                to_pin: self.to_pin.clone(),
                edge_kind: self.edge_kind,
                transmit_count: self.transmit_count,
                max_queue_depth: self.max_queue_depth,
                window_started_at: format_instant(self.window_start),
                window_ended_at: format_instant(now),
            }),
        );
        self.transmit_count = 0;
        self.max_queue_depth = 0;
        self.window_start = now;
    }
}

/// 将 [`Instant`] 格式化为 RFC3339 字符串。
///
/// [`Instant`] 是单调时钟，无绝对时间语义；此处以"进程启动后偏移"近似。
/// 未来若需精确绝对时间，可传入外部 `now: DateTime<Utc>`。
fn format_instant(instant: Instant) -> String {
    let offset = instant.elapsed();
    // 近似：以当前系统时间减去偏移量作为该 instant 的绝对时间。
    let now = chrono::Utc::now();
    let absolute = now - chrono::Duration::from_std(offset).unwrap_or_default();
    absolute.to_rfc3339()
}

/// 边窗口 key：`(from_pin, to_node, to_pin)`。
type EdgeKey = (String, String, String);

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
            );
            (key, window)
        })
        .collect();

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
                            // ADR-0016：记录边传输统计。
                            let from_pin = target.source_port_id.as_deref().unwrap_or("out");
                            let to_pin = target.target_port_id.as_deref().unwrap_or("in");
                            let key = (
                                from_pin.to_owned(),
                                target.target_node_id.clone(),
                                to_pin.to_owned(),
                            );
                            if let Some(window) = edge_windows.get_mut(&key) {
                                // TODO(ADR-0016)：queue_depth 目前记录 0。
                                // 精确测量需要 instrument 接收端（Receiver::len()），
                                // 改为共享 channel 状态或由下游反馈。
                                window.record(0);
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

        // ADR-0016：刷新有数据的边传输窗口。
        // 当前策略：每个执行周期末尾 flush 所有有数据的窗口。
        // 未来可在高频场景下改回 100ms 窗口定时 flush。
        for window in edge_windows.values_mut() {
            window.flush(&node_id, &event_tx);
        }
    }

    // ADR-0016：循环退出时 flush 剩余窗口（理论上为空，保底）。
    for window in edge_windows.values_mut() {
        window.flush(&node_id, &event_tx);
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
