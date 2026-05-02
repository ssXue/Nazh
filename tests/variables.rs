//! 端到端：变量声明在部署期初始化、注入 `NodeLifecycleContext` 与 `SharedResources`（ADR-0012 Task 5~7）。

#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::{collections::HashMap, sync::Arc, time::Duration};

use nazh_engine::{
    NodeRegistry, PinType, RuntimeResources, VariableDeclaration,
    WorkflowContext, WorkflowGraph, WorkflowVariableEvent, WorkflowVariables,
    deploy_workflow_with_ai, shared_connection_manager, standard_registry,
};
use serde_json::json;
use tokio::time::timeout;

#[tokio::test]
async fn 部署时变量按声明初始化() {
    let mut declarations = HashMap::new();
    declarations.insert(
        "setpoint".to_owned(),
        VariableDeclaration {
            variable_type: PinType::Float,
            initial: json!(25.0),
        },
    );
    let graph = WorkflowGraph {
        name: Some("vars-empty".to_owned()),
        connections: vec![],
        nodes: HashMap::new(),
        edges: vec![],
        variables: Some(declarations),
    };

    let registry: NodeRegistry = standard_registry();
    let cm = shared_connection_manager();
    let deployment =
        deploy_workflow_with_ai(graph, cm, None, &registry, None, RuntimeResources::new())
            .await
            .expect("空 DAG + 单变量应能部署");

    // 变量可达性验证留 Task 7（nodes-flow E2E）；此处只确认部署管线成功 + 干净撤销。
    deployment.shutdown().await;
}

#[tokio::test]
async fn 初值类型不匹配_部署失败() {
    let mut declarations = HashMap::new();
    declarations.insert(
        "bad".to_owned(),
        VariableDeclaration {
            variable_type: PinType::Integer,
            initial: json!("not-a-number"),
        },
    );
    let graph = WorkflowGraph {
        name: None,
        connections: vec![],
        nodes: HashMap::new(),
        edges: vec![],
        variables: Some(declarations),
    };
    let registry: NodeRegistry = standard_registry();
    let cm = shared_connection_manager();
    // WorkflowDeployment 未实现 Debug，不能用 unwrap_err / expect_err；用 let…else 提取错误值
    let Err(err) =
        deploy_workflow_with_ai(graph, cm, None, &registry, None, RuntimeResources::new()).await
    else {
        panic!("初值类型不匹配应阻止部署");
    };
    let msg = err.to_string();
    assert!(
        msg.contains("初值类型不匹配") || msg.contains("VariableInitialMismatch"),
        "错误消息应指出 variable initial mismatch，实际：{msg}"
    );
}

/// Task 7 E2E：code 节点通过 `vars.get` / `vars.set` 累积工作流变量，跨多次触发独立持有状态。
///
/// 工作流：单节点（code），Rhai 脚本每次执行将 `counter` 变量加 1，
/// 并将新值写入 `payload.value`。触发三次后 `counter` 应为 3。
#[tokio::test]
async fn rhai_code_节点同部署多次触发累积变量() {
    // counter = 0；code 节点每次触发 counter += 1，把结果放进 payload.value
    let graph = WorkflowGraph::from_json(
        &json!({
            "nodes": {
                "inc": {
                    "type": "code",
                    "config": {
                        // 先读出旧值（Integer），加 1 后写回，再写入 payload.value 返回
                        "script": "let v = vars.get(\"counter\"); let nv = v + 1; vars.set(\"counter\", nv); payload.value = nv; payload",
                        "max_operations": 10000
                    }
                }
            },
            "edges": [],
            "variables": {
                "counter": {
                    "type": {"kind": "integer"},
                    "initial": 0
                }
            }
        })
        .to_string(),
    )
    .expect("含 code 节点 + 变量声明的图应能解析");

    let registry: NodeRegistry = standard_registry();
    let cm = shared_connection_manager();
    let mut deployment =
        deploy_workflow_with_ai(graph, cm, None, &registry, None, RuntimeResources::new())
            .await
            .expect("含 code 节点的图应能部署");

    // 触发三次，每次 counter += 1
    for _ in 0..3 {
        deployment
            .submit(WorkflowContext::new(json!({ "value": 0 })))
            .await
            .expect("submit 应成功");
    }

    // 收三次 result，最后一次 value 应为 3（三次累加）
    let mut last_value: Option<serde_json::Value> = None;
    for _ in 0..3 {
        let result = timeout(Duration::from_secs(2), deployment.next_result())
            .await
            .expect("result 应在超时内到达");
        let ctx = result.expect("next_result 应返回 Some，result channel 不应在 shutdown 前关闭");
        last_value = Some(ctx.payload);
    }
    let final_payload = last_value.expect("应收到 3 次 result");
    assert_eq!(
        final_payload["value"],
        json!(3_i64),
        "三次累加后 counter 应为 3，实际：{final_payload}"
    );

    deployment.shutdown().await;
}

#[tokio::test]
async fn 部署后写变量触发_variablechanged_事件() {
    let mut declarations = HashMap::new();
    declarations.insert(
        "setpoint".to_owned(),
        VariableDeclaration {
            variable_type: PinType::Float,
            initial: json!(25.0),
        },
    );
    let graph = WorkflowGraph {
        name: Some("vars-event-test".to_owned()),
        connections: vec![],
        nodes: HashMap::new(),
        edges: vec![],
        variables: Some(declarations),
    };

    let registry: NodeRegistry = standard_registry();
    let cm = shared_connection_manager();
    let mut deployment =
        deploy_workflow_with_ai(graph, cm, None, &registry, None, RuntimeResources::new())
            .await
            .expect("空 DAG + 单变量应能部署");

    // 从 deployment 的 SharedResources 取 vars，写一次新值
    let vars = deployment
        .resources()
        .get::<Arc<WorkflowVariables>>()
        .expect("应注入 WorkflowVariables");
    vars.set("setpoint", json!(42.0), Some("test"))
        .expect("写入应成功");

    // 期望在 1 秒内收到 WorkflowVariableEvent::Changed；空 DAG 几乎无干扰事件。
    let received_change = timeout(Duration::from_secs(1), async {
        loop {
            match deployment.next_var_event().await {
                Some(WorkflowVariableEvent::Changed {
                    workflow_id,
                    name,
                    value,
                    updated_by,
                    ..
                }) => {
                    assert_eq!(workflow_id, "vars-event-test");
                    assert_eq!(name, "setpoint");
                    assert_eq!(value, json!(42.0));
                    assert_eq!(updated_by.as_deref(), Some("test"));
                    return true;
                }
                Some(_) => {}
                None => return false,
            }
        }
    })
    .await;
    assert!(
        matches!(received_change, Ok(true)),
        "未在 1s 内收到 WorkflowVariableEvent::Changed"
    );

    deployment.shutdown().await;
}
