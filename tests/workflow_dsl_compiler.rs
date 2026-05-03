//! RFC-0004 Phase 3 集成测试：Workflow DSL 编译 → 部署 → 执行全链路。
//!
//! 验证编译器输出的 JSON 能被 `deploy_workflow` 正常部署，
//! stateMachine 节点能评估 transition 条件并路由到 capabilityCall 节点。

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::sync::Arc;
use std::time::Duration;

use dsl_compiler::{CompileError, CompilerContext, compile};
use nazh_core::WorkflowVariables;
use nazh_dsl_core::capability::{CapabilityImpl, CapabilitySpec, SafetyConstraints, SafetyLevel};
use nazh_dsl_core::device::{ConnectionRef, DeviceSpec};
use nazh_dsl_core::workflow::WorkflowSpec;
use nazh_engine::{WorkflowGraph, deploy_workflow, shared_connection_manager, standard_registry};
use serde_json::json;
use tokio::time::timeout;

fn sample_device(id: &str, conn_id: &str) -> DeviceSpec {
    DeviceSpec {
        id: id.to_owned(),
        device_type: "hydraulic_press".to_owned(),
        manufacturer: Some("某液压".to_owned()),
        model: Some("YP-320T".to_owned()),
        connection: ConnectionRef {
            connection_type: "modbus-tcp".to_owned(),
            id: conn_id.to_owned(),
            unit: Some(1),
        },
        signals: vec![],
        alarms: vec![],
    }
}

fn sample_modbus_cap(id: &str, device_id: &str, register: u16, value: &str) -> CapabilitySpec {
    CapabilitySpec {
        id: id.to_owned(),
        device_id: device_id.to_owned(),
        description: String::new(),
        inputs: vec![],
        outputs: vec![],
        preconditions: vec![],
        effects: vec![],
        implementation: CapabilityImpl::ModbusWrite {
            register,
            value: value.to_owned(),
        },
        fallback: vec![],
        safety: SafetyConstraints {
            level: SafetyLevel::Low,
            requires_approval: false,
            max_execution_time: None,
        },
    }
}

/// 从 deployment 的 `SharedResources` 中取出 `WorkflowVariables`。
fn get_vars(deployment: &nazh_engine::WorkflowDeployment) -> Arc<WorkflowVariables> {
    deployment
        .resources()
        .get::<Arc<WorkflowVariables>>()
        .expect("应注入 WorkflowVariables")
}

/// 最小 2 状态工作流：编译 → 部署 → 无 action 节点执行。
#[tokio::test]
async fn 最小工作流_编译部署执行() {
    let yaml = r#"
id: minimal_test
version: "1.0.0"
states:
  idle:
  running:
transitions:
  - from: idle
    to: running
    when: "payload.start == true"
"#;
    let spec: WorkflowSpec = serde_yaml::from_str(yaml).unwrap();
    let ctx = CompilerContext::new(vec![], vec![]);
    let output = compile(&ctx, &spec).unwrap();

    // 一致性：输出可解析为 WorkflowGraph
    let json_str = serde_json::to_string(&output).unwrap();
    let graph = WorkflowGraph::from_json(&json_str).expect("输出应符合 WorkflowGraph JSON 契约");

    // 部署
    let registry = standard_registry();
    let cm = shared_connection_manager();
    let mut deployment = deploy_workflow(graph, cm.clone(), &registry)
        .await
        .expect("最小工作流应部署成功");

    // 初始状态应为 idle
    let vars = get_vars(&deployment);
    let state_key = "_sm.sm_minimal_test.current_state";
    assert_eq!(vars.get_value(state_key).unwrap(), "idle");

    // 发送 payload 触发 idle → running
    deployment
        .submit(nazh_engine::WorkflowContext::new(json!({ "start": true })))
        .await
        .expect("提交 payload 应成功");

    // 等待结果
    let result = timeout(Duration::from_secs(2), deployment.next_result()).await;
    match result {
        Ok(Some(_ctx)) => {}
        Ok(None) => panic!("结果流意外关闭"),
        Err(elapsed) => panic!("工作流未在时限内产生结果: {elapsed}"),
    }

    // 验证状态已转移
    assert_eq!(vars.get_value(state_key).unwrap(), "running");
}

/// 带 capability 调用的工作流：编译 → 部署 → 验证 stateMachine 路由到 capabilityCall。
#[tokio::test]
async fn 带_capability_的工作流_编译部署执行() {
    let yaml = r#"
id: cap_test
version: "1.0.0"
devices:
  - press_1
states:
  idle:
  moving:
    entry:
      - capability: axis.move_to
        args:
          position: 100.0
  done:
transitions:
  - from: idle
    to: moving
    when: "payload.start == true"
  - from: moving
    to: done
    when: "payload.arrived == true"
"#;
    let spec: WorkflowSpec = serde_yaml::from_str(yaml).unwrap();

    let device = sample_device("press_1", "modbus_conn");
    let cap = sample_modbus_cap("axis.move_to", "press_1", 40010, "${position}");

    let ctx = CompilerContext::new(vec![device], vec![cap]);
    let output = compile(&ctx, &spec).unwrap();

    let json_str = serde_json::to_string(&output).unwrap();
    let graph = WorkflowGraph::from_json(&json_str).expect("输出应符合 WorkflowGraph JSON 契约");

    // 验证节点数：1 stateMachine + 1 capabilityCall
    assert!(graph.nodes.len() >= 2, "应有至少 2 个节点");
    assert!(graph.nodes.contains_key("sm_cap_test"));
    assert!(
        !graph.edges.is_empty(),
        "应有从 stateMachine 到 capabilityCall 的边"
    );

    // 部署
    let registry = standard_registry();
    let cm = shared_connection_manager();
    let mut deployment = deploy_workflow(graph, cm.clone(), &registry)
        .await
        .expect("工作流应部署成功");

    let vars = get_vars(&deployment);
    let state_key = "_sm.sm_cap_test.current_state";
    assert_eq!(vars.get_value(state_key).unwrap(), "idle");

    // 触发 idle → moving（会路由到 capabilityCall）
    deployment
        .submit(nazh_engine::WorkflowContext::new(json!({ "start": true })))
        .await
        .expect("提交 payload 应成功");

    // 等待结果流
    let result = timeout(Duration::from_secs(2), deployment.next_result()).await;
    match result {
        Ok(Some(_ctx)) => {}
        Ok(None) => panic!("结果流意外关闭"),
        Err(elapsed) => panic!("工作流未在时限内产生结果: {elapsed}"),
    }

    // 状态应转移到 moving
    assert_eq!(vars.get_value(state_key).unwrap(), "moving");

    // 再触发 moving → done
    deployment
        .submit(nazh_engine::WorkflowContext::new(
            json!({ "arrived": true }),
        ))
        .await
        .expect("提交 payload 应成功");

    let result = timeout(Duration::from_secs(2), deployment.next_result()).await;
    assert!(result.is_ok(), "moving → done 应产生结果");

    assert_eq!(vars.get_value(state_key).unwrap(), "done");
}

/// 引用不存在的设备应编译失败。
#[test]
fn 引用不存在设备_编译失败() {
    let yaml = r#"
id: bad_ref
version: "1.0.0"
devices:
  - nonexistent_device
states:
  idle:
transitions: []
"#;
    let spec: WorkflowSpec = serde_yaml::from_str(yaml).unwrap();
    let ctx = CompilerContext::new(vec![], vec![]);
    let err = compile(&ctx, &spec).unwrap_err();
    assert!(
        matches!(err, CompileError::Reference { .. }),
        "应返回 Reference 错误，实际: {err:?}"
    );
}

/// 引用不存在的 capability 应编译失败。
#[test]
fn 引用不存在capability_编译失败() {
    let yaml = r#"
id: bad_cap
version: "1.0.0"
devices:
  - dev1
states:
  idle:
  active:
    entry:
      - capability: nonexistent.cap
        args: {}
transitions:
  - from: idle
    to: active
    when: "true"
"#;
    let spec: WorkflowSpec = serde_yaml::from_str(yaml).unwrap();
    let device = sample_device("dev1", "conn1");
    let ctx = CompilerContext::new(vec![device], vec![]);
    let err = compile(&ctx, &spec).unwrap_err();
    assert!(
        matches!(err, CompileError::Reference { .. }),
        "应返回 Reference 错误，实际: {err:?}"
    );
}
