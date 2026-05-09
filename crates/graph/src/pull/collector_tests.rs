use super::*;
use crate::types::WorkflowEdge;
use nazh_core::{CachedOutput, NodeExecution, PinDefinition, PinDirection, PinKind, PinType};
use serde_json::json;

fn data_edge(from: &str, sport: Option<&str>, to: &str, tport: Option<&str>) -> WorkflowEdge {
    WorkflowEdge {
        from: from.to_owned(),
        to: to.to_owned(),
        source_port_id: sport.map(ToOwned::to_owned),
        target_port_id: tport.map(ToOwned::to_owned),
    }
}

// ---- Phase 4：empty_policy 分支 + PureMemo 单元测试 ----

/// 测试用 Exec 上游节点（有 Data 输出 pin `latest`）。
struct StubExecNode {
    id: String,
    input_pins: Vec<PinDefinition>,
}

impl StubExecNode {
    fn with_data_input(empty_policy: EmptyPolicy, ttl_ms: Option<u64>) -> Self {
        Self {
            id: "consumer".to_owned(),
            input_pins: vec![
                PinDefinition::default_input(),
                PinDefinition {
                    id: "sensor".to_owned(),
                    label: "sensor".to_owned(),
                    pin_type: PinType::Json,
                    direction: PinDirection::Input,
                    required: false,
                    kind: PinKind::Data,
                    description: None,
                    empty_policy,
                    block_timeout_ms: None,
                    ttl_ms,
                },
            ],
        }
    }
}

#[async_trait::async_trait]
impl NodeTrait for StubExecNode {
    fn id(&self) -> &str {
        &self.id
    }
    fn kind(&self) -> &'static str {
        "stubExec"
    }
    fn input_pins(&self) -> Vec<PinDefinition> {
        self.input_pins.clone()
    }
    fn output_pins(&self) -> Vec<PinDefinition> {
        vec![PinDefinition::default_output()]
    }
    async fn transform(
        &self,
        _trace_id: Uuid,
        payload: Value,
    ) -> Result<NodeExecution, EngineError> {
        Ok(NodeExecution::broadcast(payload))
    }
}

#[allow(clippy::type_complexity)]
fn setup_pull_test(
    empty_policy: EmptyPolicy,
    ttl_ms: Option<u64>,
) -> (
    HashMap<String, Arc<dyn NodeTrait>>,
    HashMap<String, Arc<OutputCache>>,
    HashMap<String, Option<Duration>>,
    EdgesByConsumer,
    PureMemo,
    Arc<dyn NodeTrait>,
) {
    let upstream_cache = Arc::new(OutputCache::new());
    upstream_cache.prepare_slot("latest");

    let consumer =
        Arc::new(StubExecNode::with_data_input(empty_policy, ttl_ms)) as Arc<dyn NodeTrait>;

    let mut nodes = HashMap::new();
    nodes.insert("upstream".to_owned(), consumer.clone());

    let mut caches = HashMap::new();
    caches.insert("upstream".to_owned(), upstream_cache);

    let timeouts = HashMap::new();

    let e = data_edge("upstream", Some("latest"), "consumer", Some("sensor"));
    let refs = vec![&e];
    let edges = super::super::index::build_edges_by_consumer(&refs);

    (nodes, caches, timeouts, edges, PureMemo::new(), consumer)
}

#[tokio::test]
async fn cache_hit_直接返回值() {
    let (nodes, caches, timeouts, edges, memo, consumer) =
        setup_pull_test(EmptyPolicy::default(), None);

    // 写入缓存
    caches["upstream"].write_now("latest", json!({"temp": 25.0}), Uuid::nil());

    let result = pull_data_inputs(
        "consumer",
        consumer.as_ref(),
        json!({}),
        &edges,
        &nodes,
        &caches,
        &timeouts,
        &memo,
        Uuid::nil(),
    )
    .await
    .unwrap();

    assert_eq!(result["sensor"], json!({"temp": 25.0}));
}

#[tokio::test]
async fn default_value_在缓存空时返回() {
    let (nodes, caches, timeouts, edges, memo, consumer) =
        setup_pull_test(EmptyPolicy::DefaultValue(json!(-1)), None);

    // 不写缓存 → 空槽

    let result = pull_data_inputs(
        "consumer",
        consumer.as_ref(),
        json!({}),
        &edges,
        &nodes,
        &caches,
        &timeouts,
        &memo,
        Uuid::nil(),
    )
    .await
    .unwrap();

    assert_eq!(result["sensor"], json!(-1));
}

#[tokio::test]
async fn skip_在缓存空时返回_null() {
    let (nodes, caches, timeouts, edges, memo, consumer) = setup_pull_test(EmptyPolicy::Skip, None);

    let result = pull_data_inputs(
        "consumer",
        consumer.as_ref(),
        json!({}),
        &edges,
        &nodes,
        &caches,
        &timeouts,
        &memo,
        Uuid::nil(),
    )
    .await
    .unwrap();

    assert_eq!(result["sensor"], Value::Null);
}

#[tokio::test]
async fn block_until_ready_在缓存空时超时() {
    let (nodes, caches, timeouts, edges, memo, consumer) =
        setup_pull_test(EmptyPolicy::BlockUntilReady, None);

    // 不写缓存，expect timeout
    let result = pull_data_inputs(
        "consumer",
        consumer.as_ref(),
        json!({}),
        &edges,
        &nodes,
        &caches,
        &timeouts,
        &memo,
        Uuid::nil(),
    )
    .await;

    assert!(result.is_err(), "BlockUntilReady 在缓存空时应超时");
    let err_msg = format!("{}", result.unwrap_err());
    assert!(
        err_msg.contains("超时"),
        "错误消息应含'超时'，实际：{err_msg}"
    );
}

#[tokio::test]
async fn ttl_过期走_default_value() {
    let (nodes, caches, timeouts, edges, memo, consumer) =
        setup_pull_test(EmptyPolicy::DefaultValue(json!(0)), Some(10));

    // 写入缓存，但 produced_at 在 100ms 前（远超 10ms TTL）
    caches["upstream"].write(
        "latest",
        CachedOutput {
            value: json!({"temp": 25.0}),
            produced_at: chrono::Utc::now() - chrono::TimeDelta::milliseconds(100),
            trace_id: Uuid::nil(),
        },
    );

    let result = pull_data_inputs(
        "consumer",
        consumer.as_ref(),
        json!({}),
        &edges,
        &nodes,
        &caches,
        &timeouts,
        &memo,
        Uuid::nil(),
    )
    .await
    .unwrap();

    // TTL 过期 → DefaultValue(0) 兜底
    assert_eq!(result["sensor"], json!(0));
}

// ---- PureMemo + fan-out 测试 ----

/// 测试用 pure 节点：记录 transform 调用次数。
struct CountingPureNode {
    id: String,
    call_count: std::sync::atomic::AtomicU32,
}

impl CountingPureNode {
    fn new(id: &str) -> Self {
        Self {
            id: id.to_owned(),
            call_count: std::sync::atomic::AtomicU32::new(0),
        }
    }
    fn call_count(&self) -> u32 {
        self.call_count.load(std::sync::atomic::Ordering::Relaxed)
    }
}

#[async_trait::async_trait]
impl NodeTrait for CountingPureNode {
    fn id(&self) -> &str {
        &self.id
    }
    fn kind(&self) -> &'static str {
        "countingPure"
    }
    fn input_pins(&self) -> Vec<PinDefinition> {
        vec![PinDefinition {
            id: "in".to_owned(),
            label: "in".to_owned(),
            pin_type: PinType::Any,
            direction: PinDirection::Input,
            required: false,
            kind: PinKind::Data,
            description: None,
            empty_policy: EmptyPolicy::default(),
            block_timeout_ms: None,
            ttl_ms: None,
        }]
    }
    fn output_pins(&self) -> Vec<PinDefinition> {
        vec![PinDefinition {
            id: "out".to_owned(),
            label: "out".to_owned(),
            pin_type: PinType::Any,
            direction: PinDirection::Output,
            required: false,
            kind: PinKind::Data,
            description: None,
            empty_policy: EmptyPolicy::default(),
            block_timeout_ms: None,
            ttl_ms: None,
        }]
    }
    async fn transform(
        &self,
        _trace_id: Uuid,
        _payload: Value,
    ) -> Result<NodeExecution, EngineError> {
        self.call_count
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        Ok(NodeExecution::broadcast(json!({"out": 42})))
    }
}

/// Fan-out：同一 pure 节点被两个 Data 输入引脚拉取，transform 只执行一次。
#[tokio::test]
async fn fan_out_pure_节点只_transform_一次() {
    let pure_node = Arc::new(CountingPureNode::new("pure"));
    let pure_node_trait: Arc<dyn NodeTrait> = pure_node.clone();

    let mut nodes = HashMap::new();
    nodes.insert("pure".to_owned(), pure_node_trait);

    let caches = HashMap::new(); // pure 节点不需要 OutputCache
    let timeouts = HashMap::new();

    // 两条 Data 边从同一 pure 节点到同一 consumer 的两个输入 pin
    let e1 = data_edge("pure", Some("out"), "consumer", Some("a"));
    let e2 = data_edge("pure", Some("out"), "consumer", Some("b"));
    let refs = vec![&e1, &e2];
    let edges = super::super::index::build_edges_by_consumer(&refs);
    let memo = PureMemo::new();

    // Consumer 不需要实际 NodeTrait——只需 input_pins 返回带正确 empty_policy 的 pin。
    // 但 pull_data_inputs 需要 &dyn NodeTrait，用 StubExecNode。
    let consumer = Arc::new(StubExecNode {
        id: "consumer".to_owned(),
        input_pins: vec![
            PinDefinition::default_input(),
            PinDefinition {
                id: "a".to_owned(),
                label: "a".to_owned(),
                pin_type: PinType::Any,
                direction: PinDirection::Input,
                required: false,
                kind: PinKind::Data,
                description: None,
                empty_policy: EmptyPolicy::Skip,
                block_timeout_ms: None,
                ttl_ms: None,
            },
            PinDefinition {
                id: "b".to_owned(),
                label: "b".to_owned(),
                pin_type: PinType::Any,
                direction: PinDirection::Input,
                required: false,
                kind: PinKind::Data,
                description: None,
                empty_policy: EmptyPolicy::Skip,
                block_timeout_ms: None,
                ttl_ms: None,
            },
        ],
    }) as Arc<dyn NodeTrait>;

    let result = pull_data_inputs(
        "consumer",
        consumer.as_ref(),
        json!({}),
        &edges,
        &nodes,
        &caches,
        &timeouts,
        &memo,
        Uuid::nil(),
    )
    .await
    .unwrap();

    assert_eq!(result["a"], json!(42));
    assert_eq!(result["b"], json!(42));
    assert_eq!(
        pure_node.call_count(),
        1,
        "fan-out 下 pure 节点应只 transform 一次"
    );
}
