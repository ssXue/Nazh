use super::*;

#[tokio::test]
async fn watch_channel_在_set_值变化时通知() {
    let mut decls = HashMap::new();
    decls.insert(
        "x".to_owned(),
        VariableDeclaration {
            variable_type: PinType::Integer,
            initial: Value::from(0_i64),
        },
    );
    let vars = WorkflowVariables::from_declarations(&decls).unwrap();

    let mut rx = vars.subscribe("x").expect("变量已声明");

    vars.set("x", Value::from(1_i64), Some("node-A")).unwrap();

    rx.changed().await.expect("watch 应收到变更通知");
    let borrowed = rx.borrow();
    let snapshot = borrowed.as_ref().expect("值应为 Some");
    assert_eq!(snapshot.1, Value::from(1_i64));
}

#[tokio::test]
async fn watch_channel_在_set_值未变化时不通知() {
    use tokio::time::{Duration, timeout};

    let mut decls = HashMap::new();
    decls.insert(
        "x".to_owned(),
        VariableDeclaration {
            variable_type: PinType::Integer,
            initial: Value::from(42_i64),
        },
    );
    let vars = WorkflowVariables::from_declarations(&decls).unwrap();

    let mut rx = vars.subscribe("x").expect("变量已声明");

    // 写入与初值相同的值
    vars.set("x", Value::from(42_i64), None).unwrap();

    let result = timeout(Duration::from_millis(50), rx.changed()).await;
    assert!(result.is_err(), "值未变化时 watch 不应通知");
}

#[tokio::test]
async fn watch_channel_在_cas_成功且值变化时通知() {
    let mut decls = HashMap::new();
    decls.insert(
        "c".to_owned(),
        VariableDeclaration {
            variable_type: PinType::Integer,
            initial: Value::from(0_i64),
        },
    );
    let vars = WorkflowVariables::from_declarations(&decls).unwrap();

    let mut rx = vars.subscribe("c").expect("变量已声明");

    let ok = vars
        .compare_and_swap("c", &Value::from(0_i64), Value::from(1_i64), None)
        .unwrap();
    assert!(ok);

    rx.changed().await.expect("watch 应收到变更通知");
    let borrowed = rx.borrow();
    let snapshot = borrowed.as_ref().expect("值应为 Some");
    assert_eq!(snapshot.1, Value::from(1_i64));
}

#[tokio::test]
async fn watch_channel_不存在变量时_subscribe_返回_none() {
    let vars = WorkflowVariables::empty();
    assert!(vars.subscribe("nonexistent").is_none());
}

#[tokio::test]
async fn watch_channel_多个订阅者同时收到通知() {
    let mut decls = HashMap::new();
    decls.insert(
        "x".to_owned(),
        VariableDeclaration {
            variable_type: PinType::Integer,
            initial: Value::from(0_i64),
        },
    );
    let vars = WorkflowVariables::from_declarations(&decls).unwrap();

    let mut rx1 = vars.subscribe("x").unwrap();
    let mut rx2 = vars.subscribe("x").unwrap();

    vars.set("x", Value::from(99_i64), None).unwrap();

    rx1.changed().await.unwrap();
    rx2.changed().await.unwrap();
    let b1 = rx1.borrow();
    let b2 = rx2.borrow();
    assert_eq!(b1.as_ref().unwrap().1, Value::from(99_i64));
    assert_eq!(b2.as_ref().unwrap().1, Value::from(99_i64));
}
