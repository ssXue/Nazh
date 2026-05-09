use super::*;
use nazh_dsl_core::capability::{CapabilityImpl, CapabilitySpec, SafetyConstraints, SafetyLevel};
use nazh_dsl_core::device::{ConnectionRef, DeviceSpec};
use nazh_dsl_core::workflow::WorkflowSpec;

fn sample_device(id: &str, conn_id: &str) -> DeviceSpec {
    DeviceSpec {
        id: id.to_owned(),
        device_type: "test".to_owned(),
        manufacturer: None,
        model: None,
        connection: Some(ConnectionRef {
            connection_type: "modbus-tcp".to_owned(),
            id: conn_id.to_owned(),
            unit: Some(1),
        }),
        network_group: None,
        signals: vec![],
        alarms: vec![],
    }
}

fn sample_capability_modbus(id: &str, device_id: &str, register: u16) -> CapabilitySpec {
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
            value: "${value}".to_owned(),
        },
        fallback: vec![],
        safety: SafetyConstraints {
            level: SafetyLevel::Low,
            requires_approval: false,
            max_execution_time: None,
        },
    }
}

fn sample_capability_script(id: &str, device_id: &str) -> CapabilitySpec {
    CapabilitySpec {
        id: id.to_owned(),
        device_id: device_id.to_owned(),
        description: String::new(),
        inputs: vec![],
        outputs: vec![],
        preconditions: vec![],
        effects: vec![],
        implementation: CapabilityImpl::Script {
            content: "pass".to_owned(),
        },
        fallback: vec![],
        safety: SafetyConstraints {
            level: SafetyLevel::Low,
            requires_approval: false,
            max_execution_time: None,
        },
    }
}

#[test]
fn 最小工作流_编译成功() {
    let yaml = r#"
id: minimal
version: "1.0.0"
devices:
  - dev1
states:
  idle:
  running:
transitions:
  - from: idle
    to: running
    when: "payload.start == true"
"#;
    let spec: WorkflowSpec = serde_yaml::from_str(yaml).unwrap();
    let ctx = CompilerContext::new(vec![sample_device("dev1", "conn1")], vec![]);
    let output = compile(&ctx, &spec).unwrap();

    // 验证基本结构
    assert_eq!(output["name"], "minimal");
    assert!(output["connections"].as_array().is_some_and(Vec::is_empty));
    assert!(output["nodes"].is_object());
    assert!(output["edges"].is_array());
    assert!(output["variables"].is_object());

    // 只有 stateMachine 节点（无 action）
    assert!(
        output["nodes"]
            .as_object()
            .unwrap()
            .contains_key("sm_minimal")
    );
    // 无边（没有 action）
    assert!(output["edges"].as_array().unwrap().is_empty());
    // 有内部状态变量
    let vars = output["variables"].as_object().unwrap();
    assert!(vars.contains_key("_sm.sm_minimal.current_state"));
    assert_eq!(vars["_sm.sm_minimal.current_state"]["initial"], "idle");
}

#[test]
fn 带capability调用的工作流_编译成功() {
    let yaml = r#"
id: test_wf
version: "1.0.0"
devices:
  - dev1
variables:
  target_pressure: 25.0
  mode: "auto"
states:
  idle:
  pressing:
    entry:
      - capability: cap.press
        args:
          target: "${target_pressure}"
transitions:
  - from: idle
    to: pressing
    when: "payload.start == true"
"#;
    let spec: WorkflowSpec = serde_yaml::from_str(yaml).unwrap();
    let ctx = CompilerContext::new(
        vec![sample_device("dev1", "conn1")],
        vec![sample_capability_modbus("cap.press", "dev1", 40010)],
    );
    let output = compile(&ctx, &spec).unwrap();

    // stateMachine 节点
    let nodes = output["nodes"].as_object().unwrap();
    assert!(nodes.contains_key("sm_test_wf"));

    // capabilityCall 节点
    let cap_node_key = nodes
        .keys()
        .find(|k| k.starts_with("cap_cap_press"))
        .expect("应有 capabilityCall 节点");
    let cap_node = &nodes[cap_node_key];
    assert_eq!(cap_node["type"], "capabilityCall");
    assert_eq!(cap_node["connection_id"], "conn1");
    assert_eq!(cap_node["config"]["capability_id"], "cap.press");
    assert_eq!(cap_node["config"]["implementation"]["type"], "modbus-write");

    // 边
    let edges = output["edges"].as_array().unwrap();
    assert_eq!(edges.len(), 1);
    assert_eq!(edges[0]["from"], "sm_test_wf");
    assert_eq!(edges[0]["source_port_id"], "entry_pressing_0");

    // 用户变量
    let vars = output["variables"].as_object().unwrap();
    assert_eq!(vars["target_pressure"]["type"]["kind"], "float");
    assert_eq!(vars["target_pressure"]["initial"], 25.0);
    assert_eq!(vars["mode"]["type"]["kind"], "string");
    assert_eq!(vars["mode"]["initial"], "auto");
}

#[test]
fn 同一capability多次调用_保留各自动作参数() {
    let yaml = r#"
id: repeated_capability_args
version: "1.0.0"
devices:
  - dev1
variables:
  approach_position: 100.0
states:
  idle:
  approaching:
    entry:
      - capability: cap.move_to
        args:
          position: "${approach_position}"
  returning:
    entry:
      - capability: cap.move_to
        args:
          position: 0.0
transitions:
  - from: idle
    to: approaching
    when: "payload.start == true"
  - from: approaching
    to: returning
    when: "payload.done == true"
"#;
    let spec: WorkflowSpec = serde_yaml::from_str(yaml).unwrap();
    let ctx = CompilerContext::new(
        vec![sample_device("dev1", "conn1")],
        vec![sample_capability_modbus("cap.move_to", "dev1", 40010)],
    );
    let output = compile(&ctx, &spec).unwrap();
    let nodes = output["nodes"].as_object().unwrap();

    let approaching = nodes
        .get("cap_cap_move_to_entry_approaching_0")
        .expect("应生成 approaching entry 节点");
    let returning = nodes
        .get("cap_cap_move_to_entry_returning_0")
        .expect("应生成 returning entry 节点");

    assert_eq!(
        approaching["config"]["args"]["position"],
        "${approach_position}"
    );
    assert_eq!(returning["config"]["args"]["position"], 0.0);
}

#[test]
fn sanitize后端口id碰撞_编译期报错并列出原始状态() {
    let yaml = r#"
id: collision
version: "1.0.0"
devices:
  - dev1
states:
  a.b:
    entry:
      - capability: cap.move
  a_b:
    entry:
      - capability: cap.move
"#;
    let spec: WorkflowSpec = serde_yaml::from_str(yaml).unwrap();
    let ctx = CompilerContext::new(
        vec![sample_device("dev1", "conn1")],
        vec![sample_capability_script("cap.move", "dev1")],
    );

    let err = compile(&ctx, &spec).expect_err("sanitize 后端口 ID 碰撞应拒绝编译");
    let msg = err.to_string();
    assert!(msg.contains("entry_a_b_0"));
    assert!(msg.contains("a.b"));
    assert!(msg.contains("a_b"));
}

#[test]
fn compile_with_safety_失败时保留完整诊断报告() {
    let yaml = r#"
id: safety_error
version: "1.0.0"
devices:
  - dev1
states:
  idle:
    entry:
      - capability: cap.move
        args:
          position: 200.0
"#;
    let spec: WorkflowSpec = serde_yaml::from_str(yaml).unwrap();
    let mut cap = sample_capability_modbus("cap.move", "dev1", 40010);
    cap.inputs = vec![nazh_dsl_core::capability::CapabilityParam {
        id: "position".to_owned(),
        unit: Some("mm".to_owned()),
        range: Some(nazh_dsl_core::workflow::Range {
            min: 0.0,
            max: 100.0,
        }),
        required: true,
    }];
    let ctx = CompilerContext::new(vec![sample_device("dev1", "conn1")], vec![cap]);

    let err = compile_with_safety(&ctx, &spec).expect_err("越界参数应阻止安全编译");
    match err {
        CompileError::Safety { report } => {
            assert!(report.has_errors());
            assert!(
                report
                    .errors()
                    .any(|d| d.rule == "range_boundary" && d.message.contains("200"))
            );
        }
        other => panic!("应返回完整 SafetyReport，实际: {other:?}"),
    }
}

#[test]
fn timeout未实现时_编译期拒绝() {
    let yaml = r#"
id: timeout_not_supported
version: "1.0.0"
states:
  idle:
  pressing:
  fault:
transitions:
  - from: idle
    to: pressing
    when: "payload.start == true"
timeout:
  pressing: 60s
on_timeout: fault
"#;
    let spec: WorkflowSpec = serde_yaml::from_str(yaml).unwrap();
    let ctx = CompilerContext::new(vec![], vec![]);

    let err = compile(&ctx, &spec).expect_err("timeout 运行时未实现时应拒绝编译");

    assert!(err.to_string().contains("timeout"));
}

#[test]
fn 裸变量条件_编译期拒绝() {
    let yaml = r#"
id: bare_condition
version: "1.0.0"
states:
  idle:
  running:
transitions:
  - from: idle
    to: running
    when: "start_button == true"
"#;
    let spec: WorkflowSpec = serde_yaml::from_str(yaml).unwrap();
    let ctx = CompilerContext::new(vec![], vec![]);

    let err = compile(&ctx, &spec).expect_err("裸变量条件应在编译期被拒绝");

    assert!(err.to_string().contains("payload"));
}

#[test]
fn system_action未实现时_编译期拒绝() {
    let yaml = r#"
id: system_action_not_supported
version: "1.0.0"
states:
  idle:
  fault:
    entry:
      - action: alarm.raise
        args:
          msg: "error"
transitions:
  - from: idle
    to: fault
    when: "payload.fault == true"
"#;
    let spec: WorkflowSpec = serde_yaml::from_str(yaml).unwrap();
    let ctx = CompilerContext::new(vec![], vec![]);

    let err = compile(&ctx, &spec).expect_err("system action 未实现时应拒绝编译");

    assert!(err.to_string().contains("alarm.raise"));
}

#[test]
fn 混合capability和system_action调用_拒绝未实现动作() {
    let yaml = r#"
id: mixed
version: "1.0.0"
devices:
  - dev1
states:
  idle:
  fault:
    entry:
      - capability: cap.stop
      - action: alarm.raise
        args:
          msg: "error"
transitions:
  - from: idle
    to: fault
    when: "true"
"#;
    let spec: WorkflowSpec = serde_yaml::from_str(yaml).unwrap();
    let ctx = CompilerContext::new(
        vec![sample_device("dev1", "conn1")],
        vec![sample_capability_script("cap.stop", "dev1")],
    );
    let err = compile(&ctx, &spec).expect_err("system action 未实现时应拒绝编译");

    assert!(err.to_string().contains("alarm.raise"));
}

#[test]
fn 变量类型推断() {
    let yaml = r#"
id: types_test
version: "1.0.0"
variables:
  float_var: 3.14
  int_var: 42
  str_var: "hello"
  bool_var: true
states:
  idle:
"#;
    let spec: WorkflowSpec = serde_yaml::from_str(yaml).unwrap();
    let ctx = CompilerContext::new(vec![], vec![]);
    let output = compile(&ctx, &spec).unwrap();

    let vars = output["variables"].as_object().unwrap();
    assert_eq!(vars["float_var"]["type"]["kind"], "float");
    assert_eq!(vars["int_var"]["type"]["kind"], "integer");
    assert_eq!(vars["str_var"]["type"]["kind"], "string");
    assert_eq!(vars["bool_var"]["type"]["kind"], "bool");
}

#[test]
fn sanitize_node_id_替换特殊字符() {
    assert_eq!(sanitize_node_id("a.b-c d"), "a_b_c_d");
}
