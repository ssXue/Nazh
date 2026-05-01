//! Data 入边反向索引构建。

use std::collections::HashMap;

use crate::DEFAULT_OUTPUT_PIN_ID;
use crate::types::WorkflowEdge;

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

/// 在 [`classify_edges`](super::super::topology::classify_edges) 已分出的 `data_edges`
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

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use crate::types::WorkflowEdge;

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
