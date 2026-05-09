use super::*;

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
