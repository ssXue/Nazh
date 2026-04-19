//! 调试输出节点，将 payload 格式化后打印到控制台。
//!
//! 支持紧凑和美化两种输出格式，附带 `_debug_console` 元数据。

use async_trait::async_trait;
use chrono::Utc;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};

use uuid::Uuid;

use nazh_core::{EngineError, NodeExecution, NodeTrait, into_payload_map};

fn default_debug_pretty() -> bool {
    true
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DebugConsoleNodeConfig {
    #[serde(default)]
    pub label: Option<String>,
    #[serde(default = "default_debug_pretty")]
    pub pretty: bool,
}

/// 调试输出节点。
pub struct DebugConsoleNode {
    id: String,
    config: DebugConsoleNodeConfig,
}

impl DebugConsoleNode {
    pub fn new(id: impl Into<String>, config: DebugConsoleNodeConfig) -> Self {
        Self {
            id: id.into(),
            config,
        }
    }
}

#[async_trait]
impl NodeTrait for DebugConsoleNode {
    nazh_core::impl_node_meta!("debugConsole");

    async fn transform(
        &self,
        _trace_id: Uuid,
        payload: Value,
    ) -> Result<NodeExecution, EngineError> {
        let label = self
            .config
            .label
            .as_deref()
            .filter(|label| !label.trim().is_empty())
            .unwrap_or("调试控制台");
        let rendered_payload = if self.config.pretty {
            serde_json::to_string_pretty(&payload)
        } else {
            serde_json::to_string(&payload)
        }
        .map_err(|error| EngineError::payload_conversion(self.id.clone(), error.to_string()))?;

        tracing::info!(
            node_id = %self.id,
            trace_id = %_trace_id,
            label,
            "调试控制台输出\n{}",
            rendered_payload
        );

        let payload_map = into_payload_map(payload);
        let metadata = Map::from_iter([(
            "debug_console".to_owned(),
            json!({
                "label": label,
                "pretty": self.config.pretty,
                "logged_at": Utc::now().to_rfc3339(),
            }),
        )]);

        Ok(NodeExecution::broadcast(Value::Object(payload_map)).with_metadata(metadata))
    }
}
