//! Data 输入拉取：递归求值 pure 节点、读 Exec 节点缓存、合并 payload。

use std::collections::HashMap;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::sync::Arc;
use std::time::Duration;

use chrono::Utc;
use nazh_core::{
    DEFAULT_BLOCK_TIMEOUT_MS, EmptyPolicy, EngineError, NodeTrait, OutputCache, Uuid,
    guard::guarded_execute, is_pure_form,
};
use serde_json::{Map, Value};
use tracing::Instrument;

use super::index::EdgesByConsumer;
use super::memo::PureMemo;

/// 在被 Exec 触发的下游节点 transform 之前，收集其 Data 输入引脚的最新值，
/// 并把它们合并进 transform payload。
///
/// 合并规则（Phase 3 约定，混合输入节点见 Phase 3b 决策）：
/// - 若 `exec_payload` 为 `Object`，把每个 Data pin 的值以 `pin.id` 为键插入
/// - 否则（标量、数组）payload 重写为 `{"in": exec_payload, <pin_id>: value, ...}`
///
/// 上游若为 pure-form 节点 → 调 [`pull_one`] 递归求值（Phase 4 加 [`PureMemo`] 缓存）。
/// 上游若为 Exec 节点 → 读其 [`OutputCache`] 槽（Phase 4 加 [`EmptyPolicy`] 兜底）。
///
/// `consumer_node` 用于读取该节点声明的 Data 输入引脚的 [`EmptyPolicy`]。
#[allow(clippy::too_many_arguments)]
pub(crate) async fn pull_data_inputs(
    consumer_node_id: &str,
    consumer_node: &dyn NodeTrait,
    exec_payload: Value,
    edges_by_consumer: &EdgesByConsumer,
    nodes_index: &HashMap<String, Arc<dyn NodeTrait>>,
    output_caches_index: &HashMap<String, Arc<OutputCache>>,
    node_timeouts_index: &HashMap<String, Option<Duration>>,
    pure_memo: &PureMemo,
    trace_id: Uuid,
) -> Result<Value, EngineError> {
    let entries = edges_by_consumer.for_consumer(consumer_node_id);
    if entries.is_empty() {
        return Ok(exec_payload);
    }

    // 构建 consumer 输入引脚 id → PinDefinition 查找表，用于读取 empty_policy / ttl
    let consumer_pins = consumer_node.input_pins();
    let consumer_pin_map: HashMap<&str, _> =
        consumer_pins.iter().map(|p| (p.id.as_str(), p)).collect();

    let mut data_values: Map<String, Value> = Map::new();
    for entry in entries {
        let (empty_policy, block_timeout_ms, ttl_ms) =
            match consumer_pin_map.get(entry.consumer_input_pin_id.as_str()) {
                Some(pin) => (pin.empty_policy.clone(), pin.block_timeout_ms, pin.ttl_ms),
                None => (EmptyPolicy::default(), None, None),
            };

        let upstream_value = pull_one(
            &entry.upstream_node_id,
            &entry.upstream_output_pin_id,
            nodes_index,
            output_caches_index,
            node_timeouts_index,
            edges_by_consumer,
            pure_memo,
            trace_id,
            &empty_policy,
            block_timeout_ms,
            ttl_ms,
        )
        .await?;
        data_values.insert(entry.consumer_input_pin_id.clone(), upstream_value);
    }

    Ok(merge_payload(exec_payload, data_values))
}

pub fn merge_payload(exec_payload: Value, data_values: Map<String, Value>) -> Value {
    match exec_payload {
        Value::Object(mut map) => {
            for (k, v) in data_values {
                map.insert(k, v);
            }
            Value::Object(map)
        }
        other => {
            let mut map = data_values;
            map.insert("in".to_owned(), other);
            Value::Object(map)
        }
    }
}

/// 从单个上游 (`node_id`, `pin_id`) 拉取一份 Data 值。
///
/// Phase 4 扩展：
/// - 纯函数上游：先查 [`PureMemo`]，命中直接返回；未命中则递归求值 + 存 memo
/// - Exec 上游：读 [`OutputCache`]，空/过期时按 [`EmptyPolicy`] 分支
#[allow(
    clippy::too_many_arguments,
    clippy::too_many_lines,
    clippy::ignored_unit_patterns
)]
fn pull_one<'a>(
    upstream_node_id: &'a str,
    upstream_output_pin_id: &'a str,
    nodes_index: &'a HashMap<String, Arc<dyn NodeTrait>>,
    output_caches_index: &'a HashMap<String, Arc<OutputCache>>,
    node_timeouts_index: &'a HashMap<String, Option<Duration>>,
    edges_by_consumer: &'a EdgesByConsumer,
    pure_memo: &'a PureMemo,
    trace_id: Uuid,
    empty_policy: &'a EmptyPolicy,
    block_timeout_ms: Option<u64>,
    ttl_ms: Option<u64>,
) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<Value, EngineError>> + Send + 'a>> {
    Box::pin(async move {
        let upstream = nodes_index.get(upstream_node_id).ok_or_else(|| {
            EngineError::invalid_graph(format!(
                "拉路径上游节点 `{upstream_node_id}` 在 nodes_index 缺失"
            ))
        })?;

        if is_pure_form(upstream.as_ref()) {
            // 递归：先收集 pure 上游自己的 Data 输入
            let upstream_payload = pull_data_inputs(
                upstream_node_id,
                upstream.as_ref(),
                Value::Object(Map::new()),
                edges_by_consumer,
                nodes_index,
                output_caches_index,
                node_timeouts_index,
                pure_memo,
                trace_id,
            )
            .await?;

            let ih = input_hash(&upstream_payload);

            // 命中 memo → 直接提取 pin 值
            if let Some(cached_payload) = pure_memo.get(upstream_node_id, trace_id, ih) {
                return extract_pin_from_payload(&cached_payload, upstream_output_pin_id)
                    .ok_or_else(|| EngineError::DataPinCacheEmpty {
                        upstream: upstream_node_id.to_owned(),
                        pin: upstream_output_pin_id.to_owned(),
                    });
            }

            let span = tracing::info_span!(
                "node.transform",
                node_id = %upstream_node_id,
                trace_id = %trace_id,
                pull = true,
            );
            let timeout = node_timeouts_index.get(upstream_node_id).copied().flatten();
            let result = guarded_execute(
                upstream_node_id,
                trace_id,
                timeout,
                upstream.transform(trace_id, upstream_payload.clone()),
            )
            .instrument(span)
            .await?;

            // 存 memo
            if let Some(first_output) = result.outputs.first() {
                pure_memo.insert(upstream_node_id, trace_id, ih, first_output.payload.clone());
            }

            // 找匹配 upstream_output_pin_id 的输出 payload
            for output in &result.outputs {
                if let Value::Object(map) = &output.payload
                    && let Some(v) = map.get(upstream_output_pin_id)
                {
                    return Ok(v.clone());
                }
            }
            // 兜底：若 pure 节点只有单输出且 payload 不是 `{pin_id: value}` 形态
            result
                .outputs
                .first()
                .map(|o| o.payload.clone())
                .ok_or_else(|| EngineError::DataPinCacheEmpty {
                    upstream: upstream_node_id.to_owned(),
                    pin: upstream_output_pin_id.to_owned(),
                })
        } else {
            // 非 pure：读 OutputCache，按 empty_policy 分支
            let cache = output_caches_index.get(upstream_node_id).ok_or_else(|| {
                EngineError::invalid_graph(format!(
                    "上游 Exec 节点 `{upstream_node_id}` 在 output_caches_index 缺失"
                ))
            })?;

            // 先尝试带 TTL 读
            if let Some(cached) = cache.read(upstream_output_pin_id, ttl_ms) {
                return Ok(cached.value);
            }

            // 缓存空 / 过期 → 按 empty_policy 兜底
            match empty_policy {
                EmptyPolicy::BlockUntilReady => {
                    let mut rx = cache.subscribe(upstream_output_pin_id).ok_or_else(|| {
                        EngineError::DataPinCacheEmpty {
                            upstream: upstream_node_id.to_owned(),
                            pin: upstream_output_pin_id.to_owned(),
                        }
                    })?;
                    let timeout_ms = block_timeout_ms.unwrap_or(DEFAULT_BLOCK_TIMEOUT_MS);
                    // watch: 先检查当前值
                    if let Some(cached) = rx.borrow().clone() {
                        let age = Utc::now()
                            .signed_duration_since(cached.produced_at)
                            .num_milliseconds();
                        if ttl_ms.is_none_or(|ttl| age.unsigned_abs() <= ttl) {
                            return Ok(cached.value);
                        }
                    }
                    // 等变更
                    let result = tokio::select! {
                        res = rx.changed() => {
                            match res {
                                Ok(()) => {
                                    let snapshot = rx.borrow().clone();
                                    match snapshot {
                                        Some(cached) => {
                                            if let Some(ttl) = ttl_ms {
                                                let age = Utc::now()
                                                    .signed_duration_since(cached.produced_at)
                                                    .num_milliseconds();
                                                if age.unsigned_abs() > ttl {
                                                    return Err(EngineError::DataPinPullTimeout {
                                                        upstream: upstream_node_id.to_owned(),
                                                        pin: upstream_output_pin_id.to_owned(),
                                                        timeout_ms,
                                                    });
                                                }
                                            }
                                            Ok(cached.value)
                                        }
                                        None => Err(EngineError::DataPinCacheEmpty {
                                            upstream: upstream_node_id.to_owned(),
                                            pin: upstream_output_pin_id.to_owned(),
                                        }),
                                    }
                                }
                                Err(_) => Err(EngineError::DataPinCacheEmpty {
                                    upstream: upstream_node_id.to_owned(),
                                    pin: upstream_output_pin_id.to_owned(),
                                }),
                            }
                        }
                        _ = tokio::time::sleep(Duration::from_millis(timeout_ms)) => {
                            // 超时前最后读一次
                            cache
                                .read(upstream_output_pin_id, ttl_ms)
                                .map(|c| c.value)
                                .ok_or_else(|| EngineError::DataPinPullTimeout {
                                    upstream: upstream_node_id.to_owned(),
                                    pin: upstream_output_pin_id.to_owned(),
                                    timeout_ms,
                                })
                        }
                    };
                    result
                }
                EmptyPolicy::DefaultValue(v) => Ok(v.clone()),
                EmptyPolicy::Skip => Ok(Value::Null),
            }
        }
    })
}

/// 计算 JSON `Value` 的确定性哈希，用于 [`PureMemo`] 键。
fn input_hash(v: &Value) -> u64 {
    let mut h = DefaultHasher::new();
    serde_json::to_string(v).unwrap_or_default().hash(&mut h);
    h.finish()
}

/// 从 pure 节点 transform 产出的 payload 中提取指定 pin 的值。
fn extract_pin_from_payload(payload: &Value, pin_id: &str) -> Option<Value> {
    if let Value::Object(map) = payload {
        map.get(pin_id).cloned()
    } else {
        None
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
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
        let (nodes, caches, timeouts, edges, memo, consumer) =
            setup_pull_test(EmptyPolicy::Skip, None);

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
}
