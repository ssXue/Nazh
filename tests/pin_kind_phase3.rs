//! ADR-0014 Phase 3：pure-form 节点 + Runner 拉路径端到端集成测试。

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::too_many_lines,
    clippy::similar_names,
    clippy::unnecessary_mut_passed,
    clippy::redundant_closure_for_method_calls
)]

use std::sync::Arc;

use async_trait::async_trait;
use nazh_core::{
    EmptyPolicy, EngineError, NodeCapabilities, NodeExecution, NodeTrait, PinDefinition,
    PinDirection, PinKind, PinType,
};
use nazh_engine::{
    WorkflowContext, WorkflowGraph, deploy_workflow, shared_connection_manager, standard_registry,
};
use serde_json::{Value, json};
use std::time::Duration;
use tokio::time::timeout;
use uuid::Uuid;

// ---- 本测试用的 stub 节点：Exec 触发，输出包含 `value` Float 写入 Data 缓存 ----

struct CelsiusSourceNode {
    id: String,
    constant_celsius: f64,
}

#[async_trait]
impl NodeTrait for CelsiusSourceNode {
    fn id(&self) -> &str {
        &self.id
    }
    fn kind(&self) -> &'static str {
        "celsiusSource"
    }
    fn input_pins(&self) -> Vec<PinDefinition> {
        vec![PinDefinition::default_input()]
    }
    fn output_pins(&self) -> Vec<PinDefinition> {
        vec![
            PinDefinition::default_output(),
            PinDefinition::output_named_data(
                "value",
                "value",
                PinType::Float,
                "测试用：写入 Data 缓存的常量摄氏温度",
            ),
        ]
    }
    async fn transform(
        &self,
        _trace_id: Uuid,
        _payload: Value,
    ) -> Result<NodeExecution, EngineError> {
        Ok(NodeExecution::broadcast(
            json!({ "value": self.constant_celsius }),
        ))
    }
}

// ---- 本测试用的 sink 节点：声明 Data 输入 `temp_f` 拉取 c2f 的输出 ----

struct AssertingSinkNode {
    id: String,
    captured: tokio::sync::mpsc::Sender<Value>,
}

#[async_trait]
impl NodeTrait for AssertingSinkNode {
    fn id(&self) -> &str {
        &self.id
    }
    fn kind(&self) -> &'static str {
        "assertingSink"
    }
    fn input_pins(&self) -> Vec<PinDefinition> {
        vec![
            PinDefinition::default_input(),
            PinDefinition {
                id: "temp_f".to_owned(),
                label: "temp_f".to_owned(),
                pin_type: PinType::Float,
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
    fn output_pins(&self) -> Vec<PinDefinition> {
        vec![PinDefinition::default_output()]
    }
    async fn transform(
        &self,
        _trace_id: Uuid,
        payload: Value,
    ) -> Result<NodeExecution, EngineError> {
        self.captured.send(payload.clone()).await.ok();
        Ok(NodeExecution::broadcast(payload))
    }
}

struct JsonDataSinkNode {
    id: String,
    captured: tokio::sync::mpsc::Sender<Value>,
}

#[async_trait]
impl NodeTrait for JsonDataSinkNode {
    fn id(&self) -> &str {
        &self.id
    }
    fn kind(&self) -> &'static str {
        "jsonDataSink"
    }
    fn input_pins(&self) -> Vec<PinDefinition> {
        vec![
            PinDefinition::default_input(),
            PinDefinition {
                id: "latest".to_owned(),
                label: "latest".to_owned(),
                pin_type: PinType::Json,
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
    fn output_pins(&self) -> Vec<PinDefinition> {
        vec![PinDefinition::default_output()]
    }
    async fn transform(
        &self,
        _trace_id: Uuid,
        payload: Value,
    ) -> Result<NodeExecution, EngineError> {
        self.captured.send(payload.clone()).await.ok();
        Ok(NodeExecution::broadcast(payload))
    }
}

struct PanickingPureNode {
    id: String,
}

#[async_trait]
impl NodeTrait for PanickingPureNode {
    fn id(&self) -> &str {
        &self.id
    }
    fn kind(&self) -> &'static str {
        "panickingPure"
    }
    fn input_pins(&self) -> Vec<PinDefinition> {
        vec![PinDefinition {
            id: "value".to_owned(),
            label: "value".to_owned(),
            pin_type: PinType::Float,
            direction: PinDirection::Input,
            required: true,
            kind: PinKind::Data,
            description: None,
            empty_policy: EmptyPolicy::default(),
            block_timeout_ms: None,
            ttl_ms: None,
        }]
    }
    fn output_pins(&self) -> Vec<PinDefinition> {
        vec![PinDefinition {
            id: "out".to_owned(),
            label: "out".to_owned(),
            pin_type: PinType::Float,
            direction: PinDirection::Output,
            required: false,
            kind: PinKind::Data,
            description: None,
            empty_policy: EmptyPolicy::default(),
            block_timeout_ms: None,
            ttl_ms: None,
        }]
    }
    async fn transform(
        &self,
        _trace_id: Uuid,
        _payload: Value,
    ) -> Result<NodeExecution, EngineError> {
        panic!("pure-form panic regression");
    }
}

// ---- 测试主体 ----

#[tokio::test(flavor = "multi_thread")]
async fn pure_chain_被_celsius_source_触发拉取() {
    let (sink_tx, mut sink_rx) = tokio::sync::mpsc::channel::<Value>(4);

    let mut registry = standard_registry();
    {
        let sink_tx = sink_tx.clone();
        registry.register_with_capabilities(
            "assertingSink",
            NodeCapabilities::empty(),
            move |def, _res| {
                Ok(Arc::new(AssertingSinkNode {
                    id: def.id().to_owned(),
                    captured: sink_tx.clone(),
                }))
            },
        );
    }
    registry.register_with_capabilities("celsiusSource", NodeCapabilities::empty(), |def, _res| {
        let celsius = def
            .config()
            .get("celsius")
            .and_then(|v| v.as_f64())
            .unwrap_or(25.0);
        Ok(Arc::new(CelsiusSourceNode {
            id: def.id().to_owned(),
            constant_celsius: celsius,
        }))
    });

    // ---- 调试：source → sink（Exec + Data 边），验证 pull collector ----
    let graph: WorkflowGraph = serde_json::from_value(json!({
        "name": "source_sink_data",
        "nodes": {
            "source": { "id": "source", "type": "celsiusSource", "config": { "celsius": 25.0 } },
            "sink": { "id": "sink", "type": "assertingSink", "config": {} }
        },
        "edges": [
            { "from": "source", "to": "sink", "source_port_id": "out", "target_port_id": "in" },
            { "from": "source", "to": "sink", "source_port_id": "value", "target_port_id": "temp_f" }
        ],
        "connections": []
    }))
    .expect("图 JSON 解析");

    let conn_manager = shared_connection_manager();
    let deployment = deploy_workflow(graph, conn_manager, &registry)
        .await
        .expect("部署成功");

    deployment
        .submit(WorkflowContext::new(json!({})))
        .await
        .expect("submit");

    let captured = timeout(Duration::from_secs(5), sink_rx.recv())
        .await
        .expect("超时未收到 sink 调用")
        .expect("sink_rx 被关闭");

    // source 的 value=25.0 应通过 Data pull 进入 sink 的 temp_f
    let temp_f = captured
        .get("temp_f")
        .and_then(|v| v.as_f64())
        .expect("temp_f 应在 payload 中（Data pull from source）");
    assert!(
        (temp_f - 25.0).abs() < 1e-9,
        "temp_f 应为 25.0, got {temp_f}"
    );

    deployment.shutdown().await;

    // ---- 完整测试：source → c2f → sink（pure chain，25°C → 77°F）----
    let (sink_tx2, mut sink_rx2) = tokio::sync::mpsc::channel::<Value>(4);

    let mut registry2 = standard_registry();
    {
        let sink_tx = sink_tx2.clone();
        registry2.register_with_capabilities(
            "assertingSink",
            NodeCapabilities::empty(),
            move |def, _res| {
                Ok(Arc::new(AssertingSinkNode {
                    id: def.id().to_owned(),
                    captured: sink_tx.clone(),
                }))
            },
        );
    }
    registry2.register_with_capabilities(
        "celsiusSource",
        NodeCapabilities::empty(),
        |def, _res| {
            let celsius = def
                .config()
                .get("celsius")
                .and_then(|v| v.as_f64())
                .unwrap_or(25.0);
            Ok(Arc::new(CelsiusSourceNode {
                id: def.id().to_owned(),
                constant_celsius: celsius,
            }))
        },
    );

    let graph2: WorkflowGraph = serde_json::from_value(json!({
        "name": "pure_chain_test",
        "nodes": {
            "source": { "id": "source", "type": "celsiusSource", "config": { "celsius": 25.0 } },
            "c2f": { "id": "c2f", "type": "c2f", "config": {} },
            "sink": { "id": "sink", "type": "assertingSink", "config": {} }
        },
        "edges": [
            { "from": "source", "to": "sink", "source_port_id": "out", "target_port_id": "in" },
            { "from": "source", "to": "c2f", "source_port_id": "value", "target_port_id": "value" },
            { "from": "c2f", "to": "sink", "source_port_id": "out", "target_port_id": "temp_f" }
        ],
        "connections": []
    }))
    .expect("图 JSON 解析");

    let conn_manager2 = shared_connection_manager();
    let deployment2 = deploy_workflow(graph2, conn_manager2, &registry2)
        .await
        .expect("纯链部署成功");

    deployment2
        .submit(WorkflowContext::new(json!({})))
        .await
        .expect("submit");

    let captured2 = timeout(Duration::from_secs(5), sink_rx2.recv())
        .await
        .expect("超时未收到 sink 调用（纯链）")
        .expect("sink_rx2 被关闭");

    let temp_f = captured2
        .get("temp_f")
        .and_then(|v| v.as_f64())
        .expect("temp_f 应在 payload 中");
    assert!((temp_f - 77.0).abs() < 1e-9, "25°C → 77°F, got {temp_f}");

    deployment2.shutdown().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn modbus_latest_data_pin_拉取完整缓存_payload() {
    let (sink_tx, mut sink_rx) = tokio::sync::mpsc::channel::<Value>(4);

    let mut registry = standard_registry();
    {
        let sink_tx = sink_tx.clone();
        registry.register_with_capabilities(
            "jsonDataSink",
            NodeCapabilities::empty(),
            move |def, _res| {
                Ok(Arc::new(JsonDataSinkNode {
                    id: def.id().to_owned(),
                    captured: sink_tx.clone(),
                }))
            },
        );
    }

    let graph: WorkflowGraph = serde_json::from_value(json!({
        "name": "modbus_latest_pull_regression",
        "nodes": {
            "reader": {
                "id": "reader",
                "type": "modbusRead",
                "config": {
                    "register_type": "holding",
                    "register": 0,
                    "quantity": 2
                }
            },
            "sink": { "id": "sink", "type": "jsonDataSink", "config": {} }
        },
        "edges": [
            { "from": "reader", "to": "sink", "source_port_id": "out", "target_port_id": "in" },
            { "from": "reader", "to": "sink", "source_port_id": "latest", "target_port_id": "latest" }
        ],
        "connections": []
    }))
    .expect("图 JSON 解析");

    let conn_manager = shared_connection_manager();
    let deployment = deploy_workflow(graph, conn_manager, &registry)
        .await
        .expect("部署成功");

    deployment
        .submit(WorkflowContext::new(json!({})))
        .await
        .expect("submit");

    let captured = timeout(Duration::from_secs(5), sink_rx.recv())
        .await
        .expect("超时未收到 sink 调用")
        .expect("sink_rx 被关闭");

    let latest = captured
        .get("latest")
        .and_then(Value::as_object)
        .expect("latest 应是从 OutputCache 拉取的完整 JSON payload");
    assert!(
        latest.get("values").and_then(Value::as_array).is_some(),
        "latest 应保留 modbusRead.latest 槽内完整 payload，而不是按 pin id 二次取字段"
    );

    deployment.shutdown().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn pure_form_递归求值_panic_会被_guard_隔离() {
    let (sink_tx, _sink_rx) = tokio::sync::mpsc::channel::<Value>(4);

    let mut registry = standard_registry();
    {
        let sink_tx = sink_tx.clone();
        registry.register_with_capabilities(
            "assertingSink",
            NodeCapabilities::empty(),
            move |def, _res| {
                Ok(Arc::new(AssertingSinkNode {
                    id: def.id().to_owned(),
                    captured: sink_tx.clone(),
                }))
            },
        );
    }
    registry.register_with_capabilities("celsiusSource", NodeCapabilities::empty(), |def, _res| {
        Ok(Arc::new(CelsiusSourceNode {
            id: def.id().to_owned(),
            constant_celsius: 25.0,
        }))
    });
    registry.register_with_capabilities("panickingPure", NodeCapabilities::empty(), |def, _res| {
        Ok(Arc::new(PanickingPureNode {
            id: def.id().to_owned(),
        }))
    });

    let graph: WorkflowGraph = serde_json::from_value(json!({
        "name": "pure_panic_guard_regression",
        "nodes": {
            "source": { "id": "source", "type": "celsiusSource", "config": {} },
            "panicky": { "id": "panicky", "type": "panickingPure", "config": {}, "timeout_ms": 100 },
            "sink": { "id": "sink", "type": "assertingSink", "config": {} }
        },
        "edges": [
            { "from": "source", "to": "sink", "source_port_id": "out", "target_port_id": "in" },
            { "from": "source", "to": "panicky", "source_port_id": "value", "target_port_id": "value" },
            { "from": "panicky", "to": "sink", "source_port_id": "out", "target_port_id": "temp_f" }
        ],
        "connections": []
    }))
    .expect("图 JSON 解析");

    let conn_manager = shared_connection_manager();
    let mut deployment = deploy_workflow(graph, conn_manager, &registry)
        .await
        .expect("部署成功");

    deployment
        .submit(WorkflowContext::new(json!({})))
        .await
        .expect("submit");

    let (stage, error) = timeout(Duration::from_secs(5), wait_for_failed(&mut deployment))
        .await
        .expect("超时未收到 Failed 事件")
        .expect("应收到 Failed 事件");
    assert_eq!(stage, "sink");
    assert!(
        error.contains("panicky") && error.contains("panic"),
        "pure-form panic 应被 guarded_execute 转成失败事件，实际错误：{error}"
    );

    deployment.shutdown().await;
}

async fn wait_for_failed(
    deployment: &mut nazh_engine::WorkflowDeployment,
) -> Option<(String, String)> {
    while let Some(event) = deployment.next_event().await {
        if let nazh_engine::ExecutionEvent::Failed { stage, error, .. } = event {
            return Some((stage, error));
        }
    }
    None
}
