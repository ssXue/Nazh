//! ADR-0014 Phase 2 集成测试：`modbusRead` `latest` Data 引脚的端到端验证。
//!
//! 单节点 `modbusRead` 工作流（无 `connection_id` 走模拟模式）。断言 transform 执行后：
//! 1. `OutputCache` 的 `latest` 槽被写入（Phase 1 双路径骨架在生产节点首次激活）
//! 2. Exec `out` 路径同样把结果推到 result 通道（双路径同源 payload）
//!
//! 与 `tests/pin_kind_phase1.rs` 形成 stub→真实节点的覆盖梯度。

#![allow(clippy::expect_used)]

use std::collections::HashMap;

use nazh_engine::{
    WorkflowContext, WorkflowGraph, deploy_workflow, shared_connection_manager, standard_registry,
};
use serde_json::{Value, json};

#[tokio::test]
async fn modbus_read_的_latest_data_引脚被写入缓存槽_且_exec_out_仍推送() {
    // 1. 标准注册表（包含 modbusRead），无连接走模拟模式
    let registry = standard_registry();

    // 2. 构造单节点 modbusRead 工作流（modbusRead 是根节点，无上游入边）
    //    无 connection_id → 走 simulate_and_build 路径，输出 {"value": <number>}
    let modbus_def = serde_json::from_value::<nazh_engine::WorkflowNodeDefinition>(json!({
        "id": "reader",
        "type": "modbusRead",
        "config": {
            "register_type": "holding",
            "register": 0,
            "quantity": 2
            // 无 connection_id → 模拟模式
        }
    }))
    .expect("modbusRead WorkflowNodeDefinition 反序列化");

    let mut nodes = HashMap::new();
    nodes.insert("reader".to_owned(), modbus_def);
    let graph = WorkflowGraph {
        name: Some("phase2-modbus-latest-cache-test".to_owned()),
        connections: vec![],
        nodes,
        edges: vec![],
        variables: None,
    };

    // 3. 部署
    let conn_mgr = shared_connection_manager();
    let mut deployment = deploy_workflow(graph, conn_mgr, &registry)
        .await
        .expect("modbusRead 单节点部署应该成功");

    // 4. submit 触发一次执行
    deployment
        .submit(WorkflowContext::new(Value::Object(serde_json::Map::new())))
        .await
        .expect("submit 应该成功");

    // 5. 等待 Completed 事件（带超时兜底，参考 phase1 测试的模式）
    let completed = tokio::time::timeout(
        std::time::Duration::from_secs(2),
        wait_for_completion(&mut deployment),
    )
    .await
    .expect("transform 应在 2s 内完成")
    .expect("应有 Completed 事件");
    assert_eq!(completed.stage, "reader");

    // 6. 断言 1：OutputCache 的 latest 槽被写入
    let cache = deployment
        .output_cache("reader")
        .expect("reader 节点应该有 OutputCache（声明了 Data 引脚）");
    let cached = cache
        .read("latest")
        .expect("latest 槽应该被 modbusRead transform 写入");
    // 模拟模式产出的 JSON object——quantity=2 时 payload 含 "values" 数组键
    assert!(
        cached.value.is_object(),
        "modbusRead 模拟模式 latest 应是 JSON object，实际：{}",
        cached.value
    );

    // 7. 断言 2：Exec out 路径推到 result 通道（modbusRead 是图唯一节点 = 叶子）
    let result = deployment
        .next_result()
        .await
        .expect("Exec out 应该把结果推到 result 通道（无下游 → 走 result_tx）");
    // 同源 payload：result_tx 收到的 ctx.payload 与 cache.latest.value 应相同
    assert_eq!(
        result.payload, cached.value,
        "Exec out 与 Data latest 应是同源 payload"
    );

    // 8. 优雅关闭
    deployment.shutdown().await;
}

async fn wait_for_completion(
    deployment: &mut nazh_engine::WorkflowDeployment,
) -> Option<nazh_engine::CompletedExecutionEvent> {
    while let Some(event) = deployment.next_event().await {
        if let nazh_engine::ExecutionEvent::Completed(c) = event {
            return Some(c);
        }
    }
    None
}
