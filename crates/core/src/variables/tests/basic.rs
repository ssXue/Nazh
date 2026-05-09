use super::*;

#[test]
fn 声明并读取_setpoint() {
    let vars = vars_with("setpoint", PinType::Float, Value::from(25.0));
    let read = vars.get("setpoint").unwrap();
    assert_eq!(read.value, Value::from(25.0));
    assert_eq!(read.variable_type, PinType::Float);
    assert!(read.updated_by.is_none(), "初值无 updated_by");
}

#[test]
fn 写入更新值并标记_updated_by() {
    let vars = vars_with("counter", PinType::Integer, Value::from(0_i64));
    vars.set("counter", Value::from(7_i64), Some("node-A"))
        .unwrap();
    let read = vars.get("counter").unwrap();
    assert_eq!(read.value, Value::from(7_i64));
    assert_eq!(read.updated_by.as_deref(), Some("node-A"));
}

#[test]
fn 类型不匹配的写入被拒绝() {
    let vars = vars_with("mode", PinType::String, Value::from("auto"));
    let err = vars
        .set("mode", Value::from(42_i64), Some("node-A"))
        .unwrap_err();
    assert!(matches!(err, EngineError::VariableTypeMismatch { .. }));
    assert_eq!(
        vars.get("mode").unwrap().value,
        Value::from("auto"),
        "拒绝写入后值应保持不变"
    );
}

#[test]
fn 写入未声明的变量返回_unknownvariable() {
    let vars = vars_with("x", PinType::Integer, Value::from(0_i64));
    let err = vars.set("y", Value::from(1_i64), None).unwrap_err();
    assert!(matches!(err, EngineError::UnknownVariable { name } if name == "y"));
}

#[test]
fn cas_期望值匹配时写入成功() {
    let vars = vars_with("counter", PinType::Integer, Value::from(0_i64));
    let ok = vars
        .compare_and_swap(
            "counter",
            &Value::from(0_i64),
            Value::from(1_i64),
            Some("node-A"),
        )
        .unwrap();
    assert!(ok);
    assert_eq!(vars.get("counter").unwrap().value, Value::from(1_i64));
}

#[test]
fn cas_期望值不匹配时返回_false() {
    let vars = vars_with("counter", PinType::Integer, Value::from(0_i64));
    let ok = vars
        .compare_and_swap("counter", &Value::from(99_i64), Value::from(1_i64), None)
        .unwrap();
    assert!(!ok);
    assert_eq!(vars.get("counter").unwrap().value, Value::from(0_i64));
}

#[test]
fn cas_类型不匹配时返回_err() {
    let vars = vars_with("counter", PinType::Integer, Value::from(0_i64));
    let err = vars
        .compare_and_swap("counter", &Value::from(0_i64), Value::from("oops"), None)
        .unwrap_err();
    assert!(matches!(err, EngineError::VariableTypeMismatch { .. }));
}

#[test]
fn snapshot_含全部声明() {
    let mut declarations = HashMap::new();
    declarations.insert(
        "a".to_owned(),
        VariableDeclaration {
            variable_type: PinType::Integer,
            initial: Value::from(1_i64),
        },
    );
    declarations.insert(
        "b".to_owned(),
        VariableDeclaration {
            variable_type: PinType::String,
            initial: Value::from("x"),
        },
    );
    let vars = Arc::new(WorkflowVariables::from_declarations(&declarations).unwrap());
    let snap = vars.snapshot();
    assert_eq!(snap.len(), 2);
    assert!(snap.contains_key("a"));
    assert!(snap.contains_key("b"));
}

#[test]
fn empty_构造器写入任意键都报_unknownvariable() {
    let vars = WorkflowVariables::empty();
    let err = vars.set("any-key", Value::from(1_i64), None).unwrap_err();
    assert!(
        matches!(err, EngineError::UnknownVariable { ref name } if name == "any-key"),
        "empty() 构造器写入任意键应返回 UnknownVariable，实际：{err}"
    );
    let cas_err = vars
        .compare_and_swap("any-key", &Value::from(0_i64), Value::from(1_i64), None)
        .unwrap_err();
    assert!(matches!(cas_err, EngineError::UnknownVariable { .. }));
}

#[test]
fn 初值类型不匹配_from_declarations_失败() {
    let mut declarations = HashMap::new();
    declarations.insert(
        "wrong".to_owned(),
        VariableDeclaration {
            variable_type: PinType::Integer,
            initial: Value::from("not-a-number"),
        },
    );
    let err = WorkflowVariables::from_declarations(&declarations).unwrap_err();
    assert!(matches!(err, EngineError::VariableInitialMismatch { .. }));
}

#[test]
fn remove_存在的变量返回旧值() {
    let vars = vars_with("x", PinType::Integer, Value::from(42_i64));
    let removed = vars.remove("x").expect("应返回旧值");
    assert_eq!(removed.value, Value::from(42_i64));
    assert!(vars.get("x").is_none(), "移除后 get 应返回 None");
}

#[test]
fn remove_不存在的变量返回_none() {
    let vars = WorkflowVariables::empty();
    assert!(vars.remove("nope").is_none());
}

#[tokio::test]
async fn remove_成功时_watch_channel_收到_none() {
    let mut decls = HashMap::new();
    decls.insert(
        "x".to_owned(),
        VariableDeclaration {
            variable_type: PinType::Integer,
            initial: Value::from(1_i64),
        },
    );
    let vars = WorkflowVariables::from_declarations(&decls).unwrap();

    let mut rx = vars.subscribe("x").expect("变量已声明");

    vars.remove("x").expect("应成功移除");

    rx.changed().await.expect("watch 应收到通知");
    assert!(rx.borrow().is_none(), "删除后 watch 值应为 None");
}

#[test]
fn reset_恢复到声明初值() {
    let vars = vars_with("counter", PinType::Integer, Value::from(0_i64));
    vars.set("counter", Value::from(42_i64), Some("node-A"))
        .unwrap();
    assert_eq!(vars.get_value("counter"), Some(Value::from(42_i64)));

    vars.reset("counter", Some("ipc")).unwrap();
    assert_eq!(
        vars.get_value("counter"),
        Some(Value::from(0_i64)),
        "reset 后应恢复到初值 0"
    );
    assert_eq!(
        vars.get("counter").unwrap().updated_by.as_deref(),
        Some("ipc")
    );
}

#[test]
fn reset_未声明变量返回_unknownvariable() {
    let vars = WorkflowVariables::empty();
    let err = vars.reset("nope", None).unwrap_err();
    assert!(
        matches!(err, EngineError::UnknownVariable { ref name } if name == "nope"),
        "reset 不存在的变量应返回 UnknownVariable"
    );
}
