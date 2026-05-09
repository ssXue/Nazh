use super::*;

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
