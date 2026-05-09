use super::*;
use async_trait::async_trait;
use nazh_core::{
    EmptyPolicy, EngineError as CoreError, NodeCapabilities, NodeExecution, PinDefinition,
    PinDirection, PinKind, PinType, VariableDeclaration, WorkflowVariables,
};
use serde_json::{Value, json};
use std::collections::HashMap;
use uuid::Uuid;

struct DataOnlyEdgeProbeNode {
    id: String,
    is_source: bool,
}

#[async_trait]
impl NodeTrait for DataOnlyEdgeProbeNode {
    fn id(&self) -> &str {
        &self.id
    }

    fn kind(&self) -> &'static str {
        "dataOnlyEdgeProbe"
    }

    fn input_pins(&self) -> Vec<PinDefinition> {
        if self.is_source {
            vec![PinDefinition::default_input()]
        } else {
            vec![
                PinDefinition::default_input(),
                PinDefinition {
                    id: "sensor".to_owned(),
                    label: "sensor".to_owned(),
                    pin_type: PinType::Any,
                    direction: PinDirection::Input,
                    required: false,
                    kind: PinKind::Data,
                    description: None,
                    empty_policy: EmptyPolicy::default(),
                    block_timeout_ms: None,
                    ttl_ms: None,
                },
            ]
        }
    }

    fn output_pins(&self) -> Vec<PinDefinition> {
        if self.is_source {
            vec![
                PinDefinition::default_output(),
                PinDefinition::output_named_data(
                    "latest",
                    "latest",
                    PinType::Any,
                    "测试用 Data 输出",
                ),
            ]
        } else {
            vec![PinDefinition::default_output()]
        }
    }

    async fn transform(
        &self,
        _trace_id: Uuid,
        _payload: Value,
    ) -> Result<NodeExecution, CoreError> {
        Ok(NodeExecution::broadcast(Value::Null))
    }
}

#[tokio::test]
async fn 变量覆盖值在_on_deploy_前恢复且保留声明初值() {
    let mut declarations = HashMap::new();
    declarations.insert(
        "counter".to_owned(),
        VariableDeclaration {
            variable_type: PinType::Integer,
            initial: json!(0),
        },
    );
    let graph = WorkflowGraph {
        name: Some("wf".to_owned()),
        nodes: HashMap::new(),
        edges: Vec::new(),
        connections: Vec::new(),
        variables: Some(declarations),
    };
    let mut overrides = HashMap::new();
    overrides.insert("counter".to_owned(), json!(42));

    let deployment = deploy_workflow_with_ai_and_variable_overrides(
        graph,
        connections::shared_connection_manager(),
        None,
        &NodeRegistry::new(),
        Some("wf".to_owned()),
        RuntimeResources::new(),
        overrides,
    )
    .await
    .unwrap();
    let parts = deployment.into_parts();
    let vars = parts
        .shared_resources
        .get::<Arc<WorkflowVariables>>()
        .unwrap();
    let counter = vars.get("counter").unwrap();

    assert_eq!(counter.value, json!(42));
    assert_eq!(counter.initial, json!(0));
    assert_eq!(counter.updated_by.as_deref(), Some("restore"));
}

#[tokio::test]
async fn data_边不影响部署入口_root_识别() {
    let mut registry = NodeRegistry::new();
    registry.register_with_capabilities("probeSource", NodeCapabilities::empty(), |def, _| {
        Ok(Arc::new(DataOnlyEdgeProbeNode {
            id: def.id().to_owned(),
            is_source: true,
        }))
    });
    registry.register_with_capabilities("probeSink", NodeCapabilities::empty(), |def, _| {
        Ok(Arc::new(DataOnlyEdgeProbeNode {
            id: def.id().to_owned(),
            is_source: false,
        }))
    });

    let graph: WorkflowGraph = serde_json::from_value(json!({
            "name": "data_edge_root_regression",
            "nodes": {
                "source": { "id": "source", "type": "probeSource", "config": {} },
                "sink": { "id": "sink", "type": "probeSink", "config": {} }
            },
            "edges": [
                { "from": "source", "to": "sink", "source_port_id": "latest", "target_port_id": "sensor" }
            ],
            "connections": []
        }))
        .unwrap();

    let deployment = deploy_workflow(graph, connections::shared_connection_manager(), &registry)
        .await
        .unwrap();

    assert_eq!(
        deployment.ingress.root_nodes(),
        &["sink".to_owned(), "source".to_owned()],
        "Data-only 边不应把 sink 从外部触发入口中移除"
    );

    deployment.shutdown().await;
}
