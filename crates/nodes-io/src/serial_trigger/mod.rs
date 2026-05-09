//! 串口触发节点：监听扫码枪 / RFID / 工业仪表的主动上报数据帧。
//!
//! ## 触发模式
//!
//! [`on_deploy`] 顺序：
//! 1. acquire 连接、校验类型为串口、解析 metadata 为 [`SerialTriggerNodeConfig`]
//! 2. 在 `tokio::task::spawn_blocking` 中跑同步串口读循环
//! 3. 每收到完整帧就通过 `runtime.block_on(handle.emit(payload, metadata))` 推进 DAG
//!
//! `transform` 路径仍可被手动 dispatch 调用（带 `_serial_frame` payload）
//! 并得到等价输出——两条路径共用 [`frame::build_serial_payload`] 与
//! [`SerialTriggerNode::serial_metadata`] 确保 payload 字段（`serial_data` /
//! `serial_ascii` / `serial_hex`）与 `metadata.serial` 结构一致。
//!
//! ## 背压策略说明
//!
//! 同 [`crate::TimerNode`]：emit 走 `NodeHandle` 而非 `WorkflowDispatchRouter`
//! 的 trigger lane，后者的 backpressure / DLQ / retry / metrics 在本节点不生效。
//! 串口数据率受物理层限制，DLQ / retry 几乎无触发场景。引擎级背压能力规划见
//! ADR-0014 / ADR-0016。
//!
//! [`on_deploy`]: NodeTrait::on_deploy

mod frame;
#[path = "loop.rs"]
mod serial_loop;
#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests;

use async_trait::async_trait;
use chrono::Utc;
use connections::SharedConnectionManager;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};

use uuid::Uuid;

use nazh_core::{
    EngineError, LifecycleGuard, NodeExecution, NodeLifecycleContext, NodeTrait, into_payload_map,
};

use self::frame::{frame_string, frame_u64, normalize_ascii, normalize_hex};

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
    /// 来自节点 config 的部分字段（主要是 `inject`、`encoding`、`trim`）。
    /// `port_path` / `baud_rate` 等串口参数最终来自 `connection.metadata`，
    /// `on_deploy` 时合并。
    config: SerialTriggerNodeConfig,
    /// 节点绑定的连接 ID（来自 `WorkflowNodeDefinition::connection_id()`）。
    /// 为 `None` 时 `on_deploy` 跳过监听（手动 dispatch 模式）。
    connection_id: Option<String>,
    connection_manager: SharedConnectionManager,
}

impl SerialTriggerNode {
    pub fn new(
        id: impl Into<String>,
        config: SerialTriggerNodeConfig,
        connection_id: Option<String>,
        connection_manager: SharedConnectionManager,
    ) -> Self {
        Self {
            id: id.into(),
            config,
            connection_id,
            connection_manager,
        }
    }

    /// 触发器路径与 transform 路径共用的 metadata 构造（节点级元数据）。
    /// 11 个参数是因 metadata 字段众多；用 struct 收敛会让 transform 路径
    /// 的取值-传递两次重复，反而损可读性。
    #[allow(clippy::too_many_arguments)]
    fn serial_metadata(
        &self,
        port_path: &str,
        connection_id: Option<&str>,
        baud_rate: u64,
        data_bits: u64,
        parity: &str,
        stop_bits: u64,
        flow_control: &str,
        encoding: &str,
        byte_len: u64,
        received_at: &str,
    ) -> Map<String, Value> {
        Map::from_iter([(
            "serial".to_owned(),
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
        )])
    }
}

#[async_trait]
impl NodeTrait for SerialTriggerNode {
    nazh_core::impl_node_meta!("serialTrigger");

    async fn transform(
        &self,
        _trace_id: Uuid,
        payload: Value,
    ) -> Result<NodeExecution, EngineError> {
        let mut payload_map = into_payload_map(payload);

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
            .map_or_else(|| Utc::now().to_rfc3339(), ToOwned::to_owned);
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

        let metadata = self.serial_metadata(
            port_path,
            connection_id,
            baud_rate,
            data_bits,
            parity,
            stop_bits,
            flow_control,
            &encoding,
            byte_len,
            &received_at,
        );

        Ok(NodeExecution::broadcast(Value::Object(payload_map)).with_metadata(Some(metadata)))
    }

    async fn on_deploy(&self, ctx: NodeLifecycleContext) -> Result<LifecycleGuard, EngineError> {
        // 1. 必须有 connection_id（手动 dispatch 模式无需监听）
        let Some(connection_id) = self.connection_id.clone() else {
            return Ok(LifecycleGuard::noop());
        };

        // 2. 同步预校验：acquire 连接 + 校验类型 + 解析 metadata + port_path 非空
        let mut full_config = {
            let mut guard = self
                .connection_manager
                .acquire(&connection_id)
                .await
                .map_err(|error| {
                    EngineError::node_config(
                        self.id.clone(),
                        format!("串口连接资源 `{connection_id}` 借出失败: {error}"),
                    )
                })?;

            if !serial_loop::is_serial_connection_kind(&guard.lease().kind) {
                let reason = format!("连接资源 `{connection_id}` 不是串口类型");
                guard.mark_failure(&reason);
                drop(guard);
                let _ = self
                    .connection_manager
                    .mark_invalid_configuration(&connection_id, &reason)
                    .await;
                return Err(EngineError::node_config(self.id.clone(), reason));
            }

            let mut full_config: SerialTriggerNodeConfig =
                serde_json::from_value(guard.metadata().clone()).map_err(|error| {
                    EngineError::node_config(self.id.clone(), error.to_string())
                })?;
            // 节点 config 的 inject 优先级高于 connection metadata
            full_config.inject.clone_from(&self.config.inject);
            full_config.port_path = full_config.port_path.trim().to_owned();
            if full_config.port_path.is_empty() {
                let reason = format!("串口连接资源 `{connection_id}` 需要配置 port_path");
                guard.mark_failure(&reason);
                drop(guard);
                let _ = self
                    .connection_manager
                    .mark_invalid_configuration(&connection_id, &reason)
                    .await;
                return Err(EngineError::node_config(self.id.clone(), reason));
            }
            guard.mark_success();
            full_config
        };
        // 节点 config 的 encoding / trim 优先（ConnectionMetadata 上没有这两个字段时
        // 解析得到的是默认值；这里显式覆盖为节点 config 设置）
        full_config.encoding.clone_from(&self.config.encoding);
        full_config.trim = self.config.trim;

        // 3. 在 spawn_blocking 中跑同步串口读循环
        let id = self.id.clone();
        let handle = ctx.handle.clone();
        let token = ctx.shutdown.clone();
        let connection_manager = self.connection_manager.clone();
        let runtime = tokio::runtime::Handle::current();
        let join = tokio::task::spawn_blocking(move || {
            serial_loop::run_serial_loop(
                &id,
                &full_config,
                &connection_id,
                &connection_manager,
                &handle,
                &token,
                &runtime,
            );
        });

        Ok(LifecycleGuard::from_task(ctx.shutdown, join))
    }
}
