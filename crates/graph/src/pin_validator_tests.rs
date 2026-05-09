use super::*;

use std::sync::Arc;

use async_trait::async_trait;
use nazh_core::{
    EmptyPolicy, EngineError, NodeExecution, NodeTrait, PinDefinition, PinDirection, PinKind,
    PinType,
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
        empty_policy: EmptyPolicy::default(),
        block_timeout_ms: None,
        ttl_ms: None,
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
        empty_policy: EmptyPolicy::default(),
        block_timeout_ms: None,
        ttl_ms: None,
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
            assert_eq!(from_kind, PinKind::Data);
            assert_eq!(to_kind, PinKind::Exec);
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
                "data_in",
                PinDirection::Input,
                PinType::Any,
                false,
                PinKind::Data,
            )],
            vec![PinDefinition::default_output()],
        ),
    ]);
    let edges = vec![edge("a", "b", Some("out"), Some("data_in"))];
    validate_pin_compatibility(&nodes, &edges).unwrap();
}

#[test]
fn data_输入_pin_id_为_in_时拒绝部署() {
    let nodes = HashMap::from([node(
        "bad",
        vec![pin_with_kind(
            "in",
            PinDirection::Input,
            PinType::Any,
            false,
            PinKind::Data,
        )],
        vec![PinDefinition::default_output()],
    )]);
    let err = validate_pin_compatibility(&nodes, &[]).unwrap_err();
    match err {
        EngineError::ReservedPinId { node, pin, reason } => {
            assert_eq!(node, "bad");
            assert_eq!(pin, "in");
            assert!(reason.contains("保留"));
        }
        other => panic!("应报 ReservedPinId，实际：{other:?}"),
    }
}
