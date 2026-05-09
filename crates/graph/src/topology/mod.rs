//! DAG 校验与拓扑排序。
//!
//! 使用 Kahn 算法计算拓扑序，同时检测环和无效边引用。
//! 入度为零的节点被识别为根节点，用作工作流数据入口。
//!
//! 部署期主拓扑只应使用会推送 `ContextRef` 的 Exec / Reactive 边；Data 边
//! 只参与 pull path 索引与独立环检测，避免缓存依赖污染触发入口。

mod classify;

pub(crate) use classify::{classify_edges, detect_non_exec_edge_cycle};

use std::collections::{HashMap, VecDeque};

use nazh_core::EngineError;
use serde_json::Value;

use crate::types::{WorkflowGraph, WorkflowTopology};

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
            // 只有当节点还没有 connection_id 时才去 config 里找 fallback；避免无谓的 String 分配
            let fallback_connection_id: Option<String> =
                if node_definition.connection_id().is_none() {
                    node_definition
                        .config()
                        .get("connection_id")
                        .and_then(Value::as_str)
                        .map(ToOwned::to_owned)
                } else {
                    None
                };
            node_definition.normalize(node_id, fallback_connection_id.as_deref());
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

    /// 计算全边拓扑序（Kahn 算法）并检测环。
    pub(crate) fn topology(&self) -> Result<WorkflowTopology, EngineError> {
        self.topology_for_edges(self.edges.iter())
    }

    /// 在调用方给定的边集合上计算拓扑序。
    ///
    /// 部署阶段会传入 Exec + Reactive 边集合，让 Data 边不影响 root 识别与
    /// 任务部署顺序；`from_json` 的结构校验仍可传入全边集合做保守环检查。
    pub(crate) fn topology_for_edges<'a, I>(
        &self,
        edges: I,
    ) -> Result<WorkflowTopology, EngineError>
    where
        I: IntoIterator<Item = &'a crate::types::WorkflowEdge>,
    {
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

        for edge in edges {
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

        let mut root_nodes = incoming
            .iter()
            .filter(|(_, count)| **count == 0)
            .map(|(node_id, _)| node_id.clone())
            .collect::<Vec<_>>();
        // root_nodes 来自 HashMap，顺序非确定性；排序后让上层（部署顺序、
        // E2E 测试断言）稳定，避免随构建偶发漂移。
        root_nodes.sort();

        let mut queue = VecDeque::from(root_nodes.clone());
        let mut deployment_order = Vec::with_capacity(self.nodes.len());

        while let Some(node_id) = queue.pop_front() {
            deployment_order.push(node_id.clone());
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

        if deployment_order.len() != self.nodes.len() {
            return Err(EngineError::invalid_graph(
                "工作流图必须是无环的有向图（DAG）",
            ));
        }

        Ok(WorkflowTopology {
            root_nodes,
            downstream,
            deployment_order,
        })
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn 简单_dag_拓扑序正确() {
        let json = serde_json::json!({
            "nodes": {
                "a": { "type": "timer" },
                "b": { "type": "debugConsole" },
                "c": { "type": "debugConsole" }
            },
            "edges": [
                { "from": "a", "to": "b" },
                { "from": "b", "to": "c" }
            ]
        });
        let graph = WorkflowGraph::from_json(&serde_json::to_string(&json).unwrap()).unwrap();
        let topo = graph.topology().unwrap();
        assert_eq!(topo.root_nodes, vec!["a"]);
        assert_eq!(topo.deployment_order, vec!["a", "b", "c"]);
    }

    #[test]
    fn 有环报错() {
        let json = serde_json::json!({
            "nodes": {
                "a": { "type": "timer" },
                "b": { "type": "debugConsole" }
            },
            "edges": [
                { "from": "a", "to": "b" },
                { "from": "b", "to": "a" }
            ]
        });
        let result = WorkflowGraph::from_json(&serde_json::to_string(&json).unwrap());
        assert!(result.is_err());
    }

    #[test]
    fn 无根节点报错() {
        let json = serde_json::json!({
            "nodes": {
                "a": { "type": "timer" },
                "b": { "type": "timer" }
            },
            "edges": [
                { "from": "a", "to": "b" },
                { "from": "b", "to": "a" }
            ]
        });
        let result = WorkflowGraph::from_json(&serde_json::to_string(&json).unwrap());
        assert!(result.is_err());
    }

    #[test]
    fn 主拓扑可忽略_data_边对_root_的影响() {
        let graph: WorkflowGraph = serde_json::from_value(serde_json::json!({
            "nodes": {
                "a": { "type": "source" },
                "b": { "type": "sink" },
                "c": { "type": "dataSource" }
            },
            "edges": [
                { "from": "a", "to": "b", "source_port_id": "out", "target_port_id": "in" },
                { "from": "c", "to": "a", "source_port_id": "latest", "target_port_id": "sensor" }
            ]
        }))
        .unwrap();

        let full_topology = graph.topology().unwrap();
        assert_eq!(full_topology.root_nodes, vec!["c"]);

        let exec_topology = graph
            .topology_for_edges(graph.edges.iter().take(1))
            .unwrap();
        assert_eq!(exec_topology.root_nodes, vec!["a", "c"]);
        assert!(
            exec_topology
                .deployment_order
                .iter()
                .position(|node_id| node_id == "a")
                < exec_topology
                    .deployment_order
                    .iter()
                    .position(|node_id| node_id == "b")
        );
    }
}
