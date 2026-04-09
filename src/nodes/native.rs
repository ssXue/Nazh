//! 纯 Rust 原生节点，负责协议 I/O、数据注入和连接管理。
//!
//! 若设置了 `connection_id`，节点会从全局 [`ConnectionManager`](crate::ConnectionManager)
//! 借出连接，将注入字段和连接元数据写入 payload，执行完毕后释放连接。

use async_trait::async_trait;
use chrono::Utc;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use super::helpers::{insert_connection_lease, into_payload_map, with_connection};
use super::{NodeExecution, NodeTrait};
use crate::{ConnectionLease, EngineError, SharedConnectionManager, WorkflowContext};

/// [`NativeNode`] 的配置。
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct NativeNodeConfig {
    #[serde(default)]
    pub message: Option<String>,
    #[serde(default)]
    pub inject: Map<String, Value>,
    #[serde(default)]
    pub connection_id: Option<String>,
}

/// 纯 Rust 原生节点。
pub struct NativeNode {
    id: String,
    ai_description: String,
    config: NativeNodeConfig,
    connection_manager: SharedConnectionManager,
}

impl NativeNode {
    pub fn new(
        id: impl Into<String>,
        config: NativeNodeConfig,
        ai_description: impl Into<String>,
        connection_manager: SharedConnectionManager,
    ) -> Self {
        Self {
            id: id.into(),
            ai_description: ai_description.into(),
            config,
            connection_manager,
        }
    }

    fn build_payload(
        &self,
        ctx: WorkflowContext,
        lease: Option<&ConnectionLease>,
    ) -> Result<WorkflowContext, EngineError> {
        let mut payload_map = into_payload_map(ctx.payload);

        if let Some(message) = &self.config.message {
            payload_map.insert("_native_message".to_owned(), Value::String(message.clone()));
        }

        for (key, value) in &self.config.inject {
            payload_map.insert(key.clone(), value.clone());
        }

        if let Some(lease) = lease {
            insert_connection_lease(&self.id, &mut payload_map, lease)?;
        }

        println!(
            "[native:{}] trace_id={} message={}",
            self.id,
            ctx.trace_id,
            self.config.message.as_deref().unwrap_or("透传"),
        );

        Ok(WorkflowContext::from_parts(
            ctx.trace_id,
            Utc::now(),
            Value::Object(payload_map),
        ))
    }
}

#[async_trait]
impl NodeTrait for NativeNode {
    fn id(&self) -> &str {
        &self.id
    }

    fn kind(&self) -> &'static str {
        "native"
    }

    fn ai_description(&self) -> &str {
        &self.ai_description
    }

    async fn execute(&self, ctx: WorkflowContext) -> Result<NodeExecution, EngineError> {
        let result = with_connection(
            &self.connection_manager,
            self.config.connection_id.as_deref(),
            |lease| self.build_payload(ctx, lease),
        )
        .await?;
        Ok(NodeExecution::broadcast(result))
    }
}
