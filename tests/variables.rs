//! 端到端：变量声明在部署期初始化、注入 `NodeLifecycleContext` 与 `SharedResources`（ADR-0012 Task 5）。

#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::collections::HashMap;
use std::sync::Arc;

use nazh_engine::{
    ConnectionManager, NodeRegistry, PinType, VariableDeclaration, WorkflowGraph,
    deploy_workflow_with_ai, standard_registry,
};
use serde_json::json;

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
    let cm = Arc::new(ConnectionManager::default());
    let deployment = deploy_workflow_with_ai(graph, cm, None, &registry)
        .await
        .expect("空 DAG + 单变量应能部署");

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
    let cm = Arc::new(ConnectionManager::default());
    // WorkflowDeployment 未实现 Debug，不能用 unwrap_err / expect_err；用 let…else 提取错误值
    let Err(err) = deploy_workflow_with_ai(graph, cm, None, &registry).await else {
        panic!("初值类型不匹配应阻止部署");
    };
    let msg = err.to_string();
    assert!(
        msg.contains("初值类型不匹配") || msg.contains("VariableInitialMismatch"),
        "错误消息应指出 variable initial mismatch，实际：{msg}"
    );
}
