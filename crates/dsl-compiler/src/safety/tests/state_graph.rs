use super::*;

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
