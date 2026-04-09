//! 调试输出节点，将 payload 格式化后打印到控制台。
//!
//! 支持紧凑和美化两种输出格式，附带 `_debug_console` 元数据。

use async_trait::async_trait;
use chrono::Utc;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use super::helpers::into_payload_map;
#[allow(unused_imports)] // clippy 无法追踪 macro_rules! 宏的使用
use super::{impl_node_meta, NodeExecution, NodeTrait};
use crate::{EngineError, WorkflowContext};

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
    ai_description: String,
    config: DebugConsoleNodeConfig,
}

impl DebugConsoleNode {
    pub fn new(
        id: impl Into<String>,
        config: DebugConsoleNodeConfig,
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
impl NodeTrait for DebugConsoleNode {
    impl_node_meta!("debugConsole");

    async fn execute(&self, ctx: WorkflowContext) -> Result<NodeExecution, EngineError> {
        let label = self
            .config
            .label
            .as_deref()
            .filter(|label| !label.trim().is_empty())
            .unwrap_or("调试控制台");
        let rendered_payload = if self.config.pretty {
            serde_json::to_string_pretty(&ctx.payload)
        } else {
            serde_json::to_string(&ctx.payload)
        }
        .map_err(|error| EngineError::payload_conversion(self.id.clone(), error.to_string()))?;

        println!(
            "[debug-console:{}] trace_id={} label={}\n{}",
            self.id, ctx.trace_id, label, rendered_payload
        );

        let trace_id = ctx.trace_id;
        let mut payload_map = into_payload_map(ctx.payload);
        payload_map.insert(
            "_debug_console".to_owned(),
            json!({
                "label": label,
                "pretty": self.config.pretty,
                "logged_at": Utc::now().to_rfc3339(),
            }),
        );

        Ok(NodeExecution::broadcast(WorkflowContext::from_parts(
            trace_id,
            Utc::now(),
            Value::Object(payload_map),
        )))
    }
}
