//! DAG 校验与拓扑排序。
//!
//! 使用 Kahn 算法计算拓扑序，同时检测环和无效边引用。
//! 入度为零的节点被识别为根节点，用作工作流数据入口。

use std::collections::{HashMap, VecDeque};

use serde_json::Value;

use super::types::{WorkflowGraph, WorkflowTopology};
use crate::EngineError;

impl WorkflowGraph {
    /// 将 JSON AST 字符串解析为经过校验的 `WorkflowGraph`。
    ///
    /// # Errors
    ///
    /// JSON 解析失败、DAG 校验失败或无根节点时返回错误。
    pub fn from_json(ast: &str) -> Result<Self, EngineError> {
        let mut graph: WorkflowGraph = serde_json::from_str(ast)
            .map_err(|error| EngineError::graph_deserialization(error.to_string()))?;

        for (node_id, node_definition) in &mut graph.nodes {
            if node_definition.id.is_empty() {
                node_definition.id.clone_from(node_id);
            }

            if node_definition.connection_id.is_none() {
                node_definition.connection_id = node_definition
                    .config
                    .get("connection_id")
                    .and_then(Value::as_str)
                    .map(ToOwned::to_owned);
            }
        }

        graph.validate()?;
        Ok(graph)
    }

    /// 校验图为合法 DAG 且至少包含一个根节点。
    ///
    /// # Errors
    ///
    /// 图包含环或无根节点时返回 [`EngineError::InvalidGraph`]。
    pub fn validate(&self) -> Result<(), EngineError> {
        let topology = self.topology()?;
        if topology.root_nodes.is_empty() {
            return Err(EngineError::invalid_graph("工作流图必须包含至少一个根节点"));
        }
        Ok(())
    }

    /// 计算拓扑序（Kahn 算法）并检测环。
    pub(crate) fn topology(&self) -> Result<WorkflowTopology, EngineError> {
        let mut incoming: HashMap<String, usize> = self
            .nodes
            .keys()
            .map(|node_id| (node_id.clone(), 0_usize))
            .collect();
        let mut downstream: HashMap<String, Vec<_>> = self
            .nodes
            .keys()
            .map(|node_id| (node_id.clone(), Vec::new()))
            .collect();

        for edge in &self.edges {
            if !self.nodes.contains_key(&edge.from) {
                return Err(EngineError::invalid_graph(format!(
                    "边的源节点 `{}` 不存在",
                    edge.from
                )));
            }
            if !self.nodes.contains_key(&edge.to) {
                return Err(EngineError::invalid_graph(format!(
                    "边的目标节点 `{}` 不存在",
                    edge.to
                )));
            }
            downstream
                .entry(edge.from.clone())
                .or_default()
                .push(edge.clone());
            if let Some(count) = incoming.get_mut(&edge.to) {
                *count += 1;
            }
        }

        let root_nodes = incoming
            .iter()
            .filter(|(_, count)| **count == 0)
            .map(|(node_id, _)| node_id.clone())
            .collect::<Vec<_>>();

        let mut queue = VecDeque::from(root_nodes.clone());
        let mut processed = 0_usize;

        while let Some(node_id) = queue.pop_front() {
            processed += 1;
            if let Some(neighbors) = downstream.get(&node_id) {
                for neighbor in neighbors {
                    if let Some(count) = incoming.get_mut(&neighbor.to) {
                        *count -= 1;
                        if *count == 0 {
                            queue.push_back(neighbor.to.clone());
                        }
                    }
                }
            }
        }

        if processed != self.nodes.len() {
            return Err(EngineError::invalid_graph(
                "工作流图必须是无环的有向图（DAG）",
            ));
        }

        Ok(WorkflowTopology {
            root_nodes,
            downstream,
        })
    }
}
