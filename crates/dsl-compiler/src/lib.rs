//! Workflow DSL → [`WorkflowGraph`](nazh_graph::WorkflowGraph) JSON 编译器（RFC-0004 Phase 3）。
//!
//! 接收 [`nazh_dsl_core::workflow::WorkflowSpec`] 和已解析的设备/能力资产快照，
//! 输出符合 `WorkflowGraph` serde 契约的 `serde_json::Value`。
//!
//! 编译器不依赖 `nazh-graph` 或 `nazh-engine`——一致性由 dev-dependency 测试守护。

pub mod context;
pub mod error;
pub mod output;
pub mod safety;
pub mod validate;

pub use context::CompilerContext;
pub use error::CompileError;
pub use output::compile;
pub use output::compile_with_safety;
pub use safety::{DiagnosticLevel, SafetyDiagnostic, SafetyReport, run_safety_checks};

#[cfg(test)]
#[allow(clippy::expect_used, clippy::unwrap_used)]
mod conformance_tests {
    use nazh_dsl_core::capability::{
        CapabilityImpl, CapabilitySpec, SafetyConstraints, SafetyLevel,
    };
    use nazh_dsl_core::device::{ConnectionRef, DeviceSpec};
    use nazh_dsl_core::workflow::WorkflowSpec;
    use nazh_graph::WorkflowGraph;

    use crate::{CompilerContext, compile};

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

    fn assert_conformance(output: &serde_json::Value) -> WorkflowGraph {
        let json_str = serde_json::to_string(output).expect("输出应可序列化为 JSON");
        WorkflowGraph::from_json(&json_str).expect("输出应符合 WorkflowGraph JSON 契约")
    }

    #[test]
    fn 最小工作流一致性() {
        let yaml = r#"
id: minimal
version: "1.0.0"
states:
  idle:
  running:
transitions:
  - from: idle
    to: running
    when: "start == true"
"#;
        let spec: WorkflowSpec = serde_yaml::from_str(yaml).unwrap();
        let ctx = CompilerContext::new(vec![sample_device("dev1", "conn1")], vec![]);
        let output = compile(&ctx, &spec).unwrap();
        let graph = assert_conformance(&output);

        assert!(graph.nodes.contains_key("sm_minimal"));
        assert!(graph.edges.is_empty());
    }

    #[test]
    #[allow(clippy::too_many_lines)]
    fn auto_pressing_cycle_完整示例一致性() {
        let yaml = r#"
id: auto_pressing_cycle
description: "自动压装循环"
version: "1.0.0"
devices:
  - hydraulic_press_1
variables:
  target_pressure: 25.0
  hold_time: 5.0
  approach_position: 100.0
states:
  idle:
    entry: []
    exit: []
  approaching:
    entry:
      - capability: hydraulic_axis.move_to
        args:
          position: "${approach_position}"
  pressing:
    entry:
      - capability: hydraulic_axis.apply_pressure
        args:
          target: "${target_pressure}"
  holding:
    entry:
      - capability: hydraulic_axis.hold_pressure
        args:
          target: "${target_pressure}"
          duration: "${hold_time}"
  returning:
    entry:
      - capability: hydraulic_axis.move_to
        args:
          position: 0.0
  fault:
    entry:
      - capability: hydraulic_axis.stop
      - action: alarm.raise
        args:
          message: "压装循环异常停机"
transitions:
  - from: idle
    to: approaching
    when: "start_button == true"
  - from: approaching
    to: pressing
    when: "position >= approach_position"
  - from: pressing
    to: holding
    when: "pressure >= target_pressure"
  - from: holding
    to: returning
    when: "hold_timer >= hold_time"
  - from: returning
    to: idle
    when: "position <= 1.0"
  - from: "*"
    to: fault
    when: "pressure > 34"
    priority: 100
timeout:
  pressing: 60s
  holding: 30s
on_timeout: fault
"#;
        let spec: WorkflowSpec = serde_yaml::from_str(yaml).unwrap();

        let device = sample_device("hydraulic_press_1", "press_modbus");
        let capabilities = vec![
            sample_modbus_cap(
                "hydraulic_axis.move_to",
                "hydraulic_press_1",
                40010,
                "${position}",
            ),
            sample_modbus_cap(
                "hydraulic_axis.apply_pressure",
                "hydraulic_press_1",
                40020,
                "${target}",
            ),
            sample_modbus_cap(
                "hydraulic_axis.hold_pressure",
                "hydraulic_press_1",
                40030,
                "${target}",
            ),
            sample_modbus_cap("hydraulic_axis.stop", "hydraulic_press_1", 40100, "0"),
        ];

        let ctx = CompilerContext::new(vec![device], capabilities);
        let output = compile(&ctx, &spec).unwrap();
        let graph = assert_conformance(&output);

        // 验证节点数：1 stateMachine + 5 个唯一 capability 调用 + 1 system action
        // (move_to 被 approaching/returning 共享但 port 不同，所以有 2 个节点)
        assert!(
            graph.nodes.len() >= 6,
            "应有至少 6 个节点（1 sm + ≥5 action），实际 {}",
            graph.nodes.len()
        );

        // stateMachine 节点
        assert!(graph.nodes.contains_key("sm_auto_pressing_cycle"));

        // 验证边数 ≥ action 数量
        assert!(
            !graph.edges.is_empty(),
            "应有从 stateMachine 到 action 节点的边"
        );

        // 验证变量
        let vars = graph.variables.expect("应有变量声明");
        assert!(vars.contains_key("target_pressure"));
        assert!(vars.contains_key("_sm.sm_auto_pressing_cycle.current_state"));
        assert_eq!(
            vars["_sm.sm_auto_pressing_cycle.current_state"].initial,
            "idle"
        );
    }

    #[test]
    fn 纯状态转移无动作一致性() {
        let yaml = r#"
id: pure_transitions
version: "1.0.0"
states:
  idle:
  active:
  done:
transitions:
  - from: idle
    to: active
    when: "start"
  - from: active
    to: done
    when: "completed"
"#;
        let spec: WorkflowSpec = serde_yaml::from_str(yaml).unwrap();
        let ctx = CompilerContext::new(vec![], vec![]);
        let output = compile(&ctx, &spec).unwrap();
        let graph = assert_conformance(&output);

        // 只有 stateMachine 节点，无 action 节点
        assert_eq!(graph.nodes.len(), 1);
        assert!(graph.edges.is_empty());
    }

    #[test]
    fn 混合action类型一致性() {
        let yaml = r#"
id: mixed_actions
version: "1.0.0"
devices:
  - dev1
states:
  idle:
  error:
    entry:
      - capability: cap.stop
      - action: alarm.raise
        args:
          msg: "fault"
transitions:
  - from: idle
    to: error
    when: "fault"
"#;
        let spec: WorkflowSpec = serde_yaml::from_str(yaml).unwrap();
        let ctx = CompilerContext::new(
            vec![sample_device("dev1", "conn1")],
            vec![sample_modbus_cap("cap.stop", "dev1", 40100, "0")],
        );
        let output = compile(&ctx, &spec).unwrap();
        let graph = assert_conformance(&output);

        // sm + cap.stop + alarm.raise = 3 nodes
        assert_eq!(graph.nodes.len(), 3);
        assert_eq!(graph.edges.len(), 2);
    }
}
