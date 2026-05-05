//! EtherCAT PDO 写入节点。
//!
//! 写入指定从站的输出 PDO 数据。

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

/// EtherCAT PDO 写入节点配置。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EthercatPdoWriteConfig {
    /// 从站地址。
    #[serde(default = "default_slave_address", alias = "slaveAddress")]
    pub slave_address: u16,
    /// 连接 ID。
    #[serde(default, alias = "connectionId")]
    pub connection_id: Option<String>,
}

/// EtherCAT PDO 写入节点。
pub struct EthercatPdoWriteNode {
    id: String,
    config: EthercatPdoWriteConfig,
    connection_manager: SharedConnectionManager,
}

impl EthercatPdoWriteNode {
    pub fn new(
        id: String,
        config: EthercatPdoWriteConfig,
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
impl NodeTrait for EthercatPdoWriteNode {
    fn id(&self) -> &str {
        &self.id
    }

    fn kind(&self) -> &'static str {
        "ethercatPdoWrite"
    }

    fn input_pins(&self) -> Vec<PinDefinition> {
        vec![PinDefinition::required_input(
            PinType::Json,
            "PDO 输出数据（data 数组: [0x01, 0x02, ...]）",
        )]
    }

    fn output_pins(&self) -> Vec<PinDefinition> {
        vec![PinDefinition::output(
            PinType::Json,
            "EtherCAT PDO 写入结果（slave / data / bytesWritten）",
        )]
    }

    async fn transform(
        &self,
        _trace_id: uuid::Uuid,
        payload: serde_json::Value,
    ) -> Result<NodeExecution, EngineError> {
        let conn_id = self.config.connection_id.as_deref().ok_or_else(|| {
            EngineError::node_config(self.id.clone(), "EtherCAT 连接 ID 未配置".to_owned())
        })?;

        // 从 payload 中提取输出数据
        let data = payload
            .get("data")
            .and_then(|v| v.as_array())
            .ok_or_else(|| {
                EngineError::node_config(
                    self.id.clone(),
                    "PDO 写入数据缺失或格式错误（需要 data 数组）".to_owned(),
                )
            })?;

        let bytes: Vec<u8> = data
            .iter()
            .map(|v| {
                v.as_u64()
                    .ok_or_else(|| {
                        EngineError::node_config(
                            self.id.clone(),
                            "PDO 数据元素必须为 0-255 整数".to_owned(),
                        )
                    })
                    .and_then(|n| {
                        u8::try_from(n).map_err(|_| {
                            EngineError::node_config(
                                self.id.clone(),
                                format!("PDO 数据元素 {n} 超出 0-255 范围"),
                            )
                        })
                    })
            })
            .collect::<Result<Vec<_>, _>>()?;

        let runtime = EthercatRuntime::new(self.connection_manager.clone(), conn_id.to_owned());
        let session = runtime.ensure_session(&self.id).await?;

        let guard = session.bus(&self.id).await?;
        let bus = guard.as_ref().ok_or(EngineError::node_config(
            self.id.clone(),
            "EtherCAT 总线会话已释放".to_owned(),
        ))?;

        bus.write_outputs(self.config.slave_address, &bytes)
            .await
            .map_err(|e| EngineError::node_config(self.id.clone(), e.to_string()))?;

        let bytes_written = bytes.len();
        let output = json!({
            "slave": self.config.slave_address,
            "data": bytes,
            "bytesWritten": bytes_written,
        });
        let mut metadata = Map::new();
        metadata.insert(
            "ethercat".to_owned(),
            json!({
                "operation": "pdo-write",
                "slave": self.config.slave_address,
                "bytes_written": bytes_written,
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
        let config: EthercatPdoWriteConfig =
            serde_json::from_value(json!({ "slave_address": 2 })).unwrap();
        assert_eq!(config.slave_address, 2);
    }

    #[test]
    fn 输出_pin_是_json() {
        let node = EthercatPdoWriteNode::new(
            "ecat_write".to_owned(),
            EthercatPdoWriteConfig {
                slave_address: 1,
                connection_id: None,
            },
            connections::shared_connection_manager(),
        );
        assert_eq!(node.output_pins()[0].pin_type, PinType::Json);
    }
}
