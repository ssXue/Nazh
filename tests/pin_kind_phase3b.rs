//! ADR-0014 Phase 3b：真混合输入（Exec ▶in + Data ●ext）节点端到端测试。
//!
//! 验证 Phase 3 实现的 `pull_data_inputs` 在混合场景下正确合并 payload，
//! 节点 transform 收到既含 Exec push 内容又含 Data pull 值的 payload。
//!
//! 图结构：emitter(Exec out → mixed.in) + emitter(Data key → lookup.key)
//!          + lookup(Data out → mixed.ext)
//! emitter 输出 `{key: "alpha"}`，lookup table 查表命中 `"lookup-hit"`，
//! mixed.transform 收到 `{key: "alpha", ext: "lookup-hit"}`。

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::too_many_lines,
    clippy::similar_names
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

// ---- stub：Exec 触发 + Data 输出（提供 lookup 的 key）----

struct KeyEmitterNode {
    id: String,
    key: String,
}

#[async_trait]
impl NodeTrait for KeyEmitterNode {
    fn id(&self) -> &str {
        &self.id
    }
    fn kind(&self) -> &'static str {
        "keyEmitter"
    }
    fn input_pins(&self) -> Vec<PinDefinition> {
        vec![PinDefinition::default_input()]
    }
    fn output_pins(&self) -> Vec<PinDefinition> {
        vec![
            PinDefinition::default_output(),
            PinDefinition {
                id: "key".to_owned(),
                label: "key".to_owned(),
                pin_type: PinType::Any,
                direction: PinDirection::Output,
                required: false,
                kind: PinKind::Data,
                description: Some("测试用：写入 Data 缓存的常量 key".to_owned()),
                empty_policy: EmptyPolicy::default(),
                block_timeout_ms: None,
                ttl_ms: None,
            },
        ]
    }
    async fn transform(&self, _: Uuid, _: Value) -> Result<NodeExecution, EngineError> {
        Ok(NodeExecution::broadcast(json!({ "key": self.key })))
    }
}

// ---- stub：混合输入（Exec ▶in + Data ●ext），输出合并后 payload 给 sink ----

struct MixedFormatterNode {
    id: String,
    captured: tokio::sync::mpsc::Sender<Value>,
}

#[async_trait]
impl NodeTrait for MixedFormatterNode {
    fn id(&self) -> &str {
        &self.id
    }
    fn kind(&self) -> &'static str {
        "mixedFormatter"
    }
    fn input_pins(&self) -> Vec<PinDefinition> {
        vec![
            PinDefinition::default_input(),
            PinDefinition {
                id: "ext".to_owned(),
                label: "ext".to_owned(),
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
    fn output_pins(&self) -> Vec<PinDefinition> {
        vec![PinDefinition::default_output()]
    }
    async fn transform(&self, _: Uuid, payload: Value) -> Result<NodeExecution, EngineError> {
        self.captured.send(payload.clone()).await.ok();
        Ok(NodeExecution::broadcast(payload))
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn 混合输入节点的_transform_收到合并后_payload() {
    let (tx, mut rx) = tokio::sync::mpsc::channel::<Value>(4);

    let mut registry = standard_registry();
    {
        let tx = tx.clone();
        registry.register_with_capabilities(
            "mixedFormatter",
            NodeCapabilities::empty(),
            move |def, _res| {
                Ok(Arc::new(MixedFormatterNode {
                    id: def.id().to_owned(),
                    captured: tx.clone(),
                }))
            },
        );
    }
    registry.register_with_capabilities("keyEmitter", NodeCapabilities::empty(), |def, _res| {
        let key = def
            .config()
            .get("key")
            .and_then(|v| v.as_str())
            .unwrap_or("alpha")
            .to_owned();
        Ok(Arc::new(KeyEmitterNode {
            id: def.id().to_owned(),
            key,
        }))
    });

    // 图：emitter(Exec out → mixed.in)
    //     emitter(Data key → lookup.key)
    //     lookup(Data out → mixed.ext)
    let graph: WorkflowGraph = serde_json::from_value(json!({
        "name": "p3b",
        "nodes": {
            "emitter": { "id": "emitter", "type": "keyEmitter", "config": { "key": "alpha" } },
            "lk": { "id": "lk", "type": "lookup", "config": {
                "table": { "alpha": "lookup-hit" },
                "default": null
            }},
            "mixed": { "id": "mixed", "type": "mixedFormatter", "config": {} }
        },
        "edges": [
            { "from": "emitter", "to": "mixed", "source_port_id": "out", "target_port_id": "in" },
            { "from": "emitter", "to": "lk", "source_port_id": "key", "target_port_id": "key" },
            { "from": "lk", "to": "mixed", "source_port_id": "out", "target_port_id": "ext" }
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

    let merged = timeout(Duration::from_secs(5), rx.recv())
        .await
        .expect("超时未收到 mixed 调用")
        .expect("rx 被关闭");

    // emitter Exec out 推 `{key: "alpha"}` 给 mixed.in；
    // lookup 拉 emitter.key="alpha"，table 命中 "lookup-hit"，
    // mixed.transform 收到 merge_payload({key: "alpha"}, {ext: "lookup-hit"})
    //                  = {key: "alpha", ext: "lookup-hit"}
    assert_eq!(
        merged.get("key").and_then(|v| v.as_str()),
        Some("alpha"),
        "exec payload 的 key 字段应为 alpha"
    );
    assert_eq!(
        merged.get("ext").and_then(|v| v.as_str()),
        Some("lookup-hit"),
        "data pull 的 ext 字段应为 lookup-hit"
    );

    deployment.shutdown().await;
}
