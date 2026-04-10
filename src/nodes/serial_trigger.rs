//! 串口触发节点，接收扫码枪、RFID 等外设主动上报的数据帧。
//!
//! 实际串口监听由桌面壳层负责，本节点只负责把已接收的串口帧
//! 规范化写入 payload，使后续 DAG 节点可以统一消费 `serial_data`、
//! `serial_ascii`、`serial_hex` 与 `_serial` 元数据。

use async_trait::async_trait;
use chrono::Utc;
use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};

use super::helpers::into_payload_map;
use super::{NodeExecution, NodeTrait};
use crate::{EngineError, WorkflowContext};

fn default_baud_rate() -> u32 {
    9_600
}

fn default_data_bits() -> u8 {
    8
}

fn default_stop_bits() -> u8 {
    1
}

fn default_parity() -> String {
    "none".to_owned()
}

fn default_flow_control() -> String {
    "none".to_owned()
}

fn default_encoding() -> String {
    "ascii".to_owned()
}

fn default_delimiter() -> String {
    "\n".to_owned()
}

fn default_read_timeout_ms() -> u64 {
    100
}

fn default_idle_gap_ms() -> u64 {
    80
}

fn default_max_frame_bytes() -> usize {
    512
}

fn default_trim() -> bool {
    true
}

/// 串口触发节点配置。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SerialTriggerNodeConfig {
    /// 串口路径，如 `/dev/tty.usbserial-0001`、`COM3`。
    #[serde(default, alias = "port")]
    pub port_path: String,
    #[serde(default = "default_baud_rate")]
    pub baud_rate: u32,
    #[serde(default = "default_data_bits")]
    pub data_bits: u8,
    #[serde(default = "default_parity")]
    pub parity: String,
    #[serde(default = "default_stop_bits")]
    pub stop_bits: u8,
    #[serde(default = "default_flow_control")]
    pub flow_control: String,
    /// `ascii` 或 `hex`，决定 `serial_data` 的主显示值。
    #[serde(default = "default_encoding")]
    pub encoding: String,
    /// 行帧分隔符，扫码枪常见为 `\n` 或 `\r\n`。
    #[serde(default = "default_delimiter")]
    pub delimiter: String,
    /// 单次底层读超时。
    #[serde(default = "default_read_timeout_ms")]
    pub read_timeout_ms: u64,
    /// 未遇到分隔符时，超过空闲间隔也会提交当前缓冲帧。
    #[serde(default = "default_idle_gap_ms")]
    pub idle_gap_ms: u64,
    /// 单帧最大字节数，防止异常外设撑爆内存。
    #[serde(default = "default_max_frame_bytes")]
    pub max_frame_bytes: usize,
    #[serde(default = "default_trim")]
    pub trim: bool,
    #[serde(default)]
    pub inject: Map<String, Value>,
}

/// 串口触发节点。
pub struct SerialTriggerNode {
    id: String,
    ai_description: String,
    config: SerialTriggerNodeConfig,
}

impl SerialTriggerNode {
    pub fn new(
        id: impl Into<String>,
        config: SerialTriggerNodeConfig,
        ai_description: impl Into<String>,
    ) -> Self {
        Self {
            id: id.into(),
            ai_description: ai_description.into(),
            config,
        }
    }
}

fn normalize_hex(value: &str) -> String {
    value
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_uppercase()
}

fn normalize_ascii(value: &str, trim: bool) -> String {
    if trim {
        value.trim().to_owned()
    } else {
        value.to_owned()
    }
}

fn frame_string<'a>(frame: &'a Map<String, Value>, key: &str, fallback: &'a str) -> &'a str {
    frame
        .get(key)
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .unwrap_or(fallback)
}

fn frame_u64(frame: &Map<String, Value>, key: &str, fallback: u64) -> u64 {
    frame.get(key).and_then(Value::as_u64).unwrap_or(fallback)
}

#[async_trait]
impl NodeTrait for SerialTriggerNode {
    impl_node_meta!("serialTrigger");

    async fn execute(&self, ctx: WorkflowContext) -> Result<NodeExecution, EngineError> {
        let mut payload_map = into_payload_map(ctx.payload);

        for (key, value) in &self.config.inject {
            payload_map.insert(key.clone(), value.clone());
        }

        let incoming_frame = payload_map
            .remove("_serial_frame")
            .and_then(|value| match value {
                Value::Object(map) => Some(map),
                _ => None,
            })
            .unwrap_or_default();

        let ascii = incoming_frame
            .get("ascii")
            .and_then(Value::as_str)
            .map(|value| normalize_ascii(value, self.config.trim))
            .unwrap_or_default();
        let hex = incoming_frame
            .get("hex")
            .or_else(|| incoming_frame.get("raw_hex"))
            .and_then(Value::as_str)
            .map(normalize_hex)
            .unwrap_or_default();
        let byte_len = incoming_frame
            .get("byte_len")
            .and_then(Value::as_u64)
            .unwrap_or_default();
        let received_at = incoming_frame
            .get("received_at")
            .and_then(Value::as_str)
            .map(ToOwned::to_owned)
            .unwrap_or_else(|| Utc::now().to_rfc3339());
        let port_path = incoming_frame
            .get("port_path")
            .and_then(Value::as_str)
            .filter(|value| !value.trim().is_empty())
            .unwrap_or(&self.config.port_path);
        let connection_id = incoming_frame
            .get("connection_id")
            .and_then(Value::as_str)
            .filter(|value| !value.trim().is_empty());
        let baud_rate = frame_u64(
            &incoming_frame,
            "baud_rate",
            u64::from(self.config.baud_rate),
        );
        let data_bits = frame_u64(
            &incoming_frame,
            "data_bits",
            u64::from(self.config.data_bits),
        );
        let stop_bits = frame_u64(
            &incoming_frame,
            "stop_bits",
            u64::from(self.config.stop_bits),
        );
        let parity = frame_string(&incoming_frame, "parity", &self.config.parity);
        let flow_control = frame_string(&incoming_frame, "flow_control", &self.config.flow_control);
        let encoding = frame_string(&incoming_frame, "encoding", &self.config.encoding)
            .trim()
            .to_ascii_lowercase();
        let serial_data = if encoding == "hex" { &hex } else { &ascii };

        payload_map.insert("serial_data".to_owned(), json!(serial_data));
        payload_map.insert("serial_ascii".to_owned(), json!(ascii));
        payload_map.insert("serial_hex".to_owned(), json!(hex));
        payload_map.insert(
            "_serial".to_owned(),
            json!({
                "node_id": self.id.as_str(),
                "port_path": port_path,
                "connection_id": connection_id,
                "baud_rate": baud_rate,
                "data_bits": data_bits,
                "parity": parity,
                "stop_bits": stop_bits,
                "flow_control": flow_control,
                "encoding": encoding,
                "byte_len": byte_len,
                "received_at": received_at,
            }),
        );

        Ok(NodeExecution::broadcast(WorkflowContext::from_parts(
            ctx.trace_id,
            Utc::now(),
            Value::Object(payload_map),
        )))
    }
}
