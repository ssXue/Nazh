//! DAG 校验与拓扑排序。
//!
//! 使用 Kahn 算法计算拓扑序，同时检测环和无效边引用。
//! 入度为零的节点被识别为根节点，用作工作流数据入口。

use std::collections::{HashMap, VecDeque};
use std::sync::Arc;

use nazh_core::{NodeTrait, PinKind};
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

/// 在 Data 边构成的子图上做环检测（ADR-0014 Phase 1）。
///
/// Data 边不参与主拓扑（避免 Data 拉取关系污染 Exec 触发顺序），但**仍可能
/// 形成依赖环**——A 的 Data 输出依赖 B 的最新值、B 的 Data 输出又依赖 A 的最新值。
/// 此种环让 Phase 2/3 的"下游 transform 前拉上游缓存"陷入无定义循环依赖。
///
/// 算法：在 Data 边构成的图上跑 Kahn——若不能消化所有节点，存在环。
///
/// # Errors
///
/// Data 边构成环时返回 [`EngineError::InvalidGraph`].
pub(crate) fn detect_data_edge_cycle(
    data_edges: &[&super::types::WorkflowEdge],
) -> Result<(), crate::EngineError> {
    if data_edges.is_empty() {
        return Ok(());
    }

    // 构造 Data 子图：仅含 data_edges 涉及的节点
    let mut incoming: HashMap<String, usize> = HashMap::new();
    let mut downstream: HashMap<String, Vec<String>> = HashMap::new();
    for edge in data_edges {
        incoming.entry(edge.from.clone()).or_insert(0);
        *incoming.entry(edge.to.clone()).or_insert(0) += 1;
        downstream
            .entry(edge.from.clone())
            .or_default()
            .push(edge.to.clone());
    }

    let total_nodes = incoming.len();
    let mut queue: VecDeque<String> = incoming
        .iter()
        .filter(|(_, count)| **count == 0)
        .map(|(id, _)| id.clone())
        .collect();
    let mut consumed = 0_usize;

    while let Some(node_id) = queue.pop_front() {
        consumed += 1;
        if let Some(neighbors) = downstream.get(&node_id) {
            for neighbor in neighbors {
                if let Some(count) = incoming.get_mut(neighbor) {
                    *count -= 1;
                    if *count == 0 {
                        queue.push_back(neighbor.clone());
                    }
                }
            }
        }
    }

    if consumed != total_nodes {
        return Err(crate::EngineError::invalid_graph(
            "Data 边构成环（ADR-0014）：下游 transform 时无法确定缓存读取顺序",
        ));
    }
    Ok(())
}

/// 边按 [`PinKind`] 分类的结果（ADR-0014 Phase 1）。
///
/// `'a` 借用 `WorkflowEdge` 列表本身的生命周期——分类只重组引用，不克隆。
#[derive(Debug)]
pub(crate) struct ClassifiedEdges<'a> {
    /// Exec 边——Phase 2 起由 Runner 用于确认 Exec push 范围；Phase 1 暂未读取。
    #[allow(dead_code)]
    pub exec_edges: Vec<&'a super::types::WorkflowEdge>,
    pub data_edges: Vec<&'a super::types::WorkflowEdge>,
}

const DEFAULT_OUTPUT_PIN_ID: &str = "out";

/// 按上游节点 source pin 的 [`PinKind`] 把边分类为 exec / data。
///
/// 参数 `nodes` 必须包含图中所有节点（阶段 0.5 实例化后）。
///
/// # Errors
///
/// 边引用的源节点不存在、或源节点 `output_pins` 中找不到对应 pin id 时返回
/// [`EngineError::UnknownPin`]——这种 case 也应在 `pin_validator` 提前发现，
/// 但本函数自包含校验避免依赖前置阶段，便于单测。
pub(crate) fn classify_edges<'a>(
    edges: &'a [super::types::WorkflowEdge],
    nodes: &HashMap<String, Arc<dyn NodeTrait>>,
) -> Result<ClassifiedEdges<'a>, crate::EngineError> {
    let mut exec_edges = Vec::new();
    let mut data_edges = Vec::new();

    for edge in edges {
        let from_node = nodes.get(&edge.from).ok_or_else(|| {
            crate::EngineError::invalid_graph(format!(
                "classify_edges：边的源节点 `{}` 不存在",
                edge.from
            ))
        })?;
        let from_pin_id = edge
            .source_port_id
            .as_deref()
            .unwrap_or(DEFAULT_OUTPUT_PIN_ID);
        let from_pin = from_node
            .output_pins()
            .into_iter()
            .find(|p| p.id == from_pin_id)
            .ok_or_else(|| crate::EngineError::UnknownPin {
                node: edge.from.clone(),
                pin: from_pin_id.to_owned(),
                direction: nazh_core::PinDirection::Output,
            })?;

        match from_pin.kind {
            PinKind::Exec => exec_edges.push(edge),
            PinKind::Data => data_edges.push(edge),
        }
    }

    Ok(ClassifiedEdges {
        exec_edges,
        data_edges,
    })
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use crate::graph::types::WorkflowEdge;
    use async_trait::async_trait;
    use nazh_core::{NodeExecution, NodeTrait, PinDefinition, PinDirection, PinKind, PinType};
    use serde_json::Value;
    use std::sync::Arc;
    use uuid::Uuid;

    /// 测试 stub 节点：通过构造函数注入 input / output pin 列表。
    struct StubNode {
        id: String,
        inputs: Vec<PinDefinition>,
        outputs: Vec<PinDefinition>,
    }

    #[async_trait]
    impl NodeTrait for StubNode {
        fn id(&self) -> &str {
            &self.id
        }
        fn kind(&self) -> &'static str {
            "stub"
        }
        fn input_pins(&self) -> Vec<PinDefinition> {
            self.inputs.clone()
        }
        fn output_pins(&self) -> Vec<PinDefinition> {
            self.outputs.clone()
        }
        async fn transform(
            &self,
            _trace_id: Uuid,
            _payload: Value,
        ) -> Result<NodeExecution, nazh_core::EngineError> {
            Ok(NodeExecution::broadcast(Value::Null))
        }
    }

    fn pin(id: &str, dir: PinDirection, kind: PinKind) -> PinDefinition {
        PinDefinition {
            id: id.to_owned(),
            label: id.to_owned(),
            pin_type: PinType::Any,
            direction: dir,
            required: false,
            kind,
            description: None,
        }
    }

    fn make_node(
        id: &str,
        inputs: Vec<PinDefinition>,
        outputs: Vec<PinDefinition>,
    ) -> Arc<dyn NodeTrait> {
        Arc::new(StubNode {
            id: id.to_owned(),
            inputs,
            outputs,
        })
    }

    fn edge(from: &str, to: &str, source_port: Option<&str>) -> WorkflowEdge {
        WorkflowEdge {
            from: from.to_owned(),
            to: to.to_owned(),
            source_port_id: source_port.map(str::to_owned),
            target_port_id: None,
        }
    }

    #[test]
    fn classify_edges_把_data_pin_出边归为_data() {
        let mut nodes: HashMap<String, Arc<dyn NodeTrait>> = HashMap::new();
        nodes.insert(
            "a".to_owned(),
            make_node(
                "a",
                vec![pin("in", PinDirection::Input, PinKind::Exec)],
                vec![pin("latest", PinDirection::Output, PinKind::Data)],
            ),
        );
        nodes.insert(
            "b".to_owned(),
            make_node(
                "b",
                vec![pin("in", PinDirection::Input, PinKind::Data)],
                vec![PinDefinition::default_output()],
            ),
        );

        let edges = vec![edge("a", "b", Some("latest"))];
        let classified = classify_edges(&edges, &nodes).unwrap();
        assert_eq!(classified.exec_edges.len(), 0);
        assert_eq!(classified.data_edges.len(), 1);
        assert_eq!(classified.data_edges[0].from, "a");
    }

    #[test]
    fn classify_edges_把_exec_pin_出边归为_exec() {
        let mut nodes: HashMap<String, Arc<dyn NodeTrait>> = HashMap::new();
        nodes.insert(
            "a".to_owned(),
            make_node(
                "a",
                vec![PinDefinition::default_input()],
                vec![PinDefinition::default_output()],
            ),
        );
        nodes.insert(
            "b".to_owned(),
            make_node(
                "b",
                vec![PinDefinition::default_input()],
                vec![PinDefinition::default_output()],
            ),
        );

        let edges = vec![edge("a", "b", None)];
        let classified = classify_edges(&edges, &nodes).unwrap();
        assert_eq!(classified.exec_edges.len(), 1);
        assert_eq!(classified.data_edges.len(), 0);
    }

    #[test]
    fn classify_edges_未知_source_port_报错() {
        let mut nodes: HashMap<String, Arc<dyn NodeTrait>> = HashMap::new();
        nodes.insert(
            "a".to_owned(),
            make_node(
                "a",
                vec![PinDefinition::default_input()],
                vec![PinDefinition::default_output()],
            ),
        );
        nodes.insert(
            "b".to_owned(),
            make_node(
                "b",
                vec![PinDefinition::default_input()],
                vec![PinDefinition::default_output()],
            ),
        );

        let edges = vec![edge("a", "b", Some("ghost"))];
        let err = classify_edges(&edges, &nodes).unwrap_err();
        assert!(matches!(err, nazh_core::EngineError::UnknownPin { .. }));
    }

    #[test]
    fn detect_data_edge_cycle_无_data_边时通过() {
        let mut nodes: HashMap<String, Arc<dyn NodeTrait>> = HashMap::new();
        nodes.insert(
            "a".to_owned(),
            make_node(
                "a",
                vec![PinDefinition::default_input()],
                vec![PinDefinition::default_output()],
            ),
        );
        nodes.insert(
            "b".to_owned(),
            make_node(
                "b",
                vec![PinDefinition::default_input()],
                vec![PinDefinition::default_output()],
            ),
        );
        let edges = vec![edge("a", "b", None)];
        let classified = classify_edges(&edges, &nodes).unwrap();
        detect_data_edge_cycle(&classified.data_edges).unwrap();
    }

    #[test]
    fn detect_data_edge_cycle_data_边形成环时报错() {
        // a 的 Data 输出 → b 的 Data 输入；b 的 Data 输出 → a 的 Data 输入
        // 构成 Data 边的环
        let mut nodes: HashMap<String, Arc<dyn NodeTrait>> = HashMap::new();
        nodes.insert(
            "a".to_owned(),
            make_node(
                "a",
                vec![pin("in", PinDirection::Input, PinKind::Data)],
                vec![pin("out", PinDirection::Output, PinKind::Data)],
            ),
        );
        nodes.insert(
            "b".to_owned(),
            make_node(
                "b",
                vec![pin("in", PinDirection::Input, PinKind::Data)],
                vec![pin("out", PinDirection::Output, PinKind::Data)],
            ),
        );
        let edges = vec![edge("a", "b", Some("out")), edge("b", "a", Some("out"))];
        let classified = classify_edges(&edges, &nodes).unwrap();
        let err = detect_data_edge_cycle(&classified.data_edges).unwrap_err();
        assert!(matches!(err, crate::EngineError::InvalidGraph(_)));
    }

    #[test]
    fn detect_data_edge_cycle_data_边自环报错() {
        let mut nodes: HashMap<String, Arc<dyn NodeTrait>> = HashMap::new();
        nodes.insert(
            "a".to_owned(),
            make_node(
                "a",
                vec![pin("in", PinDirection::Input, PinKind::Data)],
                vec![pin("out", PinDirection::Output, PinKind::Data)],
            ),
        );
        let edges = vec![edge("a", "a", Some("out"))];
        let classified = classify_edges(&edges, &nodes).unwrap();
        let err = detect_data_edge_cycle(&classified.data_edges).unwrap_err();
        assert!(matches!(err, crate::EngineError::InvalidGraph(_)));
    }
}
