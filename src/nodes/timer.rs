//! 定时触发节点，按固定间隔生成包含计时元数据的上下文。

use async_trait::async_trait;
use chrono::Utc;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use super::helpers::into_payload_map;
use super::{NodeExecution, NodeTrait};
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
    fn id(&self) -> &str {
        &self.id
    }

    fn kind(&self) -> &'static str {
        "timer"
    }

    fn ai_description(&self) -> &str {
        &self.ai_description
    }

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
        timer_meta.insert("node_id".to_owned(), Value::String(self.id.clone()));
        timer_meta.insert(
            "interval_ms".to_owned(),
            Value::from(self.config.interval_ms.max(1)),
        );
        timer_meta.insert("immediate".to_owned(), Value::Bool(self.config.immediate));
        timer_meta.insert(
            "triggered_at".to_owned(),
            Value::String(Utc::now().to_rfc3339()),
        );
        payload_map.insert("_timer".to_owned(), Value::Object(timer_meta));

        Ok(NodeExecution::broadcast(WorkflowContext::from_parts(
            ctx.trace_id,
            Utc::now(),
            Value::Object(payload_map),
        )))
    }
}
