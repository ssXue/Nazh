//! ADR-0014 Phase 1 端到端集成测试：stub 节点声明 Data 输出 pin，
//! deploy + submit 后断言不 panic、`Completed` 事件正常发出。
//!
//! Phase 1 不暴露 `OutputCache` 给壳层——cache 写入由 `cache::tests` 单测覆盖；
//! 跨 Kind 拒绝由 `pin_validator::tests` 覆盖；Data 边环检测由 `topology::tests` 覆盖。
//! 本集成测试只验证 Runner 双路径骨架不破坏现有部署/触发路径。

#![allow(clippy::expect_used)]

use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;
use nazh_core::PinKind;
use nazh_engine::{
    CompletedExecutionEvent, EngineError, ExecutionEvent, NodeCapabilities, NodeExecution,
    NodeRegistry, NodeTrait, PinDefinition, PinDirection, PinType, WorkflowContext, WorkflowGraph,
    deploy_workflow, shared_connection_manager,
};
use serde_json::{Value, json};
use uuid::Uuid;

/// 声明双输出（Exec + Data）的 stub 节点：transform 直接返回 input payload。
struct DualOutputStub {
    id: String,
}

#[async_trait]
impl NodeTrait for DualOutputStub {
    fn id(&self) -> &str {
        &self.id
    }
    fn kind(&self) -> &'static str {
        "dualOutputStub"
    }
    fn input_pins(&self) -> Vec<PinDefinition> {
        vec![PinDefinition::default_input()]
    }
    fn output_pins(&self) -> Vec<PinDefinition> {
        vec![
            PinDefinition::default_output(), // Exec out（默认 kind = Exec）
            PinDefinition {
                id: "latest".to_owned(),
                label: "latest".to_owned(),
                pin_type: PinType::Any,
                direction: PinDirection::Output,
                required: false,
                kind: PinKind::Data,
                description: None,
            },
        ]
    }
    async fn transform(
        &self,
        _trace_id: Uuid,
        payload: Value,
    ) -> Result<NodeExecution, EngineError> {
        Ok(NodeExecution::broadcast(payload))
    }
}

#[tokio::test]
async fn data_输出_pin_的节点_transform_后_部署不_panic_且发出_completed_事件() {
    // 1. 注册 stub 节点
    let mut registry = NodeRegistry::default();
    registry.register_with_capabilities(
        "dualOutputStub",
        NodeCapabilities::empty(),
        |def, _res| {
            Ok(Arc::new(DualOutputStub {
                id: def.id().to_owned(),
            }) as Arc<dyn NodeTrait>)
        },
    );

    // 2. 构造图：单个 stub 节点为根，Data 输出无下游（无下游也不影响——Data 只写 cache）
    let stub_def = serde_json::from_value::<nazh_engine::WorkflowNodeDefinition>(json!({
        "id": "stub",
        "type": "dualOutputStub",
        "config": {}
    }))
    .expect("WorkflowNodeDefinition deserialize");
    let mut nodes = HashMap::new();
    nodes.insert("stub".to_owned(), stub_def);
    let graph = WorkflowGraph {
        name: Some("phase1-data-cache-test".to_owned()),
        connections: vec![],
        nodes,
        edges: vec![],
        variables: None,
    };

    // 3. 部署
    let conn_mgr = shared_connection_manager();
    let mut deployment = deploy_workflow(graph, conn_mgr, &registry)
        .await
        .expect("deploy should succeed");

    // 4. submit 一个 payload，等待 Completed 事件（带超时兜底防卡死）
    deployment
        .submit(WorkflowContext::new(Value::from(42_i64)))
        .await
        .expect("submit should succeed");

    let completed = tokio::time::timeout(
        std::time::Duration::from_secs(2),
        wait_for_completion(&mut deployment),
    )
    .await
    .expect("transform 应该在 2s 内完成")
    .expect("应有 Completed 事件");

    // 5. 断言基本信息
    assert_eq!(completed.stage, "stub");

    // 6. 优雅关闭
    deployment.shutdown().await;
}

async fn wait_for_completion(
    deployment: &mut nazh_engine::WorkflowDeployment,
) -> Option<CompletedExecutionEvent> {
    while let Some(event) = deployment.next_event().await {
        if let ExecutionEvent::Completed(c) = event {
            return Some(c);
        }
    }
    None
}
