//! 定时触发节点，按固定间隔生成包含计时元数据的上下文。

use async_trait::async_trait;
use chrono::Utc;
use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};

use super::helpers::into_payload_map;
#[allow(unused_imports)] // clippy 无法追踪 macro_rules! 宏的使用
use super::{impl_node_meta, NodeExecution, NodeTrait};
use crate::{EngineError, WorkflowContext};

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
    impl_node_meta!("timer");

    async fn execute(&self, ctx: WorkflowContext) -> Result<NodeExecution, EngineError> {
        let mut payload_map = into_payload_map(ctx.payload);

        for (key, value) in &self.config.inject {
            payload_map.insert(key.clone(), value.clone());
        }

        let existing_timer = payload_map
            .remove("_timer")
            .and_then(|value| match value {
                Value::Object(map) => Some(map),
                _ => None,
            })
            .unwrap_or_default();
        let mut timer_meta = existing_timer;
        timer_meta.insert("node_id".to_owned(), json!(self.id));
        timer_meta.insert(
            "interval_ms".to_owned(),
            json!(self.config.interval_ms.max(1)),
        );
        timer_meta.insert("immediate".to_owned(), json!(self.config.immediate));
        timer_meta.insert("triggered_at".to_owned(), json!(Utc::now().to_rfc3339()));
        payload_map.insert("_timer".to_owned(), Value::Object(timer_meta));

        Ok(NodeExecution::broadcast(WorkflowContext::from_parts(
            ctx.trace_id,
            Utc::now(),
            Value::Object(payload_map),
        )))
    }
}
