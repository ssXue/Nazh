//! 纯 Rust 原生节点，负责协议 I/O、数据注入和连接管理。
//!
//! 若设置了 `connection_id`，节点会从全局 [`ConnectionManager`](crate::ConnectionManager)
//! 借出连接，将注入字段和连接元数据写入 payload，执行完毕后释放连接。

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use super::helpers::{insert_connection_lease, into_payload_map};
use super::{NodeExecution, NodeTrait};
use crate::{ConnectionGuard, ContextRef, DataStore, EngineError, SharedConnectionManager};

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
        trace_id: uuid::Uuid,
        payload: Value,
        guard: Option<&ConnectionGuard>,
    ) -> Result<Value, EngineError> {
        let mut payload_map = into_payload_map(payload);

        if let Some(message) = &self.config.message {
            payload_map.insert("_native_message".to_owned(), Value::String(message.clone()));
        }

        for (key, value) in &self.config.inject {
            payload_map.insert(key.clone(), value.clone());
        }

        if let Some(guard) = guard {
            insert_connection_lease(&self.id, &mut payload_map, guard.lease())?;
        }

        tracing::info!(
            node_id = %self.id,
            trace_id = %trace_id,
            message = %self.config.message.as_deref().unwrap_or("透传"),
            "原生节点执行"
        );

        Ok(Value::Object(payload_map))
    }
}

#[async_trait]
impl NodeTrait for NativeNode {
    impl_node_meta!("native");

    async fn execute(&self, ctx: &ContextRef, store: &dyn DataStore) -> Result<NodeExecution, EngineError> {
        let payload = store.read_mut(&ctx.data_id)?;
        let mut guard = if let Some(conn_id) = &self.config.connection_id {
            Some(self.connection_manager.acquire(conn_id).await?)
        } else {
            None
        };
        let result = self.build_payload(ctx.trace_id, payload, guard.as_ref())?;
        if let Some(g) = &mut guard {
            g.mark_success();
        }
        Ok(NodeExecution::broadcast(result))
    }
}
