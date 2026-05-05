//! EtherCAT 从站状态查询节点。
//!
//! 查询所有从站的 AL 状态、在线状态和 PDI 大小。

use connections::{SharedConnectionManager, connection_metadata};
use nazh_core::{
    EngineError, NodeExecution, NodeLifecycleContext, NodeTrait, PinDefinition, PinType,
};
use serde::{Deserialize, Serialize};
use serde_json::{Map, json};

use super::session::{self, EthercatRuntime};

/// EtherCAT 状态查询节点配置。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EthercatStatusConfig {
    /// 连接 ID。
    #[serde(default, alias = "connectionId")]
    pub connection_id: Option<String>,
}

/// EtherCAT 从站状态查询节点。
pub struct EthercatStatusNode {
    id: String,
    config: EthercatStatusConfig,
    connection_manager: SharedConnectionManager,
}

impl EthercatStatusNode {
    pub fn new(
        id: String,
        config: EthercatStatusConfig,
        connection_manager: SharedConnectionManager,
    ) -> Self {
        Self {
            id,
            config,
            connection_manager,
        }
    }
}

#[async_trait::async_trait]
impl NodeTrait for EthercatStatusNode {
    fn id(&self) -> &str {
        &self.id
    }

    fn kind(&self) -> &'static str {
        "ethercatStatus"
    }

    fn output_pins(&self) -> Vec<PinDefinition> {
        vec![PinDefinition::output(
            PinType::Json,
            "EtherCAT 从站状态列表（slaves / channelInfo）",
        )]
    }

    async fn transform(
        &self,
        _trace_id: uuid::Uuid,
        _payload: serde_json::Value,
    ) -> Result<NodeExecution, EngineError> {
        let conn_id = self.config.connection_id.as_deref().ok_or_else(|| {
            EngineError::node_config(self.id.clone(), "EtherCAT 连接 ID 未配置".to_owned())
        })?;

        let runtime = EthercatRuntime::new(self.connection_manager.clone(), conn_id.to_owned());
        let session = runtime.ensure_session(&self.id).await?;

        let guard = session.bus(&self.id).await?;
        let bus = guard.as_ref().ok_or(EngineError::node_config(
            self.id.clone(),
            "EtherCAT 总线会话已释放".to_owned(),
        ))?;

        let states = bus.get_slave_states();
        let output = serde_json::json!({
            "slaves": states,
            "channelInfo": bus.channel_info(),
        });
        let mut metadata = Map::new();
        metadata.insert(
            "ethercat".to_owned(),
            json!({
                "operation": "status",
                "slave_count": states.len(),
                "channel_info": bus.channel_info(),
            }),
        );
        if let Some(lease) = session.lease() {
            let (key, value) = connection_metadata(&self.id, lease)?;
            metadata.insert(key, value);
        }

        Ok(NodeExecution::broadcast(output).with_metadata(Some(metadata)))
    }

    async fn on_deploy(
        &self,
        ctx: NodeLifecycleContext,
    ) -> Result<nazh_core::LifecycleGuard, EngineError> {
        if let Some(conn_id) = &self.config.connection_id {
            let runtime = EthercatRuntime::new(self.connection_manager.clone(), conn_id.clone());
            let _ = runtime.ensure_session(&self.id).await?;

            Ok(session::lifecycle_guard(
                ctx,
                self.connection_manager.clone(),
                conn_id.clone(),
            ))
        } else {
            Ok(nazh_core::LifecycleGuard::noop())
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn 配置兼容_camel_case_connection_id() {
        let config: EthercatStatusConfig =
            serde_json::from_value(json!({ "connectionId": "ecat0" })).unwrap();
        assert_eq!(config.connection_id.as_deref(), Some("ecat0"));
    }
}
