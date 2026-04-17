//! 定时触发节点，按固定间隔生成包含计时元数据的上下文。

use async_trait::async_trait;
use chrono::Utc;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};
use uuid::Uuid;

use nazh_core::EngineError;
use nazh_core::into_payload_map;
use nazh_core::{NodeExecution, NodeTrait};

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

/// 定时触发节点，将 `_timer` 元数据和自定义注入字段写入 payload。
pub struct TimerNode {
    id: String,
    ai_description: String,
    config: TimerNodeConfig,
}

impl TimerNode {
    pub fn new(
        id: impl Into<String>,
        config: TimerNodeConfig,
        ai_description: impl Into<String>,
    ) -> Self {
        Self {
            id: id.into(),
            ai_description: ai_description.into(),
            config,
        }
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

        let metadata = Map::from_iter([(
            "timer".to_owned(),
            json!({
                "node_id": self.id,
                "interval_ms": self.config.interval_ms.max(1),
                "immediate": self.config.immediate,
                "triggered_at": Utc::now().to_rfc3339(),
            }),
        )]);

        Ok(NodeExecution::broadcast(Value::Object(payload_map)).with_metadata(metadata))
    }
}
