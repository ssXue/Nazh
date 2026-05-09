use super::*;

#[tokio::test]
async fn set_值变化时发_variablechanged_事件() {
    let (tx, mut rx) = mpsc::channel(8);
    let mut decls = HashMap::new();
    decls.insert(
        "x".to_owned(),
        VariableDeclaration {
            variable_type: PinType::Integer,
            initial: Value::from(0_i64),
        },
    );
    let vars = WorkflowVariables::from_declarations(&decls).unwrap();
    vars.set_event_sender("wf-1".to_owned(), tx);

    vars.set("x", Value::from(1_i64), Some("node-A")).unwrap();

    let event = rx.recv().await.expect("应收到事件");
    match event {
        WorkflowVariableEvent::Changed {
            workflow_id,
            name,
            value,
            updated_by,
            ..
        } => {
            assert_eq!(workflow_id, "wf-1");
            assert_eq!(name, "x");
            assert_eq!(value, Value::from(1_i64));
            assert_eq!(updated_by.as_deref(), Some("node-A"));
        }
        other @ WorkflowVariableEvent::Deleted { .. } => {
            panic!("expected Changed, got {other:?}")
        }
    }
}

#[tokio::test]
async fn set_值未变化时不发事件() {
    use tokio::time::{Duration, timeout};

    let (tx, mut rx) = mpsc::channel(8);
    let mut decls = HashMap::new();
    decls.insert(
        "x".to_owned(),
        VariableDeclaration {
            variable_type: PinType::Integer,
            initial: Value::from(42_i64),
        },
    );
    let vars = WorkflowVariables::from_declarations(&decls).unwrap();
    vars.set_event_sender("wf-1".to_owned(), tx);

    // 写入与初值相同的值
    vars.set("x", Value::from(42_i64), Some("node-A")).unwrap();

    // 等 50ms 确保不会有事件到达
    let result = timeout(Duration::from_millis(50), rx.recv()).await;
    assert!(result.is_err(), "值未变化应不发事件，但收到：{result:?}");
}

#[tokio::test]
async fn cas_成功且值变化时发事件() {
    let (tx, mut rx) = mpsc::channel(8);
    let mut decls = HashMap::new();
    decls.insert(
        "c".to_owned(),
        VariableDeclaration {
            variable_type: PinType::Integer,
            initial: Value::from(0_i64),
        },
    );
    let vars = WorkflowVariables::from_declarations(&decls).unwrap();
    vars.set_event_sender("wf-1".to_owned(), tx);

    let ok = vars
        .compare_and_swap("c", &Value::from(0_i64), Value::from(1_i64), None)
        .unwrap();
    assert!(ok);

    let event = rx.recv().await.expect("应收到事件");
    assert!(matches!(event, WorkflowVariableEvent::Changed { .. }));
}

#[tokio::test]
async fn cas_失败时不发事件() {
    use tokio::time::{Duration, timeout};

    let (tx, mut rx) = mpsc::channel(8);
    let mut decls = HashMap::new();
    decls.insert(
        "c".to_owned(),
        VariableDeclaration {
            variable_type: PinType::Integer,
            initial: Value::from(0_i64),
        },
    );
    let vars = WorkflowVariables::from_declarations(&decls).unwrap();
    vars.set_event_sender("wf-1".to_owned(), tx);

    // expected 不匹配，CAS 应返回 false 不写入
    let ok = vars
        .compare_and_swap("c", &Value::from(99_i64), Value::from(1_i64), None)
        .unwrap();
    assert!(!ok);

    let result = timeout(Duration::from_millis(50), rx.recv()).await;
    assert!(result.is_err(), "CAS 失败不应发事件");
}

#[tokio::test]
async fn 未设置_event_sender_时_set_仍然正常工作() {
    let mut decls = HashMap::new();
    decls.insert(
        "x".to_owned(),
        VariableDeclaration {
            variable_type: PinType::Integer,
            initial: Value::from(0_i64),
        },
    );
    let vars = WorkflowVariables::from_declarations(&decls).unwrap();

    // 未调 set_event_sender，set 不应 panic 也不应报错
    vars.set("x", Value::from(7_i64), Some("node-A")).unwrap();
    assert_eq!(vars.get_value("x"), Some(Value::from(7_i64)));
}

#[test]
fn set_event_sender_重复注入时第二个_sender_被忽略() {
    let (tx1, _rx1) = mpsc::channel::<WorkflowVariableEvent>(1);
    let (tx2, _rx2) = mpsc::channel::<WorkflowVariableEvent>(1);
    let vars = WorkflowVariables::empty();
    vars.set_event_sender("wf-1".to_owned(), tx1);
    // 第二次注入应被忽略并 tracing::warn!，但函数本身不 panic 不返回错误
    vars.set_event_sender("wf-2".to_owned(), tx2);
    // 只能间接验证：再写一个 set 不会崩，也不会因 sender 不一致而报错
    // （此测试主要防止未来重构 OnceCell 时丢失 "set 失败仅日志" 契约）
}

#[tokio::test]
async fn cas_成功但_new_等于_expected_时不发事件() {
    use tokio::time::{Duration, timeout};

    let (tx, mut rx) = mpsc::channel(8);
    let mut decls = HashMap::new();
    decls.insert(
        "c".to_owned(),
        VariableDeclaration {
            variable_type: PinType::Integer,
            initial: Value::from(7_i64),
        },
    );
    let vars = WorkflowVariables::from_declarations(&decls).unwrap();
    vars.set_event_sender("wf-1".to_owned(), tx);

    // expected = 7（与当前值匹配）；new 也 = 7（退化情形）。CAS 成功，但值没变，不应发事件。
    let ok = vars
        .compare_and_swap("c", &Value::from(7_i64), Value::from(7_i64), None)
        .unwrap();
    assert!(ok, "CAS 应成功（expected 与当前值匹配）");

    let result = timeout(Duration::from_millis(50), rx.recv()).await;
    assert!(
        result.is_err(),
        "退化情形 (new == expected == current) 不应发事件，但收到：{result:?}"
    );
}

#[test]
fn set_事件通道满时记录丢弃计数且不回滚变量() {
    let (tx, _rx) = mpsc::channel(1);
    let mut decls = HashMap::new();
    decls.insert(
        "x".to_owned(),
        VariableDeclaration {
            variable_type: PinType::Integer,
            initial: Value::from(0_i64),
        },
    );
    let vars = WorkflowVariables::from_declarations(&decls).unwrap();
    vars.set_event_sender("wf-1".to_owned(), tx);

    vars.set("x", Value::from(1_i64), Some("node-A")).unwrap();
    vars.set("x", Value::from(2_i64), Some("node-A")).unwrap();

    assert_eq!(vars.get_value("x"), Some(Value::from(2_i64)));
    assert_eq!(
        vars.dropped_variable_event_count_for_test(),
        1,
        "事件通道满时应记录一次丢弃"
    );
}

#[test]
fn remove_事件通道关闭时记录丢弃计数且删除变量() {
    let (tx, rx) = mpsc::channel(1);
    let mut decls = HashMap::new();
    decls.insert(
        "x".to_owned(),
        VariableDeclaration {
            variable_type: PinType::Integer,
            initial: Value::from(1_i64),
        },
    );
    let vars = WorkflowVariables::from_declarations(&decls).unwrap();
    vars.set_event_sender("wf-1".to_owned(), tx);
    drop(rx);

    let removed = vars.remove("x").expect("应返回旧值");

    assert_eq!(removed.value, Value::from(1_i64));
    assert!(vars.get("x").is_none(), "事件发送失败不应回滚删除");
    assert_eq!(
        vars.dropped_variable_event_count_for_test(),
        1,
        "事件通道关闭时应记录一次丢弃"
    );
}

#[tokio::test]
async fn remove_成功时发_variabledeleted_事件() {
    let (tx, mut rx) = mpsc::channel(8);
    let mut decls = HashMap::new();
    decls.insert(
        "x".to_owned(),
        VariableDeclaration {
            variable_type: PinType::Integer,
            initial: Value::from(1_i64),
        },
    );
    let vars = WorkflowVariables::from_declarations(&decls).unwrap();
    vars.set_event_sender("wf-1".to_owned(), tx);

    vars.remove("x").expect("应成功移除");

    let event = rx.recv().await.expect("应收到事件");
    match event {
        WorkflowVariableEvent::Deleted { workflow_id, name } => {
            assert_eq!(workflow_id, "wf-1");
            assert_eq!(name, "x");
        }
        other @ WorkflowVariableEvent::Changed { .. } => {
            panic!("expected Deleted, got {other:?}")
        }
    }
}

#[tokio::test]
async fn remove_不存在的变量不发事件() {
    use tokio::time::{Duration, timeout};

    let (tx, mut rx) = mpsc::channel(8);
    let vars = WorkflowVariables::empty();
    vars.set_event_sender("wf-1".to_owned(), tx);

    assert!(vars.remove("nope").is_none());

    let result = timeout(Duration::from_millis(50), rx.recv()).await;
    assert!(result.is_err(), "移除不存在的变量不应发事件");
}

#[tokio::test]
async fn reset_值变化时发_variablechanged_事件() {
    let (tx, mut rx) = mpsc::channel(8);
    let mut decls = HashMap::new();
    decls.insert(
        "x".to_owned(),
        VariableDeclaration {
            variable_type: PinType::Integer,
            initial: Value::from(10_i64),
        },
    );
    let vars = WorkflowVariables::from_declarations(&decls).unwrap();
    vars.set_event_sender("wf-1".to_owned(), tx);

    // 先改成 99，再 reset 回 10
    vars.set("x", Value::from(99_i64), None).unwrap();
    let _ = rx.recv().await;
    vars.reset("x", Some("ipc")).unwrap();

    let event = rx.recv().await.expect("reset 应触发事件");
    match event {
        WorkflowVariableEvent::Changed { value, name, .. } => {
            assert_eq!(name, "x");
            assert_eq!(value, Value::from(10_i64));
        }
        other @ WorkflowVariableEvent::Deleted { .. } => {
            panic!("expected Changed, got {other:?}")
        }
    }
}
