use std::sync::atomic::{AtomicBool, Ordering};

use super::*;
use crate::ArenaDataStore;

#[tokio::test]
async fn noop_guard_drop_不_panic() {
    let guard = LifecycleGuard::noop();
    drop(guard);
}

#[tokio::test]
async fn noop_guard_shutdown_立即返回() {
    let guard = LifecycleGuard::noop();
    guard.shutdown().await;
}

#[tokio::test]
async fn guard_drop_触发_token_cancel() {
    let token = CancellationToken::new();
    let observed = Arc::new(AtomicBool::new(false));
    let observed_clone = Arc::clone(&observed);
    let task_token = token.clone();
    let join = tokio::spawn(async move {
        task_token.cancelled().await;
        observed_clone.store(true, Ordering::SeqCst);
    });

    let guard = LifecycleGuard::from_task(token, join);
    drop(guard);
    // 给任务一个 yield 机会观察到 cancel
    tokio::time::sleep(Duration::from_millis(50)).await;
    assert!(observed.load(Ordering::SeqCst), "drop 应触发 cancel 信号");
}

#[tokio::test]
async fn shutdown_等待任务正常退出() {
    let token = CancellationToken::new();
    let task_token = token.clone();
    let join = tokio::spawn(async move {
        task_token.cancelled().await;
        // 模拟一点清理工作
        tokio::time::sleep(Duration::from_millis(10)).await;
    });

    let guard = LifecycleGuard::from_task(token, join);
    let shutdown_started = std::time::Instant::now();
    guard.shutdown().await;
    let elapsed = shutdown_started.elapsed();
    assert!(
        elapsed < Duration::from_secs(1),
        "shutdown 应等到任务退出，但花了 {elapsed:?}"
    );
}

#[tokio::test]
#[allow(clippy::duration_suboptimal_units)]
async fn shutdown_超时则放弃等待() {
    let token = CancellationToken::new();
    // 任务故意忽略 cancel，模拟"卡住"的清理
    let join = tokio::spawn(async {
        tokio::time::sleep(Duration::from_mins(1)).await;
    });

    let guard =
        LifecycleGuard::from_task(token, join).with_shutdown_timeout(Duration::from_millis(50));
    let started = std::time::Instant::now();
    guard.shutdown().await;
    let elapsed = started.elapsed();
    assert!(
        elapsed < Duration::from_millis(500),
        "shutdown 超时应在 ~50ms 后返回，实际 {elapsed:?}"
    );
}

#[tokio::test]
async fn node_handle_emit_发出_started_completed_事件() {
    let store: Arc<dyn DataStore> = Arc::new(ArenaDataStore::new());
    let (event_tx, mut event_rx) = mpsc::channel(8);
    let (downstream_tx, mut downstream_rx) = mpsc::channel(4);
    let handle = NodeHandle::new("trigger-1", store, vec![downstream_tx], event_tx);

    handle
        .emit(serde_json::json!({"value": 42}), Map::new())
        .await
        .unwrap();

    let started = event_rx.recv().await.unwrap();
    assert!(matches!(started, ExecutionEvent::Started { .. }));
    let completed = event_rx.recv().await.unwrap();
    match completed {
        ExecutionEvent::Completed(event) => {
            assert_eq!(event.stage, "trigger-1");
            assert!(event.metadata.is_none(), "空 metadata 应转为 None");
        }
        other => panic!("expected Completed, got {other:?}"),
    }

    let ctx_ref = downstream_rx.recv().await.unwrap();
    assert_eq!(ctx_ref.source_node.as_deref(), Some("trigger-1"));
}

#[tokio::test]
async fn node_handle_emit_metadata_非空时进入_completed() {
    let store: Arc<dyn DataStore> = Arc::new(ArenaDataStore::new());
    let (event_tx, mut event_rx) = mpsc::channel(8);
    let handle = NodeHandle::new("trigger-2", store, vec![], event_tx);

    let mut metadata = Map::new();
    metadata.insert("timer".to_owned(), serde_json::json!({"interval_ms": 1000}));
    handle
        .emit(serde_json::Value::Null, metadata.clone())
        .await
        .unwrap();

    // 跳过 Started，校验 Completed 中的 metadata
    let _ = event_rx.recv().await;
    match event_rx.recv().await.unwrap() {
        ExecutionEvent::Completed(event) => {
            assert_eq!(event.metadata.as_ref(), Some(&metadata));
        }
        other => panic!("expected Completed, got {other:?}"),
    }
}

#[tokio::test]
async fn node_handle_emit_下游关闭不报错() {
    let store = Arc::new(ArenaDataStore::new());
    let store_dyn: Arc<dyn DataStore> = store.clone();
    let (event_tx, _event_rx) = mpsc::channel(8);
    let (downstream_tx, downstream_rx) = mpsc::channel::<ContextRef>(1);
    drop(downstream_rx); // 立即关闭下游
    let handle = NodeHandle::new("trigger-3", store_dyn, vec![downstream_tx], event_tx);

    // 不应返回 Err；只是 tracing::debug! 记录
    handle
        .emit(serde_json::Value::Null, Map::new())
        .await
        .unwrap();

    assert!(store.is_empty(), "发送失败的数据引用必须被释放");
}

#[tokio::test]
async fn node_handle_emit_无下游时释放_payload() {
    let store = Arc::new(ArenaDataStore::new());
    let store_dyn: Arc<dyn DataStore> = store.clone();
    let (event_tx, _event_rx) = mpsc::channel(8);
    let handle = NodeHandle::new("trigger-empty", store_dyn, vec![], event_tx);

    handle
        .emit(serde_json::json!({"value": 42}), Map::new())
        .await
        .unwrap();

    assert!(store.is_empty(), "无下游时不应留下无人消费的 payload");
}

#[tokio::test]
async fn node_handle_emit_事件通道满时仍不阻塞数据通路() {
    let store: Arc<dyn DataStore> = Arc::new(ArenaDataStore::new());
    let (event_tx, _event_rx) = mpsc::channel(1);
    event_tx
        .try_send(ExecutionEvent::Started {
            stage: "占满事件通道".to_owned(),
            trace_id: Uuid::nil(),
        })
        .unwrap();
    let (downstream_tx, mut downstream_rx) = mpsc::channel(4);
    let handle = NodeHandle::new("trigger-full", store, vec![downstream_tx], event_tx);

    match tokio::time::timeout(
        Duration::from_millis(100),
        handle.emit(serde_json::json!({"value": 42}), Map::new()),
    )
    .await
    {
        Ok(Ok(())) => {}
        Ok(Err(err)) => panic!("emit 应返回成功：{err}"),
        Err(err) => panic!("事件通道满不应阻塞 emit：{err}"),
    }

    let ctx_ref = downstream_rx.recv().await.unwrap();
    assert_eq!(ctx_ref.source_node.as_deref(), Some("trigger-full"));
}

#[tokio::test]
async fn lifecycle_context_暴露_variables() {
    use crate::{PinType, VariableDeclaration, WorkflowVariables};
    use std::collections::HashMap;

    let mut declarations = HashMap::new();
    declarations.insert(
        "setpoint".to_owned(),
        VariableDeclaration {
            variable_type: PinType::Float,
            initial: serde_json::Value::from(25.0),
        },
    );
    let vars = Arc::new(WorkflowVariables::from_declarations(&declarations).unwrap());

    let store: Arc<dyn DataStore> = Arc::new(ArenaDataStore::new());
    let (event_tx, _event_rx) = mpsc::channel(8);
    let handle = NodeHandle::new("trigger-x", store, vec![], event_tx);
    let token = CancellationToken::new();

    let ctx = NodeLifecycleContext {
        resources: Arc::new(crate::RuntimeResources::new()),
        handle,
        shutdown: token.child_token(),
        variables: Arc::clone(&vars),
    };

    assert_eq!(
        ctx.variables.get("setpoint").unwrap().value,
        serde_json::Value::from(25.0)
    );
}
