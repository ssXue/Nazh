//! CAN 帧发送节点。
//!
//! 通过 SLCAN 适配器发送单帧 CAN 数据。
//! 帧 ID 和 data 可从 payload 动态提取，或由 config 中的默认值覆盖。
//! 无连接时走 MockBackend，记录发送但不实际操作硬件。

use async_trait::async_trait;
use chrono::Utc;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};
use uuid::Uuid;

use connections::{SharedConnectionManager, connection_metadata};
use nazh_core::{EngineError, NodeExecution, NodeTrait, PinDefinition, PinType, into_payload_map};

use crate::can::{CanBusConfig, CanFrame, create_can_bus, hex, validate_can_id};

/// CAN 写节点配置。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CanWriteNodeConfig {
    /// 连接 ID（引用 `ConnectionManager` 中的连接定义）。
    #[serde(default)]
    pub connection_id: Option<String>,
    /// 默认帧 ID（可被 payload 中的 `can_id` 覆盖）。
    #[serde(default)]
    pub can_id: Option<u32>,
    /// 默认帧是否为扩展帧（29-bit）。
    #[serde(default)]
    pub is_extended: bool,
}

/// CAN 帧发送节点。
pub struct CanWriteNode {
    id: String,
    config: CanWriteNodeConfig,
    connection_manager: SharedConnectionManager,
}

impl CanWriteNode {
    pub fn new(
        id: impl Into<String>,
        config: CanWriteNodeConfig,
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
impl NodeTrait for CanWriteNode {
    nazh_core::impl_node_meta!("canWrite");

    fn input_pins(&self) -> Vec<PinDefinition> {
        vec![PinDefinition::required_input(
            PinType::Json,
            "待发送 CAN 帧参数（can_id / data / is_extended）",
        )]
    }

    fn output_pins(&self) -> Vec<PinDefinition> {
        vec![PinDefinition::output(
            PinType::Json,
            "发送结果（包含实际发送的帧信息）",
        )]
    }

    #[allow(clippy::cast_possible_truncation)]
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

        // 从 payload 提取帧参数（优先于 config 默认值）
        let payload_map = into_payload_map(payload.clone());
        let can_id = payload_map
            .get("can_id")
            .and_then(Value::as_u64)
            .map(|value| {
                u32::try_from(value).map_err(|_| {
                    EngineError::node_config(
                        self.id.clone(),
                        format!("CAN 帧 ID {value} 超过 u32 上限"),
                    )
                })
            })
            .transpose()?
            .or(self.config.can_id)
            .unwrap_or(0x001);

        let is_extended = payload_map
            .get("is_extended")
            .and_then(Value::as_bool)
            .unwrap_or(self.config.is_extended);

        validate_can_id(can_id, is_extended)
            .map_err(|e| EngineError::node_config(self.id.clone(), e.to_string()))?;

        let data: Vec<u8> = payload_map
            .get("data")
            .and_then(|v| {
                if let Value::Array(arr) = v {
                    Some(parse_byte_array(arr, &self.id))
                } else if let Value::String(s) = v {
                    // 支持十六进制字符串如 "01 02 03" 或 "010203"
                    Some(
                        parse_hex_string(s)
                            .map_err(|message| EngineError::node_config(self.id.clone(), message)),
                    )
                } else {
                    None
                }
            })
            .transpose()?
            .unwrap_or_default();

        // 构建 CAN 帧
        let frame = if is_extended {
            CanFrame::new_extended(can_id, &data)
        } else {
            CanFrame::new_standard(can_id, &data)
        };

        // 构建 CAN 总线配置
        let bus_config = if let Some(ref mut g) = guard {
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

        // 创建后端并发送
        let bus = create_can_bus(&bus_config)
            .await
            .map_err(|e| EngineError::stage_execution(self.id.clone(), trace_id, e.to_string()))?;

        bus.send(&frame)
            .await
            .map_err(|e| EngineError::stage_execution(self.id.clone(), trace_id, e.to_string()))?;

        // 构建输出 payload
        let mut output = payload_map;
        output.insert(
            "sent".to_owned(),
            json!({
                "id": frame.id,
                "id_hex": format!("0x{:03X}", frame.id),
                "data": frame.data,
                "data_hex": hex::encode(&frame.data).to_ascii_uppercase(),
                "dlc": frame.dlc,
                "is_extended": frame.is_extended,
            }),
        );

        // 构建 metadata
        let mut can_meta = Map::from_iter([
            ("simulated".to_owned(), json!(guard.is_none())),
            ("channel_info".to_owned(), json!(bus.channel_info())),
            ("sent_at".to_owned(), json!(Utc::now().to_rfc3339())),
        ]);

        if let Some(ref mut g) = guard {
            let (key, value) = connection_metadata(&self.id, g.lease())?;
            can_meta.insert(key, value);
            g.mark_success();
        }

        let metadata = Map::from_iter([("can".to_owned(), Value::Object(can_meta))]);
        Ok(NodeExecution::broadcast(Value::Object(output)).with_metadata(Some(metadata)))
    }
}

/// 解析十六进制字符串为字节数组。
///
/// 支持格式：`"01 02 03"`、`"010203"`、`"0x01 0x02"`
fn parse_byte_array(values: &[Value], node_id: &str) -> Result<Vec<u8>, EngineError> {
    values
        .iter()
        .enumerate()
        .map(|(index, value)| {
            let Some(number) = value.as_u64() else {
                return Err(EngineError::node_config(
                    node_id.to_owned(),
                    format!("data[{index}] 必须是 0-255 的整数"),
                ));
            };
            u8::try_from(number).map_err(|_| {
                EngineError::node_config(
                    node_id.to_owned(),
                    format!("data[{index}]={number} 超过字节上限 255"),
                )
            })
        })
        .collect()
}

fn parse_hex_string(s: &str) -> Result<Vec<u8>, String> {
    let without_prefix = s.replace("0x", "").replace("0X", "");
    let mut cleaned = String::with_capacity(without_prefix.len());

    for ch in without_prefix.chars() {
        if ch.is_ascii_hexdigit() {
            cleaned.push(ch);
        } else if ch.is_ascii_whitespace() || matches!(ch, '_' | '-' | ':' | ',') {
        } else {
            return Err(format!("非法十六进制字符: {ch}"));
        }
    }

    hex::decode(&cleaned)
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    #[test]
    fn 解析十六进制字符串空格分隔() {
        assert_eq!(
            parse_hex_string("01 02 03").unwrap(),
            vec![0x01, 0x02, 0x03]
        );
    }

    #[test]
    fn 解析十六进制字符串无分隔() {
        assert_eq!(parse_hex_string("010203").unwrap(), vec![0x01, 0x02, 0x03]);
    }

    #[test]
    fn 解析十六进制字符串带前缀() {
        assert_eq!(parse_hex_string("0x01 0x02").unwrap(), vec![0x01, 0x02]);
    }

    #[test]
    fn 解析空字符串() {
        assert!(parse_hex_string("").unwrap().is_empty());
    }

    #[test]
    fn 解析奇数长度十六进制失败() {
        assert!(parse_hex_string("123").is_err());
    }

    #[test]
    fn input_pin_是_json_必需() {
        let node = CanWriteNode::new(
            "can-write-1",
            CanWriteNodeConfig {
                connection_id: None,
                can_id: None,
                is_extended: false,
            },
            connections::shared_connection_manager(),
        );

        let pins = node.input_pins();
        assert_eq!(pins.len(), 1);
        assert_eq!(pins[0].id, "in");
        assert_eq!(pins[0].pin_type, PinType::Json);
        assert!(pins[0].required);
    }
}
