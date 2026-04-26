//! 定时触发节点，按固定间隔生成包含计时元数据的上下文。
//!
//! ## 触发模式
//!
//! Timer 是触发器节点的最简范本：[`on_deploy`] 中 spawn 后台任务，按
//! `interval_ms` 调用 [`NodeHandle::emit`] 推数据进 DAG；[`LifecycleGuard`]
//! 通过 `CancellationToken` 在撤销时通知后台任务退出。
//!
//! `transform` 路径仍可被手动 dispatch 调用并得到等价输出——两条路径共用
//! [`TimerNode::trigger_payload`] / [`timer_metadata`] helper，确保 payload
//! 含 inject 字段、`metadata.timer` 含 `node_id` / `interval_ms` /
//! `immediate` / `triggered_at`。
//!
//! ## 背压策略说明
//!
//! emit 走 `NodeHandle` 而非 `WorkflowDispatchRouter` 的 trigger lane；
//! 后者带的 backpressure / 死信队列 / 重试 / metrics 在本节点不生效。timer
//! 触发规律可控（按固定 interval），DLQ / retry 几乎无触发场景。引擎级背压
//! 能力规划见 ADR-0014 / ADR-0016。
//!
//! [`on_deploy`]: NodeTrait::on_deploy
//! [`NodeHandle::emit`]: nazh_core::NodeHandle::emit
//! [`LifecycleGuard`]: nazh_core::LifecycleGuard
//! [`timer_metadata`]: TimerNode::timer_metadata

use std::sync::Arc;

use async_trait::async_trait;
use chrono::Utc;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};
use tokio::time::Duration;

use uuid::Uuid;

use nazh_core::{
    EngineError, LifecycleGuard, NodeExecution, NodeLifecycleContext, NodeTrait, into_payload_map,
};

fn default_timer_interval_ms() -> u64 {
    5_000
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimerNodeConfig {
    #[serde(default = "default_timer_interval_ms")]
    pub interval_ms: u64,
    #[serde(default)]
    pub immediate: bool,
    #[serde(default)]
    pub inject: Map<String, Value>,
}

/// 定时触发节点，将 `timer` 元数据和自定义注入字段写入 payload。
pub struct TimerNode {
    id: String,
    config: TimerNodeConfig,
}

impl TimerNode {
    pub fn new(id: impl Into<String>, config: TimerNodeConfig) -> Self {
        Self {
            id: id.into(),
            config,
        }
    }

    /// 触发器构造的 base payload（不含上游数据，只含 inject 字段）。
    fn trigger_payload(&self) -> Value {
        let mut payload_map = Map::new();
        for (key, value) in &self.config.inject {
            payload_map.insert(key.clone(), value.clone());
        }
        Value::Object(payload_map)
    }

    /// 时间触发元数据，emit 路径与 transform 路径共用。
    fn timer_metadata(&self) -> Map<String, Value> {
        Map::from_iter([(
            "timer".to_owned(),
            json!({
                "node_id": self.id,
                "interval_ms": self.config.interval_ms.max(1),
                "immediate": self.config.immediate,
                "triggered_at": Utc::now().to_rfc3339(),
            }),
        )])
    }
}

#[async_trait]
impl NodeTrait for TimerNode {
    nazh_core::impl_node_meta!("timer");

    async fn transform(
        &self,
        _trace_id: Uuid,
        payload: Value,
    ) -> Result<NodeExecution, EngineError> {
        let mut payload_map = into_payload_map(payload);

        for (key, value) in &self.config.inject {
            payload_map.insert(key.clone(), value.clone());
        }

        Ok(NodeExecution::broadcast(Value::Object(payload_map))
            .with_metadata(self.timer_metadata()))
    }

    async fn on_deploy(
        &self,
        ctx: NodeLifecycleContext,
    ) -> Result<LifecycleGuard, EngineError> {
        let interval = Duration::from_millis(self.config.interval_ms.max(1));
        let immediate = self.config.immediate;
        let handle = ctx.handle.clone();
        let token = ctx.shutdown.clone();

        // 把节点克隆进任务以便每次 tick 都能调用 trigger_payload / timer_metadata。
        // TimerNode 内部都是 Clone-friendly 字段，clone 廉价。
        let node = Arc::new(TimerNode {
            id: self.id.clone(),
            config: self.config.clone(),
        });

        let join = tokio::spawn(async move {
            // immediate 模式：先发一次再进入循环。监听 cancel 是为了 deploy
            // 失败的极短窗口里及时退出。
            if immediate
                && !token.is_cancelled()
                && let Err(error) = handle.emit(node.trigger_payload(), node.timer_metadata()).await
            {
                tracing::warn!(node_id = %node.id, ?error, "timer immediate emit 失败");
            }

            loop {
                tokio::select! {
                    biased;
                    () = token.cancelled() => break,
                    () = tokio::time::sleep(interval) => {
                        if let Err(error) =
                            handle.emit(node.trigger_payload(), node.timer_metadata()).await
                        {
                            tracing::warn!(node_id = %node.id, ?error, "timer tick emit 失败");
                        }
                    }
                }
            }
        });

        Ok(LifecycleGuard::from_task(ctx.shutdown, join))
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn trigger_payload_含_inject_字段() {
        let mut inject = Map::new();
        inject.insert("source".to_owned(), json!("test"));
        inject.insert("value".to_owned(), json!(42));
        let node = TimerNode::new(
            "t1",
            TimerNodeConfig {
                interval_ms: 100,
                immediate: false,
                inject,
            },
        );
        let payload = node.trigger_payload();
        let obj = payload.as_object().unwrap();
        assert_eq!(obj.get("source"), Some(&json!("test")));
        assert_eq!(obj.get("value"), Some(&json!(42)));
    }

    #[test]
    fn timer_metadata_含必要字段() {
        let node = TimerNode::new(
            "tick",
            TimerNodeConfig {
                interval_ms: 250,
                immediate: true,
                inject: Map::new(),
            },
        );
        let metadata = node.timer_metadata();
        let timer = metadata.get("timer").unwrap().as_object().unwrap();
        assert_eq!(timer.get("node_id"), Some(&json!("tick")));
        assert_eq!(timer.get("interval_ms"), Some(&json!(250_u64)));
        assert_eq!(timer.get("immediate"), Some(&json!(true)));
        assert!(timer.contains_key("triggered_at"));
    }

    #[test]
    fn interval_ms_最小为_1() {
        let node = TimerNode::new(
            "z",
            TimerNodeConfig {
                interval_ms: 0,
                immediate: false,
                inject: Map::new(),
            },
        );
        let metadata = node.timer_metadata();
        let timer = metadata.get("timer").unwrap().as_object().unwrap();
        assert_eq!(timer.get("interval_ms"), Some(&json!(1_u64)));
    }
}
