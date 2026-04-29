//! ADR-0014 Phase 3：Data 输入引脚的运行时拉路径。
//!
//! 当一个被 Exec 边触发的下游节点在 [`NodeTrait::input_pins`] 中声明了
//! [`PinKind::Data`](nazh_core::PinKind::Data) 引脚，本模块负责在 Runner 调用
//! `transform` **之前**：
//! 1. 反查每个 Data 输入引脚对应的上游边（[`EdgesByConsumer`]）
//! 2. 上游若为 pure-form 节点 → 递归求值
//! 3. 上游若为 Exec 节点（如 `modbusRead.latest`）→ 读取其 [`OutputCache`]
//! 4. 把收集到的 Data 值合并进 `transform` payload

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use nazh_core::{EngineError, NodeTrait, OutputCache, Uuid, guard::guarded_execute, is_pure_form};
use serde_json::{Map, Value};
use tracing::Instrument;

use super::DEFAULT_OUTPUT_PIN_ID;
use super::types::WorkflowEdge;

/// 反向索引：每个 consumer node id → 其所有 Data 入边的元组列表。
///
/// 元组结构：`(consumer_input_pin_id, upstream_node_id, upstream_output_pin_id)`。
/// `consumer_input_pin_id` 用 `target_port_id` 解析，缺省时为 `"in"`；
/// `upstream_output_pin_id` 用 `source_port_id` 解析，缺省时为 [`DEFAULT_OUTPUT_PIN_ID`]。
#[derive(Debug, Default, Clone)]
pub(crate) struct EdgesByConsumer {
    by_consumer: HashMap<String, Vec<DataInEdge>>,
}

#[derive(Debug, Clone)]
#[allow(clippy::struct_field_names)]
pub(crate) struct DataInEdge {
    pub consumer_input_pin_id: String,
    pub upstream_node_id: String,
    pub upstream_output_pin_id: String,
}

impl EdgesByConsumer {
    pub fn for_consumer(&self, consumer_node_id: &str) -> &[DataInEdge] {
        self.by_consumer
            .get(consumer_node_id)
            .map_or(&[], Vec::as_slice)
    }
}

/// 在 [`classify_edges`](super::topology::classify_edges) 已分出的 `data_edges`
/// 上构造反向索引。
pub(crate) fn build_edges_by_consumer(data_edges: &[&WorkflowEdge]) -> EdgesByConsumer {
    let mut by_consumer: HashMap<String, Vec<DataInEdge>> = HashMap::new();
    for edge in data_edges {
        let entry = DataInEdge {
            consumer_input_pin_id: edge
                .target_port_id
                .clone()
                .unwrap_or_else(|| "in".to_owned()),
            upstream_node_id: edge.from.clone(),
            upstream_output_pin_id: edge
                .source_port_id
                .clone()
                .unwrap_or_else(|| DEFAULT_OUTPUT_PIN_ID.to_owned()),
        };
        by_consumer.entry(edge.to.clone()).or_default().push(entry);
    }
    EdgesByConsumer { by_consumer }
}

/// 在被 Exec 触发的下游节点 transform 之前，收集其 Data 输入引脚的最新值，
/// 并把它们合并进 transform payload。
///
/// 合并规则（Phase 3 约定，混合输入节点见 Phase 3b 决策）：
/// - 若 `exec_payload` 为 `Object`，把每个 Data pin 的值以 `pin.id` 为键插入
/// - 否则（标量、数组）payload 重写为 `{"in": exec_payload, <pin_id>: value, ...}`
///
/// 上游若为 pure-form 节点 → 调 [`pull_one`] 递归求值。
/// 上游若为 Exec 节点 → 读其 [`OutputCache`] 槽。
pub(crate) async fn pull_data_inputs(
    consumer_node_id: &str,
    exec_payload: Value,
    edges_by_consumer: &EdgesByConsumer,
    nodes_index: &HashMap<String, Arc<dyn NodeTrait>>,
    output_caches_index: &HashMap<String, Arc<OutputCache>>,
    node_timeouts_index: &HashMap<String, Option<Duration>>,
    trace_id: Uuid,
) -> Result<Value, EngineError> {
    let entries = edges_by_consumer.for_consumer(consumer_node_id);
    if entries.is_empty() {
        return Ok(exec_payload);
    }

    let mut data_values: Map<String, Value> = Map::new();
    for entry in entries {
        let upstream_value = pull_one(
            &entry.upstream_node_id,
            &entry.upstream_output_pin_id,
            nodes_index,
            output_caches_index,
            node_timeouts_index,
            edges_by_consumer,
            trace_id,
        )
        .await?;
        data_values.insert(entry.consumer_input_pin_id.clone(), upstream_value);
    }

    Ok(merge_payload(exec_payload, data_values))
}

pub fn merge_payload(exec_payload: Value, data_values: Map<String, Value>) -> Value {
    match exec_payload {
        Value::Object(mut map) => {
            for (k, v) in data_values {
                map.insert(k, v);
            }
            Value::Object(map)
        }
        other => {
            let mut map = data_values;
            map.insert("in".to_owned(), other);
            Value::Object(map)
        }
    }
}

/// 从单个上游 (`node_id`, `pin_id`) 拉取一份 Data 值。
fn pull_one<'a>(
    upstream_node_id: &'a str,
    upstream_output_pin_id: &'a str,
    nodes_index: &'a HashMap<String, Arc<dyn NodeTrait>>,
    output_caches_index: &'a HashMap<String, Arc<OutputCache>>,
    node_timeouts_index: &'a HashMap<String, Option<Duration>>,
    edges_by_consumer: &'a EdgesByConsumer,
    trace_id: Uuid,
) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<Value, EngineError>> + Send + 'a>> {
    Box::pin(async move {
        let upstream = nodes_index.get(upstream_node_id).ok_or_else(|| {
            EngineError::invalid_graph(format!(
                "拉路径上游节点 `{upstream_node_id}` 在 nodes_index 缺失"
            ))
        })?;

        if is_pure_form(upstream.as_ref()) {
            // 递归：先收集 pure 上游自己的 Data 输入，再调用其 transform
            let upstream_payload = pull_data_inputs(
                upstream_node_id,
                Value::Object(Map::new()),
                edges_by_consumer,
                nodes_index,
                output_caches_index,
                node_timeouts_index,
                trace_id,
            )
            .await?;
            let span = tracing::info_span!(
                "node.transform",
                node_id = %upstream_node_id,
                trace_id = %trace_id,
                pull = true,
            );
            let timeout = node_timeouts_index.get(upstream_node_id).copied().flatten();
            let result = guarded_execute(
                upstream_node_id,
                trace_id,
                timeout,
                upstream.transform(trace_id, upstream_payload),
            )
            .instrument(span)
            .await?;
            // 找匹配 upstream_output_pin_id 的输出 payload
            // pure 节点 transform payload 约定为 `{ <pin_id>: value, ... }`
            for output in &result.outputs {
                if let Value::Object(map) = &output.payload
                    && let Some(v) = map.get(upstream_output_pin_id)
                {
                    return Ok(v.clone());
                }
            }
            // 兜底：若 pure 节点只有单输出且 payload 不是 `{pin_id: value}` 形态
            result.outputs.first().map(|o| o.payload.clone()).ok_or(
                EngineError::DataPinCacheEmpty {
                    upstream: upstream_node_id.to_owned(),
                    pin: upstream_output_pin_id.to_owned(),
                },
            )
        } else {
            // 非 pure：读 OutputCache
            let cache = output_caches_index.get(upstream_node_id).ok_or_else(|| {
                EngineError::invalid_graph(format!(
                    "上游 Exec 节点 `{upstream_node_id}` 在 output_caches_index 缺失"
                ))
            })?;
            let cached =
                cache
                    .read(upstream_output_pin_id)
                    .ok_or(EngineError::DataPinCacheEmpty {
                        upstream: upstream_node_id.to_owned(),
                        pin: upstream_output_pin_id.to_owned(),
                    })?;
            Ok(cached.value)
        }
    })
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use crate::graph::types::WorkflowEdge;

    fn data_edge(from: &str, sport: Option<&str>, to: &str, tport: Option<&str>) -> WorkflowEdge {
        WorkflowEdge {
            from: from.to_owned(),
            to: to.to_owned(),
            source_port_id: sport.map(ToOwned::to_owned),
            target_port_id: tport.map(ToOwned::to_owned),
        }
    }

    #[test]
    fn 单_data_边构造单_entry() {
        let e = data_edge("up", Some("latest"), "down", Some("temp"));
        let refs = vec![&e];
        let idx = build_edges_by_consumer(&refs);
        let entries = idx.for_consumer("down");
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].consumer_input_pin_id, "temp");
        assert_eq!(entries[0].upstream_node_id, "up");
        assert_eq!(entries[0].upstream_output_pin_id, "latest");
    }

    #[test]
    fn 多个_data_边按_consumer_分组() {
        let e1 = data_edge("up1", Some("o1"), "down", Some("a"));
        let e2 = data_edge("up2", Some("o2"), "down", Some("b"));
        let refs = vec![&e1, &e2];
        let idx = build_edges_by_consumer(&refs);
        assert_eq!(idx.for_consumer("down").len(), 2);
        assert!(idx.for_consumer("missing").is_empty());
    }

    #[test]
    fn 缺端口_id_默认到_in_和_out() {
        let e = data_edge("up", None, "down", None);
        let refs = vec![&e];
        let idx = build_edges_by_consumer(&refs);
        let entries = idx.for_consumer("down");
        assert_eq!(entries[0].consumer_input_pin_id, "in");
        assert_eq!(entries[0].upstream_output_pin_id, "out");
    }
}
