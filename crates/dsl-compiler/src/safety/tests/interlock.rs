use super::*;

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
