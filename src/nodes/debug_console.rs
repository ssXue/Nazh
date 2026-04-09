//! 调试输出节点，将 payload 格式化后打印到控制台。
//!
//! 支持紧凑和美化两种输出格式，附带 `_debug_console` 元数据。

use async_trait::async_trait;
use chrono::Utc;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use super::helpers::into_payload_map;
use super::{NodeExecution, NodeTrait};
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
    fn id(&self) -> &str {
        &self.id
    }

    fn kind(&self) -> &'static str {
        "debugConsole"
    }

    fn ai_description(&self) -> &str {
        &self.ai_description
    }

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
        let mut debug_meta = Map::new();
        debug_meta.insert("label".to_owned(), Value::String(label.to_owned()));
        debug_meta.insert("pretty".to_owned(), Value::Bool(self.config.pretty));
        debug_meta.insert(
            "logged_at".to_owned(),
            Value::String(Utc::now().to_rfc3339()),
        );
        payload_map.insert("_debug_console".to_owned(), Value::Object(debug_meta));

        Ok(NodeExecution::broadcast(WorkflowContext::from_parts(
            trace_id,
            Utc::now(),
            Value::Object(payload_map),
        )))
    }
}
