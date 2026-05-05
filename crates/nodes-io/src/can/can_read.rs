//! CAN 帧接收节点。
//!
//! 通过 SLCAN 适配器接收单帧 CAN 数据，将帧内容转换为 JSON payload。
//! 无连接时自动回退到 `MockBackend` 生成模拟帧。

use async_trait::async_trait;
use chrono::Utc;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};
use uuid::Uuid;

use connections::{SharedConnectionManager, connection_metadata};
use nazh_core::{EngineError, NodeExecution, NodeTrait, PinDefinition, PinType, into_payload_map};

use crate::can::{CanBusConfig, CanFilter, create_can_bus, hex, validate_can_id};

/// CAN 读节点配置。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CanReadNodeConfig {
    /// 连接 ID（引用 `ConnectionManager` 中的连接定义）。
    #[serde(default)]
    pub connection_id: Option<String>,
    /// 可选：只接收指定 CAN ID 的帧。
    #[serde(default)]
    pub can_id: Option<u32>,
    /// 目标帧是否为扩展帧（29-bit）。
    #[serde(default)]
    pub is_extended: bool,
    /// 接收超时（毫秒），默认 1000。
    #[serde(default = "default_timeout_ms")]
    pub timeout_ms: u64,
}

fn default_timeout_ms() -> u64 {
    1000
}

/// CAN 帧接收节点。
pub struct CanReadNode {
    id: String,
    config: CanReadNodeConfig,
    connection_manager: SharedConnectionManager,
}

impl CanReadNode {
    pub fn new(
        id: impl Into<String>,
        config: CanReadNodeConfig,
        connection_manager: SharedConnectionManager,
    ) -> Self {
        Self {
            id: id.into(),
            config,
            connection_manager,
        }
    }
}

#[async_trait]
impl NodeTrait for CanReadNode {
    nazh_core::impl_node_meta!("canRead");

    fn output_pins(&self) -> Vec<PinDefinition> {
        vec![PinDefinition::output(
            PinType::Json,
            "接收到的 CAN 帧（id / data / dlc / is_extended / timestamp）",
        )]
    }

    async fn transform(
        &self,
        trace_id: Uuid,
        payload: Value,
    ) -> Result<NodeExecution, EngineError> {
        let mut guard = if let Some(conn_id) = &self.config.connection_id {
            Some(self.connection_manager.acquire(conn_id).await?)
        } else {
            None
        };

        // 构建 CAN 总线配置
        let mut bus_config = if let Some(ref mut g) = guard {
            CanBusConfig::from_metadata(g.metadata())
                .map_err(|e| EngineError::node_config(self.id.clone(), e.to_string()))?
        } else {
            // 无连接时回退到 Mock
            CanBusConfig {
                interface: "mock".to_owned(),
                channel: "mock-can".to_owned(),
                baud_rate: 115_200,
                bitrate: 500_000,
                filters: Vec::new(),
                fd: false,
                receive_own_messages: false,
            }
        };
        // 节点级 can_id 追加到连接级过滤器；Mock 回退也必须遵守同一筛选语义。
        if let Some(can_id) = self.config.can_id {
            validate_can_id(can_id, self.config.is_extended)
                .map_err(|e| EngineError::node_config(self.id.clone(), e.to_string()))?;
            let filter = if self.config.is_extended {
                CanFilter::extended(can_id, 0x1FFF_FFFF)
            } else {
                CanFilter::standard(can_id, 0x7FF)
            };
            bus_config.filters.push(filter);
        }

        // 创建后端并接收帧
        let bus = create_can_bus(&bus_config)
            .await
            .map_err(|e| EngineError::stage_execution(self.id.clone(), trace_id, e.to_string()))?;

        if !bus_config.filters.is_empty() {
            bus.set_filters(&bus_config.filters).map_err(|e| {
                EngineError::stage_execution(self.id.clone(), trace_id, e.to_string())
            })?;
        }

        let timeout = std::time::Duration::from_millis(self.config.timeout_ms);
        let frame = bus
            .recv(timeout)
            .await
            .map_err(|e| EngineError::stage_execution(self.id.clone(), trace_id, e.to_string()))?;

        // 构建 payload
        let mut payload_map = into_payload_map(payload);
        if let Some(ref f) = frame {
            payload_map.insert(
                "can".to_owned(),
                json!({
                    "id": f.id,
                    "id_hex": format!("0x{:03X}", f.id),
                    "data": f.data,
                    "data_hex": hex::encode(&f.data).to_ascii_uppercase(),
                    "dlc": f.dlc,
                    "is_extended": f.is_extended,
                    "is_remote": f.is_remote,
                    "timestamp": f.timestamp.map(|t| t.to_rfc3339()),
                }),
            );
        } else {
            payload_map.insert("can".to_owned(), Value::Null);
        }

        // 构建 metadata
        let mut can_meta = Map::from_iter([
            ("simulated".to_owned(), json!(guard.is_none())),
            ("channel_info".to_owned(), json!(bus.channel_info())),
            ("sampled_at".to_owned(), json!(Utc::now().to_rfc3339())),
        ]);

        if let Some(ref mut g) = guard {
            let (key, value) = connection_metadata(&self.id, g.lease())?;
            can_meta.insert(key, value);
            g.mark_success();
        }

        let metadata = Map::from_iter([("can".to_owned(), Value::Object(can_meta))]);
        Ok(NodeExecution::broadcast(Value::Object(payload_map)).with_metadata(Some(metadata)))
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use connections::shared_connection_manager;

    fn make_node(config: CanReadNodeConfig) -> CanReadNode {
        CanReadNode::new("can-read-1", config, shared_connection_manager())
    }

    #[test]
    fn output_pin_是_json() {
        let node = make_node(CanReadNodeConfig {
            connection_id: None,
            can_id: None,
            is_extended: false,
            timeout_ms: 1000,
        });

        let pins = node.output_pins();
        assert_eq!(pins.len(), 1);
        assert_eq!(pins[0].id, "out");
        assert_eq!(pins[0].pin_type, PinType::Json);
    }

    #[tokio::test]
    async fn 标准帧_can_id_超限会失败() {
        let node = make_node(CanReadNodeConfig {
            connection_id: None,
            can_id: Some(0x800),
            is_extended: false,
            timeout_ms: 1,
        });

        let err = node
            .transform(Uuid::new_v4(), Value::Null)
            .await
            .unwrap_err();
        assert!(matches!(err, EngineError::NodeConfig { .. }));
    }
}
