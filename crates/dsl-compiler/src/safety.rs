//! 安全编译器——Workflow DSL 编译后安全校验（RFC-0004 Phase 5）。
//!
//! 在引用校验和语义校验通过后，对已编译的 `WorkflowSpec` 执行 6 条安全规则，
//! 产出结构化诊断（错误 + 警告），为工业场景提供部署前安全保障。

use std::collections::HashSet;

use nazh_dsl_core::workflow::{ActionSpec, ActionTarget, WorkflowSpec};

use crate::context::CompilerContext;

mod action_rules;
mod interlock;
mod preconditions;
mod report;
mod state_graph;
mod template;

#[cfg(test)]
use preconditions::extract_identifiers;
pub use report::{DiagnosticLevel, SafetyDiagnostic, SafetyReport};
#[cfg(test)]
use template::extract_variable_ref;

// ---- 公共入口 ----

/// 对已通过引用校验和语义校验的 `WorkflowSpec` 执行安全编译器校验。
///
/// 前置条件：`ctx.validate_references(spec)` 和 `validate_workflow_spec(spec)` 均已成功。
///
/// 返回 [`SafetyReport`]，包含所有诊断条目（错误 + 警告）。
/// 调用者应检查 `report.has_errors()` 决定是否继续编译。
pub fn run_safety_checks(
    ctx: &CompilerContext,
    spec: &WorkflowSpec,
    initial_state: &str,
) -> SafetyReport {
    let mut report = SafetyReport::default();

    action_rules::check_unit_consistency(ctx, spec, &mut report);
    action_rules::check_range_boundary(ctx, spec, &mut report);
    preconditions::check_precondition_reachability(ctx, spec, &mut report);
    state_graph::check_state_machine_completeness(spec, initial_state, &mut report);
    action_rules::check_dangerous_action_approval(ctx, spec, &mut report);
    interlock::check_mechanical_interlock(ctx, spec, &mut report);

    report
}

// ---- 共享 helper ----

/// 从 action 列表中提取 Capability ID。
fn collect_capability_ids(actions: &[ActionSpec], out: &mut HashSet<String>) {
    for action in actions {
        if let ActionTarget::Capability(id) = &action.target {
            out.insert(id.clone());
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use nazh_dsl_core::capability::{
        CapabilityImpl, CapabilityParam, CapabilitySpec, SafetyConstraints, SafetyLevel,
    };
    use nazh_dsl_core::device::{ConnectionRef, DeviceSpec, SignalSource, SignalSpec, SignalType};
    use nazh_dsl_core::workflow::{Range, WorkflowSpec};

    use super::*;
    use crate::CompilerContext;

    // ---- 测试辅助 ----

    fn sample_device_with_signals(id: &str, signals: Vec<SignalSpec>) -> DeviceSpec {
        DeviceSpec {
            id: id.to_owned(),
            device_type: "test".to_owned(),
            manufacturer: None,
            model: None,
            connection: Some(ConnectionRef {
                connection_type: "modbus-tcp".to_owned(),
                id: format!("{id}_conn"),
                unit: Some(1),
            }),
            network_group: None,
            signals,
            alarms: vec![],
        }
    }

    fn sample_device(id: &str) -> DeviceSpec {
        sample_device_with_signals(id, vec![])
    }

    fn readable_signal(id: &str) -> SignalSpec {
        SignalSpec {
            id: id.to_owned(),
            signal_type: SignalType::AnalogInput,
            unit: Some("MPa".to_owned()),
            range: Some(Range {
                min: 0.0,
                max: 35.0,
            }),
            source: SignalSource::Register {
                register: 40001,
                access: nazh_dsl_core::device::AccessMode::Read,
                data_type: nazh_dsl_core::device::DataType::Float32,
                bit: None,
            },
            scale: None,
        }
    }

    fn writable_signal(id: &str) -> SignalSpec {
        SignalSpec {
            id: id.to_owned(),
            signal_type: SignalType::AnalogOutput,
            unit: Some("mm".to_owned()),
            range: Some(Range {
                min: 0.0,
                max: 150.0,
            }),
            source: SignalSource::Register {
                register: 40010,
                access: nazh_dsl_core::device::AccessMode::Write,
                data_type: nazh_dsl_core::device::DataType::Float32,
                bit: None,
            },
            scale: None,
        }
    }

    fn cap_with_input(
        id: &str,
        device_id: &str,
        input_id: &str,
        unit: Option<&str>,
        range: Option<Range>,
        register: u16,
    ) -> CapabilitySpec {
        CapabilitySpec {
            id: id.to_owned(),
            device_id: device_id.to_owned(),
            description: String::new(),
            inputs: vec![CapabilityParam {
                id: input_id.to_owned(),
                unit: unit.map(String::from),
                range,
                required: true,
            }],
            outputs: vec![],
            preconditions: vec![],
            effects: vec![],
            implementation: CapabilityImpl::ModbusWrite {
                register,
                value: format!("${{{input_id}}}"),
            },
            fallback: vec![],
            safety: SafetyConstraints {
                level: SafetyLevel::Low,
                requires_approval: false,
                max_execution_time: None,
            },
        }
    }

    fn parse_spec(yaml: &str) -> WorkflowSpec {
        serde_yaml::from_str(yaml).unwrap()
    }

    // ---- 规则 1: 单位一致性 ----

    #[test]
    fn 单位一致性_数值字面量产生警告() {
        let cap = cap_with_input("cap.move", "dev1", "position", Some("mm"), None, 40010);
        let yaml = r#"
id: test
version: "1.0.0"
devices: [dev1]
states:
  idle:
    entry:
      - capability: cap.move
        args:
          position: 100.0
transitions: []
"#;
        let spec = parse_spec(yaml);
        let ctx = CompilerContext::new(vec![sample_device("dev1")], vec![cap]);
        let report = run_safety_checks(&ctx, &spec, "idle");
        let unit_warnings: Vec<_> = report
            .warnings()
            .filter(|d| d.rule == "unit_consistency")
            .collect();
        assert_eq!(unit_warnings.len(), 1);
        assert!(unit_warnings[0].message.contains("无法静态校验单位"));
    }

    #[test]
    fn 单位一致性_变量引用产生警告() {
        let cap = cap_with_input("cap.move", "dev1", "position", Some("mm"), None, 40010);
        let yaml = r#"
id: test
version: "1.0.0"
devices: [dev1]
variables:
  pos: 100.0
states:
  idle:
    entry:
      - capability: cap.move
        args:
          position: "${pos}"
transitions: []
"#;
        let spec = parse_spec(yaml);
        let ctx = CompilerContext::new(vec![sample_device("dev1")], vec![cap]);
        let report = run_safety_checks(&ctx, &spec, "idle");
        let unit_warnings: Vec<_> = report
            .warnings()
            .filter(|d| d.rule == "unit_consistency")
            .collect();
        assert_eq!(unit_warnings.len(), 1);
        assert!(unit_warnings[0].message.contains("pos"));
    }

    #[test]
    fn 单位一致性_无单位参数不产生诊断() {
        let cap = cap_with_input("cap.move", "dev1", "position", None, None, 40010);
        let yaml = r#"
id: test
version: "1.0.0"
devices: [dev1]
states:
  idle:
    entry:
      - capability: cap.move
        args:
          position: 100.0
transitions: []
"#;
        let spec = parse_spec(yaml);
        let ctx = CompilerContext::new(vec![sample_device("dev1")], vec![cap]);
        let report = run_safety_checks(&ctx, &spec, "idle");
        assert!(
            report
                .diagnostics
                .iter()
                .all(|d| d.rule != "unit_consistency")
        );
    }

    #[test]
    fn 单位一致性_system_action不检查() {
        let yaml = r#"
id: test
version: "1.0.0"
states:
  idle:
    entry:
      - action: alarm.raise
        args:
          msg: "error"
transitions: []
"#;
        let spec = parse_spec(yaml);
        let ctx = CompilerContext::new(vec![], vec![]);
        let report = run_safety_checks(&ctx, &spec, "idle");
        assert!(
            report
                .diagnostics
                .iter()
                .all(|d| d.rule != "unit_consistency")
        );
    }

    // ---- 规则 2: 量程边界 ----

    #[test]
    fn 量程边界_字面量在范围内通过() {
        let cap = cap_with_input(
            "cap.move",
            "dev1",
            "position",
            Some("mm"),
            Some(Range {
                min: 0.0,
                max: 150.0,
            }),
            40010,
        );
        let yaml = r#"
id: test
version: "1.0.0"
devices: [dev1]
states:
  idle:
    entry:
      - capability: cap.move
        args:
          position: 100.0
transitions: []
"#;
        let spec = parse_spec(yaml);
        let ctx = CompilerContext::new(vec![sample_device("dev1")], vec![cap]);
        let report = run_safety_checks(&ctx, &spec, "idle");
        assert!(!report.has_errors());
    }

    #[test]
    fn 量程边界_字面量越界报错() {
        let cap = cap_with_input(
            "cap.move",
            "dev1",
            "position",
            Some("mm"),
            Some(Range {
                min: 0.0,
                max: 150.0,
            }),
            40010,
        );
        let yaml = r#"
id: test
version: "1.0.0"
devices: [dev1]
states:
  idle:
    entry:
      - capability: cap.move
        args:
          position: 200.0
transitions: []
"#;
        let spec = parse_spec(yaml);
        let ctx = CompilerContext::new(vec![sample_device("dev1")], vec![cap]);
        let report = run_safety_checks(&ctx, &spec, "idle");
        let range_errors: Vec<_> = report
            .errors()
            .filter(|d| d.rule == "range_boundary")
            .collect();
        assert_eq!(range_errors.len(), 1);
        assert!(range_errors[0].message.contains("超出"));
    }

    #[test]
    fn 量程边界_变量初始值越界报错() {
        let cap = cap_with_input(
            "cap.move",
            "dev1",
            "position",
            Some("mm"),
            Some(Range {
                min: 0.0,
                max: 150.0,
            }),
            40010,
        );
        let yaml = r#"
id: test
version: "1.0.0"
devices: [dev1]
variables:
  target_pos: 200.0
states:
  idle:
    entry:
      - capability: cap.move
        args:
          position: "${target_pos}"
transitions: []
"#;
        let spec = parse_spec(yaml);
        let ctx = CompilerContext::new(vec![sample_device("dev1")], vec![cap]);
        let report = run_safety_checks(&ctx, &spec, "idle");
        let range_errors: Vec<_> = report
            .errors()
            .filter(|d| d.rule == "range_boundary")
            .collect();
        assert_eq!(range_errors.len(), 1);
        assert!(range_errors[0].message.contains("target_pos"));
    }

    #[test]
    fn 量程边界_未声明变量产生警告() {
        let cap = cap_with_input(
            "cap.move",
            "dev1",
            "position",
            Some("mm"),
            Some(Range {
                min: 0.0,
                max: 150.0,
            }),
            40010,
        );
        let yaml = r#"
id: test
version: "1.0.0"
devices: [dev1]
states:
  idle:
    entry:
      - capability: cap.move
        args:
          position: "${unknown_var}"
transitions: []
"#;
        let spec = parse_spec(yaml);
        let ctx = CompilerContext::new(vec![sample_device("dev1")], vec![cap]);
        let report = run_safety_checks(&ctx, &spec, "idle");
        let range_warnings: Vec<_> = report
            .warnings()
            .filter(|d| d.rule == "range_boundary")
            .collect();
        assert_eq!(range_warnings.len(), 1);
        assert!(range_warnings[0].message.contains("unknown_var"));
    }

    // ---- 规则 3: 前置条件可达性 ----

    #[test]
    fn 前置条件_可读信号通过() {
        let mut cap = cap_with_input("cap.move", "dev1", "position", None, None, 40010);
        cap.preconditions = vec!["pressure < 32".to_owned()];

        let device = sample_device_with_signals("dev1", vec![readable_signal("pressure")]);
        let yaml = r#"
id: test
version: "1.0.0"
devices: [dev1]
states:
  idle:
    entry:
      - capability: cap.move
transitions: []
"#;
        let spec = parse_spec(yaml);
        let ctx = CompilerContext::new(vec![device], vec![cap]);
        let report = run_safety_checks(&ctx, &spec, "idle");
        assert!(
            report
                .diagnostics
                .iter()
                .all(|d| d.rule != "precondition_reachability")
        );
    }

    #[test]
    fn 前置条件_信号不存在产生警告() {
        let mut cap = cap_with_input("cap.move", "dev1", "position", None, None, 40010);
        cap.preconditions = vec!["nonexistent_signal > 10".to_owned()];

        let device = sample_device_with_signals("dev1", vec![readable_signal("pressure")]);
        let yaml = r#"
id: test
version: "1.0.0"
devices: [dev1]
states:
  idle:
    entry:
      - capability: cap.move
transitions: []
"#;
        let spec = parse_spec(yaml);
        let ctx = CompilerContext::new(vec![device], vec![cap]);
        let report = run_safety_checks(&ctx, &spec, "idle");
        let prec_warnings: Vec<_> = report
            .diagnostics
            .iter()
            .filter(|d| d.rule == "precondition_reachability")
            .collect();
        assert_eq!(prec_warnings.len(), 1);
        assert!(prec_warnings[0].message.contains("nonexistent_signal"));
    }

    #[test]
    fn 前置条件_不可写信号报错() {
        let mut cap = cap_with_input("cap.move", "dev1", "position", None, None, 40010);
        cap.preconditions = vec!["target_position > 0".to_owned()];

        // target_position 是 AnalogOutput（只写）
        let device = sample_device_with_signals("dev1", vec![writable_signal("target_position")]);
        let yaml = r#"
id: test
version: "1.0.0"
devices: [dev1]
states:
  idle:
    entry:
      - capability: cap.move
transitions: []
"#;
        let spec = parse_spec(yaml);
        let ctx = CompilerContext::new(vec![device], vec![cap]);
        let report = run_safety_checks(&ctx, &spec, "idle");
        let prec_errors: Vec<_> = report
            .errors()
            .filter(|d| d.rule == "precondition_reachability")
            .collect();
        assert_eq!(prec_errors.len(), 1);
        assert!(prec_errors[0].message.contains("不可读"));
    }

    #[test]
    fn 前置条件_无前置条件的capability跳过() {
        let cap = cap_with_input("cap.move", "dev1", "position", None, None, 40010);
        // 无 preconditions
        let yaml = r#"
id: test
version: "1.0.0"
devices: [dev1]
states:
  idle:
    entry:
      - capability: cap.move
transitions: []
"#;
        let spec = parse_spec(yaml);
        let ctx = CompilerContext::new(vec![sample_device("dev1")], vec![cap]);
        let report = run_safety_checks(&ctx, &spec, "idle");
        assert!(
            report
                .diagnostics
                .iter()
                .all(|d| d.rule != "precondition_reachability")
        );
    }

    // ---- 规则 4: 状态机完整性 ----

    #[test]
    fn 状态机_全部可达通过() {
        let yaml = r#"
id: test
version: "1.0.0"
states:
  idle:
  running:
  done:
transitions:
  - from: idle
    to: running
    when: "start"
  - from: running
    to: done
    when: "completed"
"#;
        let spec = parse_spec(yaml);
        let ctx = CompilerContext::new(vec![], vec![]);
        let report = run_safety_checks(&ctx, &spec, "idle");
        let sm_diags: Vec<_> = report
            .diagnostics
            .iter()
            .filter(|d| d.rule == "state_machine_completeness")
            .collect();
        assert!(sm_diags.is_empty());
    }

    #[test]
    fn 状态机_不可达状态产生警告() {
        let yaml = r#"
id: test
version: "1.0.0"
states:
  idle:
  active:
  orphan:
transitions:
  - from: idle
    to: active
    when: "start"
"#;
        let spec = parse_spec(yaml);
        let ctx = CompilerContext::new(vec![], vec![]);
        let report = run_safety_checks(&ctx, &spec, "idle");
        let sm_warnings: Vec<_> = report
            .warnings()
            .filter(|d| d.rule == "state_machine_completeness" && d.message.contains("不可达"))
            .collect();
        assert_eq!(sm_warnings.len(), 1);
        assert!(sm_warnings[0].message.contains("orphan"));
    }

    #[test]
    fn 状态机_死胡同状态产生警告() {
        let yaml = r#"
id: test
version: "1.0.0"
states:
  idle:
  stuck:
transitions:
  - from: idle
    to: stuck
    when: "go"
"#;
        let spec = parse_spec(yaml);
        let ctx = CompilerContext::new(vec![], vec![]);
        let report = run_safety_checks(&ctx, &spec, "idle");
        let sm_warnings: Vec<_> = report
            .warnings()
            .filter(|d| d.rule == "state_machine_completeness" && d.message.contains("死胡同"))
            .collect();
        assert_eq!(sm_warnings.len(), 1);
        assert!(sm_warnings[0].message.contains("stuck"));
    }

    #[test]
    fn 状态机_循环触发报错() {
        let yaml = r#"
id: test
version: "1.0.0"
states:
  a:
  b:
transitions:
  - from: a
    to: b
    when: "true"
  - from: b
    to: a
    when: "true"
"#;
        let spec = parse_spec(yaml);
        let ctx = CompilerContext::new(vec![], vec![]);
        let report = run_safety_checks(&ctx, &spec, "a");
        let sm_errors: Vec<_> = report
            .errors()
            .filter(|d| d.rule == "state_machine_completeness" && d.message.contains("循环"))
            .collect();
        assert_eq!(sm_errors.len(), 1);
    }

    #[test]
    fn 状态机_有条件返回_idle_不报循环错误() {
        let yaml = r#"
id: test
version: "1.0.0"
states:
  idle:
  running:
transitions:
  - from: idle
    to: running
    when: "payload.start == true"
  - from: running
    to: idle
    when: "payload.done == true"
"#;
        let spec = parse_spec(yaml);
        let ctx = CompilerContext::new(vec![], vec![]);
        let report = run_safety_checks(&ctx, &spec, "idle");
        let sm_errors: Vec<_> = report
            .errors()
            .filter(|d| d.rule == "state_machine_completeness" && d.message.contains("循环"))
            .collect();
        assert!(
            sm_errors.is_empty(),
            "有条件返回 idle 是正常业务模式，不应作为循环错误: {sm_errors:?}"
        );
    }

    #[test]
    fn 状态机_从_initial_真实遍历识别下游不可达状态() {
        let yaml = r#"
id: test
version: "1.0.0"
states:
  idle:
  active:
  orphan:
  orphan_child:
transitions:
  - from: idle
    to: active
    when: "payload.start == true"
  - from: orphan
    to: orphan_child
    when: "payload.go == true"
"#;
        let spec = parse_spec(yaml);
        let ctx = CompilerContext::new(vec![], vec![]);
        let report = run_safety_checks(&ctx, &spec, "idle");
        let unreachable: Vec<_> = report
            .warnings()
            .filter(|d| d.rule == "state_machine_completeness" && d.message.contains("不可达"))
            .collect();

        assert_eq!(unreachable.len(), 2);
        assert!(unreachable.iter().any(|d| d.message.contains("orphan`")));
        assert!(
            unreachable
                .iter()
                .any(|d| d.message.contains("orphan_child`"))
        );
    }

    #[test]
    fn 状态机_终端状态名不报死胡同() {
        let yaml = r#"
id: test
version: "1.0.0"
states:
  idle:
  fault:
transitions:
  - from: idle
    to: fault
    when: "error"
"#;
        let spec = parse_spec(yaml);
        let ctx = CompilerContext::new(vec![], vec![]);
        let report = run_safety_checks(&ctx, &spec, "idle");
        let dead_end_warnings: Vec<_> = report
            .warnings()
            .filter(|d| d.message.contains("死胡同"))
            .collect();
        assert!(
            dead_end_warnings.is_empty(),
            "fault 是终端状态名，不应报死胡同"
        );
    }

    #[test]
    fn 状态机_通配符transition使所有状态可达() {
        let yaml = r#"
id: test
version: "1.0.0"
states:
  idle:
  fault:
transitions:
  - from: "*"
    to: fault
    when: "error"
    priority: 100
"#;
        let spec = parse_spec(yaml);
        let ctx = CompilerContext::new(vec![], vec![]);
        let report = run_safety_checks(&ctx, &spec, "idle");
        let unreachable_warnings: Vec<_> = report
            .warnings()
            .filter(|d| d.message.contains("不可达"))
            .collect();
        assert!(
            unreachable_warnings.is_empty(),
            "通配符 transition 使 fault 可达"
        );
    }

    // ---- 规则 5: 危险动作审批 ----

    #[test]
    fn 审批_high等级需审批产生警告() {
        let mut cap = cap_with_input("cap.danger", "dev1", "value", None, None, 40010);
        cap.safety = SafetyConstraints {
            level: SafetyLevel::High,
            requires_approval: true,
            max_execution_time: None,
        };
        let yaml = r#"
id: test
version: "1.0.0"
devices: [dev1]
states:
  idle:
    entry:
      - capability: cap.danger
transitions: []
"#;
        let spec = parse_spec(yaml);
        let ctx = CompilerContext::new(vec![sample_device("dev1")], vec![cap]);
        let report = run_safety_checks(&ctx, &spec, "idle");
        let approval_warnings: Vec<_> = report
            .warnings()
            .filter(|d| d.rule == "dangerous_action_approval")
            .collect();
        assert_eq!(approval_warnings.len(), 1);
        assert!(approval_warnings[0].message.contains("High"));
    }

    #[test]
    fn 审批_high等级无需审批不产生警告() {
        let mut cap = cap_with_input("cap.safe", "dev1", "value", None, None, 40010);
        cap.safety = SafetyConstraints {
            level: SafetyLevel::High,
            requires_approval: false,
            max_execution_time: None,
        };
        let yaml = r#"
id: test
version: "1.0.0"
devices: [dev1]
states:
  idle:
    entry:
      - capability: cap.safe
transitions: []
"#;
        let spec = parse_spec(yaml);
        let ctx = CompilerContext::new(vec![sample_device("dev1")], vec![cap]);
        let report = run_safety_checks(&ctx, &spec, "idle");
        assert!(
            report
                .diagnostics
                .iter()
                .all(|d| d.rule != "dangerous_action_approval")
        );
    }

    #[test]
    fn 审批_low等级不产生警告() {
        let cap = cap_with_input("cap.low", "dev1", "value", None, None, 40010);
        let yaml = r#"
id: test
version: "1.0.0"
devices: [dev1]
states:
  idle:
    entry:
      - capability: cap.low
transitions: []
"#;
        let spec = parse_spec(yaml);
        let ctx = CompilerContext::new(vec![sample_device("dev1")], vec![cap]);
        let report = run_safety_checks(&ctx, &spec, "idle");
        assert!(
            report
                .diagnostics
                .iter()
                .all(|d| d.rule != "dangerous_action_approval")
        );
    }

    // ---- 规则 6: 机械互锁 ----

    #[test]
    fn 互锁_同设备同寄存器产生警告() {
        let cap_a = cap_with_input("cap.write_a", "dev1", "value", None, None, 40010);
        let cap_b = cap_with_input("cap.write_b", "dev1", "value", None, None, 40010);
        let yaml = r#"
id: test
version: "1.0.0"
devices: [dev1]
states:
  idle:
    entry:
      - capability: cap.write_a
  running:
    entry:
      - capability: cap.write_b
transitions:
  - from: idle
    to: running
    when: "go"
"#;
        let spec = parse_spec(yaml);
        let ctx = CompilerContext::new(vec![sample_device("dev1")], vec![cap_a, cap_b]);
        let report = run_safety_checks(&ctx, &spec, "idle");
        let interlock_warnings: Vec<_> = report
            .warnings()
            .filter(|d| d.rule == "mechanical_interlock")
            .collect();
        assert_eq!(interlock_warnings.len(), 1);
        assert!(interlock_warnings[0].message.contains("40010"));
    }

    #[test]
    fn 互锁_不同设备不产生警告() {
        let cap_a = cap_with_input("cap.write_a", "dev1", "value", None, None, 40010);
        let cap_b = cap_with_input("cap.write_b", "dev2", "value", None, None, 40010);
        let yaml = r#"
id: test
version: "1.0.0"
devices: [dev1, dev2]
states:
  idle:
    entry:
      - capability: cap.write_a
  running:
    entry:
      - capability: cap.write_b
transitions:
  - from: idle
    to: running
    when: "go"
"#;
        let spec = parse_spec(yaml);
        let ctx = CompilerContext::new(
            vec![sample_device("dev1"), sample_device("dev2")],
            vec![cap_a, cap_b],
        );
        let report = run_safety_checks(&ctx, &spec, "idle");
        assert!(
            report
                .diagnostics
                .iter()
                .all(|d| d.rule != "mechanical_interlock")
        );
    }

    #[test]
    fn 互锁_同设备不同寄存器不产生警告() {
        let cap_a = cap_with_input("cap.write_a", "dev1", "value", None, None, 40010);
        let cap_b = cap_with_input("cap.write_b", "dev1", "value", None, None, 40020);
        let yaml = r#"
id: test
version: "1.0.0"
devices: [dev1]
states:
  idle:
    entry:
      - capability: cap.write_a
  running:
    entry:
      - capability: cap.write_b
transitions:
  - from: idle
    to: running
    when: "go"
"#;
        let spec = parse_spec(yaml);
        let ctx = CompilerContext::new(vec![sample_device("dev1")], vec![cap_a, cap_b]);
        let report = run_safety_checks(&ctx, &spec, "idle");
        assert!(
            report
                .diagnostics
                .iter()
                .all(|d| d.rule != "mechanical_interlock")
        );
    }

    // ---- 辅助函数测试 ----

    #[test]
    fn extract_variable_ref_正常提取() {
        assert_eq!(extract_variable_ref("${position}"), Some("position"));
        assert_eq!(
            extract_variable_ref("${target_pressure}"),
            Some("target_pressure")
        );
    }

    #[test]
    fn extract_variable_ref_非变量模板返回none() {
        assert_eq!(extract_variable_ref("hello"), None);
        assert_eq!(extract_variable_ref("${}"), None);
        assert_eq!(extract_variable_ref("100.0"), None);
    }

    #[test]
    fn extract_identifiers_基本表达式() {
        let ids = extract_identifiers("pressure > 34");
        assert!(ids.contains(&"pressure".to_owned()));
        assert!(!ids.contains(&"34".to_owned())); // 数字被过滤
    }

    #[test]
    fn extract_identifiers_过滤保留字() {
        let ids = extract_identifiers("pressure > 34 and servo_ready == true");
        assert!(ids.contains(&"pressure".to_owned()));
        assert!(ids.contains(&"servo_ready".to_owned()));
        assert!(!ids.contains(&"and".to_owned()));
        assert!(!ids.contains(&"true".to_owned()));
    }
}
