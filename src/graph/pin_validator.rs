//! 部署期 Pin 类型兼容校验（ADR-0010 阶段 0.5）。
//!
//! 调用时机：节点已在 `deploy.rs` 阶段 0.5 实例化完成，但 `on_deploy` 尚未调用。
//! 任何副作用（连接借出、订阅、后台任务）都未发生——校验失败直接返回错误，
//! 不需要 RAII 回滚。
//!
//! 校验三类约束：
//! 1. 同节点同方向不可重复 pin id（`DuplicatePinId`）
//! 2. 边引用的 port id 必须在节点 pin 列表中存在；`source_port_id`/`target_port_id`
//!    为 `None` 时回落到默认 `"out"` / `"in"`（`UnknownPin`）
//! 3. 上下游 pin 类型按 [`PinType::is_compatible_with`] 兼容（`IncompatiblePinTypes`）
//!
//! 必需输入引脚（`required: true`）的"每节点必有上游入边"校验同样在此完成。

use std::collections::HashMap;
use std::sync::Arc;

use nazh_core::{EngineError, NodeTrait, PinDefinition, PinDirection};

use super::types::WorkflowEdge;

const DEFAULT_INPUT_PIN_ID: &str = "in";
const DEFAULT_OUTPUT_PIN_ID: &str = "out";

/// 节点 pin 索引：每个节点拆成 input / output 两张 id → `PinDefinition` 的 map。
struct NodePinIndex {
    inputs: HashMap<String, PinDefinition>,
    outputs: HashMap<String, PinDefinition>,
}

fn build_pin_index(node_id: &str, node: &Arc<dyn NodeTrait>) -> Result<NodePinIndex, EngineError> {
    let mut inputs = HashMap::new();
    for pin in node.input_pins() {
        if inputs.contains_key(&pin.id) {
            return Err(EngineError::DuplicatePinId {
                node: node_id.to_owned(),
                pin: pin.id.clone(),
                direction: PinDirection::Input,
            });
        }
        inputs.insert(pin.id.clone(), pin);
    }

    let mut outputs = HashMap::new();
    for pin in node.output_pins() {
        if outputs.contains_key(&pin.id) {
            return Err(EngineError::DuplicatePinId {
                node: node_id.to_owned(),
                pin: pin.id.clone(),
                direction: PinDirection::Output,
            });
        }
        outputs.insert(pin.id.clone(), pin);
    }

    Ok(NodePinIndex { inputs, outputs })
}

/// 对每条边校验两端 pin 类型兼容；对每个节点校验 required 输入有上游入边。
pub(crate) fn validate_pin_compatibility(
    nodes: &HashMap<String, Arc<dyn NodeTrait>>,
    edges: &[WorkflowEdge],
) -> Result<(), EngineError> {
    // 1. 为每个节点建索引（同时检测同方向重复 id）
    let mut indexes: HashMap<&str, NodePinIndex> = HashMap::with_capacity(nodes.len());
    for (id, node) in nodes {
        indexes.insert(id.as_str(), build_pin_index(id, node)?);
    }

    // 2. 校验每条边
    for edge in edges {
        let from_index = indexes.get(edge.from.as_str()).ok_or_else(|| {
            EngineError::invalid_graph(format!("边的源节点 `{}` 在节点表中不存在", edge.from))
        })?;
        let to_index = indexes.get(edge.to.as_str()).ok_or_else(|| {
            EngineError::invalid_graph(format!("边的目标节点 `{}` 在节点表中不存在", edge.to))
        })?;

        let from_pin_id = edge
            .source_port_id
            .as_deref()
            .unwrap_or(DEFAULT_OUTPUT_PIN_ID);
        let to_pin_id = edge
            .target_port_id
            .as_deref()
            .unwrap_or(DEFAULT_INPUT_PIN_ID);

        let from_pin =
            from_index
                .outputs
                .get(from_pin_id)
                .ok_or_else(|| EngineError::UnknownPin {
                    node: edge.from.clone(),
                    pin: from_pin_id.to_owned(),
                    direction: PinDirection::Output,
                })?;
        let to_pin = to_index
            .inputs
            .get(to_pin_id)
            .ok_or_else(|| EngineError::UnknownPin {
                node: edge.to.clone(),
                pin: to_pin_id.to_owned(),
                direction: PinDirection::Input,
            })?;

        if !from_pin.pin_type.is_compatible_with(&to_pin.pin_type) {
            return Err(EngineError::IncompatiblePinTypes {
                from: format!("{}.{}", edge.from, from_pin.id),
                to: format!("{}.{}", edge.to, to_pin.id),
                from_type: format!("{:?}", from_pin.pin_type),
                to_type: format!("{:?}", to_pin.pin_type),
            });
        }

        if !from_pin.kind.is_compatible_with(to_pin.kind) {
            return Err(EngineError::IncompatiblePinKinds {
                from: format!("{}.{}", edge.from, from_pin.id),
                to: format!("{}.{}", edge.to, to_pin.id),
                from_kind: from_pin.kind.to_string(),
                to_kind: to_pin.kind.to_string(),
            });
        }
    }

    // 3. 校验 required 输入引脚有上游入边
    let mut covered_inputs: std::collections::HashSet<(&str, &str)> =
        std::collections::HashSet::with_capacity(edges.len());
    for edge in edges {
        let to_pin_id = edge
            .target_port_id
            .as_deref()
            .unwrap_or(DEFAULT_INPUT_PIN_ID);
        covered_inputs.insert((edge.to.as_str(), to_pin_id));
    }

    for (node_id, index) in &indexes {
        for (pin_id, pin) in &index.inputs {
            if !pin.required {
                continue;
            }
            // 默认 "in" 引脚的根节点由 ingress 直接喂数据，不经过 WorkflowEdge——
            // 详见 PinDefinition::default_input rustdoc。
            if pin_id == DEFAULT_INPUT_PIN_ID {
                continue;
            }
            if covered_inputs.contains(&(*node_id, pin_id.as_str())) {
                continue;
            }
            return Err(EngineError::invalid_graph(format!(
                "节点 `{node_id}` 的必需输入引脚 `{pin_id}` 没有任何上游入边"
            )));
        }
    }

    Ok(())
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    use std::sync::Arc;

    use async_trait::async_trait;
    use nazh_core::{
        EngineError, NodeExecution, NodeTrait, PinDefinition, PinDirection, PinKind, PinType,
    };
    use serde_json::Value;
    use uuid::Uuid;

    /// 测试用 stub 节点，pin 列表通过构造函数注入。
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
        ) -> Result<NodeExecution, EngineError> {
            Ok(NodeExecution::broadcast(Value::Null))
        }
    }

    fn pin(id: &str, dir: PinDirection, ty: PinType, required: bool) -> PinDefinition {
        PinDefinition {
            id: id.to_owned(),
            label: id.to_owned(),
            pin_type: ty,
            direction: dir,
            required,
            kind: PinKind::Exec,
            description: None,
        }
    }

    fn pin_with_kind(
        id: &str,
        dir: PinDirection,
        ty: PinType,
        required: bool,
        kind: PinKind,
    ) -> PinDefinition {
        PinDefinition {
            id: id.to_owned(),
            label: id.to_owned(),
            pin_type: ty,
            direction: dir,
            required,
            kind,
            description: None,
        }
    }

    fn node(
        id: &str,
        inputs: Vec<PinDefinition>,
        outputs: Vec<PinDefinition>,
    ) -> (String, Arc<dyn NodeTrait>) {
        (
            id.to_owned(),
            Arc::new(StubNode {
                id: id.to_owned(),
                inputs,
                outputs,
            }) as Arc<dyn NodeTrait>,
        )
    }

    fn edge(from: &str, to: &str, source: Option<&str>, target: Option<&str>) -> WorkflowEdge {
        WorkflowEdge {
            from: from.to_owned(),
            to: to.to_owned(),
            source_port_id: source.map(str::to_owned),
            target_port_id: target.map(str::to_owned),
        }
    }

    #[test]
    fn 默认_any_的两节点直连通过校验() {
        let nodes = HashMap::from([
            node(
                "a",
                vec![PinDefinition::default_input()],
                vec![PinDefinition::default_output()],
            ),
            node(
                "b",
                vec![PinDefinition::default_input()],
                vec![PinDefinition::default_output()],
            ),
        ]);
        let edges = vec![edge("a", "b", None, None)];
        validate_pin_compatibility(&nodes, &edges).unwrap();
    }

    #[test]
    fn 类型不兼容报_incompatible_pin_types() {
        let nodes = HashMap::from([
            node(
                "a",
                vec![PinDefinition::default_input()],
                vec![pin("out", PinDirection::Output, PinType::String, false)],
            ),
            node(
                "b",
                vec![pin("in", PinDirection::Input, PinType::Integer, true)],
                vec![PinDefinition::default_output()],
            ),
        ]);
        let edges = vec![edge("a", "b", None, None)];
        let err = validate_pin_compatibility(&nodes, &edges).unwrap_err();
        match err {
            EngineError::IncompatiblePinTypes { from, to, .. } => {
                assert_eq!(from, "a.out");
                assert_eq!(to, "b.in");
            }
            other => panic!("应报 IncompatiblePinTypes，实际：{other:?}"),
        }
    }

    #[test]
    fn 引用不存在的_pin_id_报_unknown_pin() {
        let nodes = HashMap::from([
            node(
                "a",
                vec![PinDefinition::default_input()],
                vec![pin("out", PinDirection::Output, PinType::Any, false)],
            ),
            node(
                "b",
                vec![PinDefinition::default_input()],
                vec![PinDefinition::default_output()],
            ),
        ]);
        // 边声明 source_port_id = "ghost"，但 a 没有该 output
        let edges = vec![edge("a", "b", Some("ghost"), None)];
        let err = validate_pin_compatibility(&nodes, &edges).unwrap_err();
        match err {
            EngineError::UnknownPin { node, pin, .. } => {
                assert_eq!(node, "a");
                assert_eq!(pin, "ghost");
            }
            other => panic!("应报 UnknownPin，实际：{other:?}"),
        }
    }

    #[test]
    fn 重复_pin_id_报_duplicate_pin_id() {
        let nodes = HashMap::from([node(
            "a",
            vec![
                pin("dup", PinDirection::Input, PinType::Any, true),
                pin("dup", PinDirection::Input, PinType::String, false),
            ],
            vec![PinDefinition::default_output()],
        )]);
        let err = validate_pin_compatibility(&nodes, &[]).unwrap_err();
        match err {
            EngineError::DuplicatePinId { node, pin, .. } => {
                assert_eq!(node, "a");
                assert_eq!(pin, "dup");
            }
            other => panic!("应报 DuplicatePinId，实际：{other:?}"),
        }
    }

    #[test]
    fn 具名_required_输入缺入边报错() {
        let nodes = HashMap::from([
            node(
                "src",
                vec![PinDefinition::default_input()],
                vec![PinDefinition::default_output()],
            ),
            node(
                "sink",
                vec![
                    pin("primary", PinDirection::Input, PinType::Any, true),
                    pin("secondary", PinDirection::Input, PinType::Any, true),
                ],
                vec![PinDefinition::default_output()],
            ),
        ]);
        // 只连了 primary，secondary 没人喂——应该报错
        let edges = vec![edge("src", "sink", None, Some("primary"))];
        let err = validate_pin_compatibility(&nodes, &edges).unwrap_err();
        assert!(matches!(err, EngineError::InvalidGraph(_)));
    }

    #[test]
    fn 默认_in_required_的根节点不报错() {
        // 单节点图，没有任何边——根节点的默认 "in" required 不应触发"缺入边"，
        // 因为根节点由 ingress 直接喂数据。
        let nodes = HashMap::from([node(
            "root",
            vec![PinDefinition::default_input()],
            vec![PinDefinition::default_output()],
        )]);
        validate_pin_compatibility(&nodes, &[]).unwrap();
    }

    #[test]
    fn 分支节点路由到具名输出通过校验() {
        let nodes = HashMap::from([
            node(
                "if",
                vec![PinDefinition::default_input()],
                vec![
                    pin("true", PinDirection::Output, PinType::Any, false),
                    pin("false", PinDirection::Output, PinType::Any, false),
                ],
            ),
            node(
                "yes",
                vec![PinDefinition::default_input()],
                vec![PinDefinition::default_output()],
            ),
            node(
                "no",
                vec![PinDefinition::default_input()],
                vec![PinDefinition::default_output()],
            ),
        ]);
        let edges = vec![
            edge("if", "yes", Some("true"), None),
            edge("if", "no", Some("false"), None),
        ];
        validate_pin_compatibility(&nodes, &edges).unwrap();
    }

    #[test]
    fn array_嵌套兼容通过校验() {
        let nodes = HashMap::from([
            node(
                "a",
                vec![PinDefinition::default_input()],
                vec![pin(
                    "out",
                    PinDirection::Output,
                    PinType::Array {
                        inner: Box::new(PinType::Any),
                    },
                    false,
                )],
            ),
            node(
                "b",
                vec![pin(
                    "in",
                    PinDirection::Input,
                    PinType::Array {
                        inner: Box::new(PinType::Integer),
                    },
                    true,
                )],
                vec![PinDefinition::default_output()],
            ),
        ]);
        // Array(Any) 上游 → Array(Integer) 下游应通过
        let edges = vec![edge("a", "b", None, None)];
        validate_pin_compatibility(&nodes, &edges).unwrap();
    }

    #[test]
    fn 跨_kind_连接报_incompatible_pin_kinds() {
        let nodes = HashMap::from([
            node(
                "a",
                vec![PinDefinition::default_input()],
                vec![pin_with_kind(
                    "out",
                    PinDirection::Output,
                    PinType::Any,
                    false,
                    PinKind::Data,
                )],
            ),
            node(
                "b",
                vec![pin_with_kind(
                    "in",
                    PinDirection::Input,
                    PinType::Any,
                    false,
                    PinKind::Exec,
                )],
                vec![PinDefinition::default_output()],
            ),
        ]);
        let edges = vec![edge("a", "b", None, None)];
        let err = validate_pin_compatibility(&nodes, &edges).unwrap_err();
        match err {
            EngineError::IncompatiblePinKinds {
                from,
                to,
                from_kind,
                to_kind,
            } => {
                assert_eq!(from, "a.out");
                assert_eq!(to, "b.in");
                assert_eq!(from_kind, "data");
                assert_eq!(to_kind, "exec");
            }
            other => panic!("应报 IncompatiblePinKinds，实际：{other:?}"),
        }
    }

    #[test]
    fn 同_kind_data_data_连接通过校验() {
        let nodes = HashMap::from([
            node(
                "a",
                vec![PinDefinition::default_input()],
                vec![pin_with_kind(
                    "out",
                    PinDirection::Output,
                    PinType::Any,
                    false,
                    PinKind::Data,
                )],
            ),
            node(
                "b",
                vec![pin_with_kind(
                    "in",
                    PinDirection::Input,
                    PinType::Any,
                    false,
                    PinKind::Data,
                )],
                vec![PinDefinition::default_output()],
            ),
        ]);
        let edges = vec![edge("a", "b", None, None)];
        validate_pin_compatibility(&nodes, &edges).unwrap();
    }
}
