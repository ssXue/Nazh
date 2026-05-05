//! EtherCAT PDO 读取节点。
//!
//! 读取指定从站的输入 PDO 数据，输出为 JSON。

use connections::{SharedConnectionManager, connection_metadata};
use nazh_core::{
    EngineError, NodeExecution, NodeLifecycleContext, NodeTrait, PinDefinition, PinType,
};
use serde::{Deserialize, Serialize};
use serde_json::{Map, json};

use super::session::{self, EthercatRuntime};

fn default_slave_address() -> u16 {
    1
}

/// EtherCAT PDO 读取节点配置。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EthercatPdoReadConfig {
    /// 从站地址。
    #[serde(default = "default_slave_address", alias = "slaveAddress")]
    pub slave_address: u16,
    /// 连接 ID。
    #[serde(default, alias = "connectionId")]
    pub connection_id: Option<String>,
}

/// EtherCAT PDO 读取节点。
pub struct EthercatPdoReadNode {
    id: String,
    config: EthercatPdoReadConfig,
    connection_manager: SharedConnectionManager,
}

impl EthercatPdoReadNode {
    pub fn new(
        id: String,
        config: EthercatPdoReadConfig,
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
impl NodeTrait for EthercatPdoReadNode {
    fn id(&self) -> &str {
        &self.id
    }

    fn kind(&self) -> &'static str {
        "ethercatPdoRead"
    }

    fn output_pins(&self) -> Vec<PinDefinition> {
        vec![PinDefinition::output(
            PinType::Json,
            "EtherCAT 从站输入 PDO 数据（slave / inputs）",
        )]
    }

    async fn transform(
        &self,
        trace_id: uuid::Uuid,
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

        let inputs = bus
            .read_inputs(self.config.slave_address)
            .await
            .map_err(|e| EngineError::node_config(self.id.clone(), e.to_string()))?;

        let output = serde_json::json!({
            "slave": self.config.slave_address,
            "inputs": inputs,
            "traceId": trace_id,
        });
        let mut metadata = Map::new();
        metadata.insert(
            "ethercat".to_owned(),
            json!({
                "operation": "pdo-read",
                "slave": self.config.slave_address,
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
    fn 配置兼容前端_snake_case() {
        let config: EthercatPdoReadConfig =
            serde_json::from_value(json!({ "slave_address": 3 })).unwrap();
        assert_eq!(config.slave_address, 3);
    }

    #[test]
    fn 配置兼容_camel_case() {
        let config: EthercatPdoReadConfig =
            serde_json::from_value(json!({ "slaveAddress": 4 })).unwrap();
        assert_eq!(config.slave_address, 4);
    }
}
