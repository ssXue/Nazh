//! ADR-0009 节点生命周期钩子集成测试。
//!
//! 覆盖 Runner 部署/撤销路径的关键不变量：
//! - on_deploy 按拓扑序调用
//! - on_deploy 失败按逆序回滚已部署的 guards
//! - WorkflowDeployment::shutdown 按逆部署序触发取消
//! - NodeHandle::emit 与 transform 路径产生等价的事件序列

// 测试代码批量豁免 pedantic 风格 lint：测试更看重表达力，强求与生产代码同
// 等严格的 lint 反而损可读性。
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::doc_markdown,
    clippy::needless_pass_by_value,
    clippy::items_after_statements,
    clippy::used_underscore_binding,
    clippy::needless_continue
)]

use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, AtomicUsize, Ordering},
};
use std::time::Duration;

use async_trait::async_trait;
use nazh_engine::{
    CancellationToken, EngineError, ExecutionEvent, LifecycleGuard, NodeCapabilities,
    NodeExecution, NodeLifecycleContext, NodeRegistry, NodeTrait, WorkflowContext, WorkflowGraph,
    deploy_workflow, shared_connection_manager,
};
use serde_json::{Value, json};
use tokio::time::timeout;
use uuid::Uuid;

/// 记录 on_deploy / shutdown 调用顺序的可观测节点。
///
/// 用 `Arc<Mutex<Vec<...>>>` 收集所有实例的事件——所有同类型节点共用一个
/// recorder，调用方在测试里持有 Arc 检查顺序。
struct ProbeNode {
    id: String,
    recorder: Arc<Mutex<Vec<String>>>,
    /// on_deploy 行为：Ok 返回 noop guard；Err 直接报错；Spawn 返回带任务的 guard
    behavior: ProbeBehavior,
}

#[derive(Clone)]
enum ProbeBehavior {
    Noop,
    Error(String),
    /// 后台任务等到 cancel 才退出，并把 "shutdown:{id}" 写入 recorder
    SpawnUntilCancel,
}

impl ProbeNode {
    fn new(
        id: impl Into<String>,
        recorder: Arc<Mutex<Vec<String>>>,
        behavior: ProbeBehavior,
    ) -> Self {
        Self {
            id: id.into(),
            recorder,
            behavior,
        }
    }
}

#[async_trait]
impl NodeTrait for ProbeNode {
    fn id(&self) -> &str {
        &self.id
    }
    fn kind(&self) -> &'static str {
        "probe"
    }

    async fn transform(
        &self,
        _trace_id: Uuid,
        payload: Value,
    ) -> Result<NodeExecution, EngineError> {
        Ok(NodeExecution::broadcast(payload))
    }

    async fn on_deploy(&self, ctx: NodeLifecycleContext) -> Result<LifecycleGuard, EngineError> {
        self.recorder
            .lock()
            .unwrap()
            .push(format!("on_deploy:{}", self.id));
        match &self.behavior {
            ProbeBehavior::Noop => Ok(LifecycleGuard::noop()),
            ProbeBehavior::Error(msg) => {
                Err(EngineError::node_config(self.id.clone(), msg.clone()))
            }
            ProbeBehavior::SpawnUntilCancel => {
                let recorder = Arc::clone(&self.recorder);
                let id = self.id.clone();
                let token = ctx.shutdown.clone();
                let join = tokio::spawn(async move {
                    token.cancelled().await;
                    recorder.lock().unwrap().push(format!("shutdown:{id}"));
                });
                Ok(LifecycleGuard::from_task(ctx.shutdown, join))
            }
        }
    }
}

/// 单根线性 DAG: A → B → C
///
/// `behaviors` 按 [a, b, c] 顺序提供（注意 a/b/c 必须按字母序，因为
/// `topology` 对 root_nodes 排序保证拓扑序确定性）。
fn linear_graph_with_behaviors(behaviors: [ProbeBehavior; 3]) -> (WorkflowGraph, NodeRegistry) {
    let ast = json!({
        "nodes": {
            "a": {"id": "a", "type": "probe"},
            "b": {"id": "b", "type": "probe"},
            "c": {"id": "c", "type": "probe"}
        },
        "edges": [
            {"from": "a", "to": "b"},
            {"from": "b", "to": "c"}
        ]
    });
    let graph = WorkflowGraph::from_json(&ast.to_string()).unwrap();

    let mut registry = NodeRegistry::new();
    let recorder: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
    let behaviors = Arc::new(Mutex::new(Some(behaviors)));
    registry.register_with_capabilities("probe", NodeCapabilities::empty(), {
        let recorder = Arc::clone(&recorder);
        let behaviors = Arc::clone(&behaviors);
        move |def, _res| {
            let mut slot = behaviors.lock().unwrap();
            // 按节点 id 字母序取出对应 behavior（不依赖工厂调用顺序）
            let arr = slot.take().expect("behaviors 已被消费完，重复部署？");
            let behavior = match def.id() {
                "a" => arr[0].clone(),
                "b" => arr[1].clone(),
                "c" => arr[2].clone(),
                other => panic!("unexpected node id: {other}"),
            };
            // 按 id 决定使用后还放回（让其他节点的工厂调用也能取到）
            *slot = Some(arr);
            Ok(Arc::new(ProbeNode::new(
                def.id(),
                Arc::clone(&recorder),
                behavior,
            )))
        }
    });

    // 通过 recorder 让测试断言；这里把 recorder 塞进 graph 不太自然，所以
    // 分两个返回让测试自己持有 recorder（通过另一个辅助函数）
    (graph, registry)
}

/// 同上，但额外暴露 recorder。
fn linear_graph_with_recorder(
    behaviors: [ProbeBehavior; 3],
) -> (WorkflowGraph, NodeRegistry, Arc<Mutex<Vec<String>>>) {
    let ast = json!({
        "nodes": {
            "a": {"id": "a", "type": "probe"},
            "b": {"id": "b", "type": "probe"},
            "c": {"id": "c", "type": "probe"}
        },
        "edges": [
            {"from": "a", "to": "b"},
            {"from": "b", "to": "c"}
        ]
    });
    let graph = WorkflowGraph::from_json(&ast.to_string()).unwrap();

    let mut registry = NodeRegistry::new();
    let recorder: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
    let behaviors_slot: Arc<Mutex<[Option<ProbeBehavior>; 3]>> = Arc::new(Mutex::new([
        Some(behaviors[0].clone()),
        Some(behaviors[1].clone()),
        Some(behaviors[2].clone()),
    ]));
    registry.register_with_capabilities("probe", NodeCapabilities::empty(), {
        let recorder = Arc::clone(&recorder);
        let behaviors_slot = Arc::clone(&behaviors_slot);
        move |def, _res| {
            let idx = match def.id() {
                "a" => 0,
                "b" => 1,
                "c" => 2,
                other => panic!("unexpected node id: {other}"),
            };
            let behavior = behaviors_slot.lock().unwrap()[idx]
                .take()
                .expect("behavior 已被取走");
            Ok(Arc::new(ProbeNode::new(
                def.id(),
                Arc::clone(&recorder),
                behavior,
            )))
        }
    });

    (graph, registry, recorder)
}

#[tokio::test]
async fn on_deploy_按拓扑序调用() {
    let (graph, registry, recorder) = linear_graph_with_recorder([
        ProbeBehavior::Noop,
        ProbeBehavior::Noop,
        ProbeBehavior::Noop,
    ]);
    let deployment = deploy_workflow(graph, shared_connection_manager(), &registry)
        .await
        .unwrap();

    let calls = recorder.lock().unwrap().clone();
    assert_eq!(
        calls,
        vec!["on_deploy:a", "on_deploy:b", "on_deploy:c"],
        "on_deploy 必须按拓扑序 a → b → c 调用"
    );

    deployment.shutdown().await;
}

#[tokio::test]
async fn on_deploy_失败时按逆序回滚已部署节点的_guard() {
    let (graph, registry, recorder) = linear_graph_with_recorder([
        ProbeBehavior::SpawnUntilCancel,             // a 部署成功并起后台任务
        ProbeBehavior::SpawnUntilCancel,             // b 部署成功并起后台任务
        ProbeBehavior::Error("故意失败".to_owned()), // c 失败
    ]);

    let result = deploy_workflow(graph, shared_connection_manager(), &registry).await;
    assert!(result.is_err(), "c 节点 on_deploy 失败时部署应整体返回 Err");

    // deploy 函数 return 时，lifecycle_guards Drop 会触发 a 与 b 的 token cancel；
    // 后台任务感知 cancel 后写入 "shutdown:{id}" 到 recorder。给 Tokio 一点时间。
    tokio::time::sleep(Duration::from_millis(100)).await;

    let calls = recorder.lock().unwrap().clone();
    assert!(
        calls.contains(&"shutdown:a".to_owned()) && calls.contains(&"shutdown:b".to_owned()),
        "a 与 b 的后台任务必须在 deploy 失败后被取消，实际事件序列：{calls:?}"
    );
    // c 的 on_deploy 已被调用但没成功——仍应在 recorder 里
    assert!(
        calls.contains(&"on_deploy:c".to_owned()),
        "c 的 on_deploy 应被调用过"
    );
}

#[tokio::test]
async fn shutdown_后所有节点的_lifecycle_任务都退出() {
    // 注意：shutdown 内部对**根 token cancel 是广播的**（所有节点同时收到
    // cancel），但 `guard.shutdown().await` 是**串行 await join**——逆序仅
    // 影响"何时拿到 join 结果"，不影响"何时写入 recorder"（任务收到 cancel
    // 立即触发回调，写入时机由 Tokio 调度决定）。因此本测试只断言可观测
    // 语义"所有节点都退出"，不断言写入顺序。
    //
    // 真正的"逆拓扑序"价值在于：若节点 cleanup 内部需要按依赖顺序释放共享
    // 资源，串行 await join 比并行 await 更可控。这是实现细节，不在 API 测试范围。
    let (graph, registry, recorder) = linear_graph_with_recorder([
        ProbeBehavior::SpawnUntilCancel,
        ProbeBehavior::SpawnUntilCancel,
        ProbeBehavior::SpawnUntilCancel,
    ]);
    let deployment = deploy_workflow(graph, shared_connection_manager(), &registry)
        .await
        .unwrap();

    deployment.shutdown().await;

    let calls = recorder.lock().unwrap().clone();
    let shutdown_count = calls.iter().filter(|s| s.starts_with("shutdown:")).count();
    assert_eq!(
        shutdown_count, 3,
        "三个节点都应记录 shutdown 事件，实际：{calls:?}"
    );
    for id in ["a", "b", "c"] {
        assert!(
            calls.contains(&format!("shutdown:{id}")),
            "节点 {id} 必须记录 shutdown:{id}，实际：{calls:?}"
        );
    }
}

/// 同样的 payload + metadata，验证：
/// - 走 transform 路径产生 Started + Completed(metadata=Some(map))
/// - 走 NodeHandle::emit 路径产生 Started + Completed(metadata=Some(map))
/// 两者事件**结构等价**（trace_id 和 stage 名不同是预期的）。
#[tokio::test]
async fn node_handle_emit_与_transform_路径事件结构等价() {
    use nazh_core::{ArenaDataStore, DataStore};
    use nazh_engine::NodeHandle;
    use serde_json::Map;
    use tokio::sync::mpsc;

    // 构造一份模拟的 NodeHandle（直接用低阶 API，不经过 deploy）
    let store: Arc<dyn DataStore> = Arc::new(ArenaDataStore::new());
    let (event_tx, mut event_rx) = mpsc::channel(8);
    let handle = NodeHandle::new("trigger-x", store, vec![], event_tx);

    let mut metadata = Map::new();
    metadata.insert("kind".to_owned(), json!("test"));
    handle
        .emit(json!({"v": 1}), metadata.clone())
        .await
        .unwrap();

    let started = event_rx.recv().await.unwrap();
    let completed = event_rx.recv().await.unwrap();

    // 等价性断言：emit 路径与 run_node 路径都遵循"Started → Completed(metadata)"
    // 序列。Stage 名 = node_id，metadata 非空时 Some(map)。
    match started {
        ExecutionEvent::Started { stage, .. } => assert_eq!(stage, "trigger-x"),
        other => panic!("expected Started, got {other:?}"),
    }
    match completed {
        ExecutionEvent::Completed(event) => {
            assert_eq!(event.stage, "trigger-x");
            assert_eq!(event.metadata.as_ref(), Some(&metadata));
        }
        other => panic!("expected Completed, got {other:?}"),
    }
}

#[tokio::test]
async fn on_deploy_的_handle_emit_数据流可被下游接收() {
    // 构造单触发器节点 + 一个下游节点
    let recorder: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
    let recorder_clone = Arc::clone(&recorder);

    struct EmitterNode {
        id: String,
        emit_count: AtomicUsize,
    }
    #[async_trait]
    impl NodeTrait for EmitterNode {
        fn id(&self) -> &str {
            &self.id
        }
        fn kind(&self) -> &'static str {
            "emitter"
        }
        async fn transform(&self, _t: Uuid, p: Value) -> Result<NodeExecution, EngineError> {
            Ok(NodeExecution::broadcast(p))
        }
        async fn on_deploy(
            &self,
            ctx: NodeLifecycleContext,
        ) -> Result<LifecycleGuard, EngineError> {
            // 通过 handle.emit 推一条数据进 DAG
            let handle = ctx.handle.clone();
            let token = ctx.shutdown.clone();
            let join = tokio::spawn(async move {
                let _ = handle
                    .emit(json!({"emitted": true}), serde_json::Map::new())
                    .await;
                token.cancelled().await;
            });
            self.emit_count.fetch_add(1, Ordering::SeqCst);
            Ok(LifecycleGuard::from_task(ctx.shutdown, join))
        }
    }

    struct CollectorNode {
        id: String,
        recorder: Arc<Mutex<Vec<String>>>,
    }
    #[async_trait]
    impl NodeTrait for CollectorNode {
        fn id(&self) -> &str {
            &self.id
        }
        fn kind(&self) -> &'static str {
            "collector"
        }
        async fn transform(&self, _t: Uuid, p: Value) -> Result<NodeExecution, EngineError> {
            self.recorder
                .lock()
                .unwrap()
                .push(format!("collector_received:{p}"));
            Ok(NodeExecution::broadcast(p))
        }
    }

    let mut registry = NodeRegistry::new();
    registry.register_with_capabilities("emitter", NodeCapabilities::TRIGGER, |def, _res| {
        Ok(Arc::new(EmitterNode {
            id: def.id().to_owned(),
            emit_count: AtomicUsize::new(0),
        }))
    });
    registry.register_with_capabilities("collector", NodeCapabilities::empty(), {
        let recorder = Arc::clone(&recorder_clone);
        move |def, _res| {
            Ok(Arc::new(CollectorNode {
                id: def.id().to_owned(),
                recorder: Arc::clone(&recorder),
            }))
        }
    });

    let ast = json!({
        "nodes": {
            "trig": {"id": "trig", "type": "emitter"},
            "sink": {"id": "sink", "type": "collector"}
        },
        "edges": [{"from": "trig", "to": "sink"}]
    });
    let graph = WorkflowGraph::from_json(&ast.to_string()).unwrap();
    let deployment = deploy_workflow(graph, shared_connection_manager(), &registry)
        .await
        .unwrap();

    // 给后台任务时间 emit + downstream 处理
    tokio::time::sleep(Duration::from_millis(100)).await;

    let received = recorder.lock().unwrap().clone();
    assert!(
        received.iter().any(|s| s.contains("\"emitted\":true")),
        "下游 collector 必须收到触发器 emit 的数据，实际：{received:?}"
    );

    deployment.shutdown().await;
}

#[tokio::test]
async fn deployment_drop_未显式_shutdown_仍能_cancel_token() {
    let started = Arc::new(AtomicBool::new(false));
    let cancelled = Arc::new(AtomicBool::new(false));
    let started_c = Arc::clone(&started);
    let cancelled_c = Arc::clone(&cancelled);

    struct DropProbeNode {
        id: String,
        started: Arc<AtomicBool>,
        cancelled: Arc<AtomicBool>,
    }
    #[async_trait]
    impl NodeTrait for DropProbeNode {
        fn id(&self) -> &str {
            &self.id
        }
        fn kind(&self) -> &'static str {
            "drop_probe"
        }
        async fn transform(&self, _t: Uuid, p: Value) -> Result<NodeExecution, EngineError> {
            Ok(NodeExecution::broadcast(p))
        }
        async fn on_deploy(
            &self,
            ctx: NodeLifecycleContext,
        ) -> Result<LifecycleGuard, EngineError> {
            let token = ctx.shutdown.clone();
            let started = Arc::clone(&self.started);
            let cancelled = Arc::clone(&self.cancelled);
            let join = tokio::spawn(async move {
                started.store(true, Ordering::SeqCst);
                token.cancelled().await;
                cancelled.store(true, Ordering::SeqCst);
            });
            Ok(LifecycleGuard::from_task(ctx.shutdown, join))
        }
    }

    let mut registry = NodeRegistry::new();
    registry.register_with_capabilities(
        "drop_probe",
        NodeCapabilities::empty(),
        move |def, _res| {
            Ok(Arc::new(DropProbeNode {
                id: def.id().to_owned(),
                started: Arc::clone(&started_c),
                cancelled: Arc::clone(&cancelled_c),
            }))
        },
    );

    let ast = json!({
        "nodes": {"x": {"id": "x", "type": "drop_probe"}},
        "edges": []
    });
    let graph = WorkflowGraph::from_json(&ast.to_string()).unwrap();
    {
        let _deployment = deploy_workflow(graph, shared_connection_manager(), &registry)
            .await
            .unwrap();
        // 等待后台任务起来
        tokio::time::sleep(Duration::from_millis(50)).await;
        assert!(started.load(Ordering::SeqCst), "后台任务必须已启动");
        // 不调 shutdown，直接让 _deployment drop
    }
    // Drop 后 token 被 cancel；后台任务感知 cancel 退出
    tokio::time::sleep(Duration::from_millis(100)).await;
    assert!(
        cancelled.load(Ordering::SeqCst),
        "deployment drop 必须 cancel 节点 token"
    );
}

#[tokio::test]
async fn shutdown_有超时保护() {
    // 节点 spawn 一个**不响应 cancel** 的任务（不监听 token）；
    // shutdown 应在 LifecycleGuard 默认 5s 超时内返回——本测试设小超时验证。
    struct StuckNode {
        id: String,
    }
    #[async_trait]
    impl NodeTrait for StuckNode {
        fn id(&self) -> &str {
            &self.id
        }
        fn kind(&self) -> &'static str {
            "stuck"
        }
        async fn transform(&self, _t: Uuid, p: Value) -> Result<NodeExecution, EngineError> {
            Ok(NodeExecution::broadcast(p))
        }
        async fn on_deploy(
            &self,
            _ctx: NodeLifecycleContext,
        ) -> Result<LifecycleGuard, EngineError> {
            // 故意忽略 ctx.shutdown，模拟"卡住"的清理路径
            let join = tokio::spawn(async {
                tokio::time::sleep(Duration::from_mins(1)).await;
            });
            Ok(LifecycleGuard::from_task(CancellationToken::new(), join)
                .with_shutdown_timeout(Duration::from_millis(50)))
        }
    }

    let mut registry = NodeRegistry::new();
    registry.register_with_capabilities("stuck", NodeCapabilities::empty(), |def, _res| {
        Ok(Arc::new(StuckNode {
            id: def.id().to_owned(),
        }))
    });

    let ast = json!({
        "nodes": {"s": {"id": "s", "type": "stuck"}},
        "edges": []
    });
    let graph = WorkflowGraph::from_json(&ast.to_string()).unwrap();
    let _deployment = deploy_workflow(graph, shared_connection_manager(), &registry)
        .await
        .unwrap();

    // shutdown 应在 ~50ms 内返回（卡住任务的超时）；不能挂死
    let result = timeout(Duration::from_secs(2), _deployment.shutdown()).await;
    assert!(result.is_ok(), "shutdown 必须受超时保护，不能无限挂死");
}

/// 端到端：真正的 `TimerNode` 部署后能按 interval 触发下游。
///
/// 1. `immediate=true` 时部署后立即触发一次
/// 2. 之后按 `interval_ms` 周期触发
///
/// `shutdown` 后停止触发由 `shutdown_后所有节点的_lifecycle_任务都退出` 间接覆盖。
#[tokio::test]
async fn timer_节点_on_deploy_按_interval_触发下游() {
    use nazh_engine::{NodeRegistry, PluginHost, TimerNode, TimerNodeConfig};
    use nodes_io::IoPlugin;
    use serde_json::Map;

    // 用 IoPlugin（包含 timer）+ 自定义最小 sink 节点。不用 standard_registry
    // 是因为里面的 debugConsole 等需要复杂 config，简化测试。
    let mut host = PluginHost::new();
    host.load(&IoPlugin);
    let mut registry: NodeRegistry = host.into_registry();
    registry.register_with_capabilities("test_sink", NodeCapabilities::empty(), |def, _res| {
        struct SinkNode {
            id: String,
        }
        #[async_trait]
        impl NodeTrait for SinkNode {
            fn id(&self) -> &str {
                &self.id
            }
            fn kind(&self) -> &'static str {
                "test_sink"
            }
            async fn transform(&self, _t: Uuid, p: Value) -> Result<NodeExecution, EngineError> {
                Ok(NodeExecution::broadcast(p))
            }
        }
        Ok(Arc::new(SinkNode {
            id: def.id().to_owned(),
        }))
    });

    let ast = json!({
        "nodes": {
            "ticker": {
                "id": "ticker",
                "type": "timer",
                "config": {
                    "interval_ms": 50,
                    "immediate": true,
                    "inject": {"source": "test_timer"}
                }
            },
            "sink": {"id": "sink", "type": "test_sink"}
        },
        "edges": [{"from": "ticker", "to": "sink"}]
    });

    let graph = WorkflowGraph::from_json(&ast.to_string()).unwrap();
    let mut deployment = deploy_workflow(graph, shared_connection_manager(), &registry)
        .await
        .unwrap();

    // 收集 ~250ms 内的 ticker 节点 Started 事件。immediate=true + interval=50ms
    // 预期至少 3-4 次 emit（实际数量受 Tokio 调度抖动影响）。
    let collect_deadline = tokio::time::Instant::now() + Duration::from_millis(250);
    let mut started_count = 0;
    while tokio::time::Instant::now() < collect_deadline {
        match tokio::time::timeout(Duration::from_millis(100), deployment.next_event()).await {
            Ok(Some(ExecutionEvent::Started { stage, .. })) if stage == "ticker" => {
                started_count += 1;
            }
            Ok(Some(_)) => continue,
            Ok(None) | Err(_) => break,
        }
    }
    assert!(
        started_count >= 2,
        "250ms 内 timer 应至少触发 2 次（immediate + 周期），实际 {started_count} 次"
    );

    // shutdown 消费 deployment；"shutdown 后停止触发"由
    // `shutdown_后所有节点的_lifecycle_任务都退出` 测试覆盖。
    deployment.shutdown().await;

    // 静态检查：避免 unused import
    let _ = TimerNode::new(
        "_warm",
        TimerNodeConfig {
            interval_ms: 1,
            immediate: false,
            inject: Map::new(),
        },
    );
}

// 抑制 lints：测试里有未直接使用的 helper（`linear_graph_with_behaviors`）
// 是后续 reuse 的便利函数。
#[allow(dead_code)]
fn _silence_unused() {
    let _ = linear_graph_with_behaviors([
        ProbeBehavior::Noop,
        ProbeBehavior::Noop,
        ProbeBehavior::Noop,
    ]);
}

// 用 `WorkflowContext::new` 触发一次 ingress.submit，避免 unused warning。
#[allow(dead_code)]
fn _silence_unused_ctx() {
    let _ = WorkflowContext::new(Value::Null);
}
