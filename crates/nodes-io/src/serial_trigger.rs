//! 串口触发节点：监听扫码枪 / RFID / 工业仪表的主动上报数据帧。
//!
//! ## 触发模式（ADR-0009 后）
//!
//! 节点 [`on_deploy`] 中：
//! 1. acquire 连接、校验类型为串口、解析 metadata 为 [`SerialTriggerNodeConfig`]
//! 2. 在 `tokio::task::spawn_blocking` 中跑同步串口读循环
//! 3. 每收到完整帧就通过 `runtime.block_on(handle.emit(payload, metadata))` 推进 DAG
//!
//! `transform` 路径仍保留——若调用方手动 dispatch 到 serial 节点（带 `_serial_frame`
//! payload），会得到等价输出。两条路径共用 [`build_serial_payload`] 与
//! [`SerialTriggerNode::serial_metadata`] 确保 payload 字段（`serial_data` /
//! `serial_ascii` / `serial_hex`）与 metadata.serial 结构一致。
//!
//! ## 与壳层 `dispatch_router` 的语义差异
//!
//! 同 [`crate::TimerNode`]：迁移前壳层 `submit_serial_frame` 走 `dispatch_router`
//! 的 trigger lane（含 backpressure / 死信队列 / 重试 / metrics）。迁移后直接
//! 走 `NodeHandle::emit`，失去这套防御能力。串口数据率受物理层限制，DLQ /
//! retry 几乎无触发场景。后续 ADR-0014 / ADR-0016 引擎级背压能力再补回。
//!
//! [`on_deploy`]: NodeTrait::on_deploy

use async_trait::async_trait;
use chrono::Utc;
use connections::SharedConnectionManager;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};

use uuid::Uuid;

use nazh_core::{
    EngineError, LifecycleGuard, NodeExecution, NodeLifecycleContext, NodeTrait, into_payload_map,
};

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

/// 把已收到的帧字节解码为标准化 payload 字段。emit 路径与 transform 路径共用。
fn build_serial_payload(
    frame_bytes: &[u8],
    config: &SerialTriggerNodeConfig,
) -> (Value, u64, String) {
    let ascii_raw = String::from_utf8_lossy(frame_bytes).to_string();
    let ascii = normalize_ascii(&ascii_raw, config.trim);
    let hex_raw = serial_helpers::bytes_to_hex(frame_bytes);
    let hex = normalize_hex(&hex_raw);
    let encoding = config.encoding.trim().to_ascii_lowercase();
    let serial_data = if encoding == "hex" { &hex } else { &ascii };

    let mut payload_map = Map::new();
    for (key, value) in &config.inject {
        payload_map.insert(key.clone(), value.clone());
    }
    payload_map.insert("serial_data".to_owned(), json!(serial_data));
    payload_map.insert("serial_ascii".to_owned(), json!(ascii));
    payload_map.insert("serial_hex".to_owned(), json!(hex));

    #[allow(clippy::cast_possible_truncation)]
    let byte_len = frame_bytes.len() as u64;
    (Value::Object(payload_map), byte_len, encoding)
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

        Ok(NodeExecution::broadcast(Value::Object(payload_map)).with_metadata(metadata))
    }

    async fn on_deploy(
        &self,
        ctx: NodeLifecycleContext,
    ) -> Result<LifecycleGuard, EngineError> {
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

            if !serial_helpers::is_serial_connection_kind(&guard.lease().kind) {
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
            // node config 的 inject 覆盖 connection metadata（与原壳层语义一致）
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
            serial_helpers::run_serial_loop(
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

/// 串口同步读循环、bytes/hex/delimiter 等私有 helper。
mod serial_helpers {
    use std::io::Read;
    use std::time::{Duration, Instant};

    use chrono::Utc;
    use connections::SharedConnectionManager;
    use nazh_core::{CancellationToken, NodeHandle};
    use serde_json::Map;

    use super::{SerialTriggerNodeConfig, build_serial_payload};

    pub(super) fn is_serial_connection_kind(connection_kind: &str) -> bool {
        matches!(
            connection_kind.trim().to_ascii_lowercase().as_str(),
            "serial" | "serialport" | "serial_port" | "uart" | "rs232" | "rs485"
        )
    }

    pub(super) fn bytes_to_hex(bytes: &[u8]) -> String {
        const HEX: &[u8; 16] = b"0123456789ABCDEF";
        let mut output = String::with_capacity(bytes.len().saturating_mul(3).saturating_sub(1));
        for (index, byte) in bytes.iter().enumerate() {
            if index > 0 {
                output.push(' ');
            }
            output.push(HEX[(*byte >> 4) as usize] as char);
            output.push(HEX[(*byte & 0x0F) as usize] as char);
        }
        output
    }

    fn parse_hex_bytes(value: &str) -> Vec<u8> {
        let nibbles = value.bytes().filter_map(hex_nibble).collect::<Vec<_>>();
        let mut bytes = Vec::with_capacity(nibbles.len() / 2);
        for pair in nibbles.chunks(2) {
            if pair.len() == 2 {
                bytes.push((pair[0] << 4) | pair[1]);
            }
        }
        bytes
    }

    fn hex_nibble(value: u8) -> Option<u8> {
        match value {
            b'0'..=b'9' => Some(value - b'0'),
            b'a'..=b'f' => Some(value - b'a' + 10),
            b'A'..=b'F' => Some(value - b'A' + 10),
            _ => None,
        }
    }

    pub(super) fn decode_serial_delimiter(value: &str) -> Vec<u8> {
        if value.is_empty() {
            return Vec::new();
        }
        let trimmed = value.trim();
        if let Some(hex) = trimmed
            .strip_prefix("hex:")
            .or_else(|| trimmed.strip_prefix("0x"))
        {
            return parse_hex_bytes(hex);
        }
        let mut bytes = Vec::new();
        let mut chars = value.chars();
        while let Some(ch) = chars.next() {
            if ch != '\\' {
                let mut encoded = [0_u8; 4];
                bytes.extend_from_slice(ch.encode_utf8(&mut encoded).as_bytes());
                continue;
            }
            match chars.next() {
                Some('n') => bytes.push(b'\n'),
                Some('r') => bytes.push(b'\r'),
                Some('t') => bytes.push(b'\t'),
                Some('\\') | None => bytes.push(b'\\'),
                Some(other) => {
                    let mut encoded = [0_u8; 4];
                    bytes.extend_from_slice(other.encode_utf8(&mut encoded).as_bytes());
                }
            }
        }
        bytes
    }

    fn drain_delimited_frame(buffer: &mut Vec<u8>, delimiter: &[u8]) -> Option<Vec<u8>> {
        if delimiter.is_empty() || buffer.len() < delimiter.len() {
            return None;
        }
        let delimiter_index = buffer
            .windows(delimiter.len())
            .position(|window| window == delimiter)?;
        let frame = buffer.drain(..delimiter_index).collect::<Vec<_>>();
        let _ = buffer.drain(..delimiter.len()).count();
        Some(frame)
    }

    fn serial_data_bits(value: u8) -> serialport::DataBits {
        match value {
            5 => serialport::DataBits::Five,
            6 => serialport::DataBits::Six,
            7 => serialport::DataBits::Seven,
            _ => serialport::DataBits::Eight,
        }
    }

    fn serial_parity(value: &str) -> serialport::Parity {
        match value.trim().to_ascii_lowercase().as_str() {
            "odd" | "o" => serialport::Parity::Odd,
            "even" | "e" => serialport::Parity::Even,
            _ => serialport::Parity::None,
        }
    }

    fn serial_stop_bits(value: u8) -> serialport::StopBits {
        if value == 2 {
            serialport::StopBits::Two
        } else {
            serialport::StopBits::One
        }
    }

    fn serial_flow_control(value: &str) -> serialport::FlowControl {
        match value.trim().to_ascii_lowercase().as_str() {
            "software" | "xonxoff" => serialport::FlowControl::Software,
            "hardware" | "rtscts" => serialport::FlowControl::Hardware,
            _ => serialport::FlowControl::None,
        }
    }

    fn governance_u64(metadata: &serde_json::Value, key: &str) -> Option<u64> {
        metadata
            .as_object()
            .and_then(|value| value.get("governance"))
            .and_then(serde_json::Value::as_object)
            .and_then(|governance| governance.get(key))
            .and_then(serde_json::Value::as_u64)
    }

    /// 同步 sleep 但响应 cancel——通过短间隔轮询 `is_cancelled`。
    fn sleep_with_cancel(token: &CancellationToken, total: Duration) {
        let step = Duration::from_millis(50);
        let mut remaining = total;
        while remaining > Duration::ZERO {
            if token.is_cancelled() {
                return;
            }
            let chunk = remaining.min(step);
            std::thread::sleep(chunk);
            remaining = remaining.saturating_sub(chunk);
        }
    }

    /// 提交一帧：构造 payload + metadata，`runtime.block_on` 调用 `handle.emit`。
    fn submit_frame(
        node_id: &str,
        config: &SerialTriggerNodeConfig,
        connection_id: &str,
        frame: &[u8],
        handle: &NodeHandle,
        runtime: &tokio::runtime::Handle,
    ) {
        if frame.is_empty() {
            return;
        }
        let (payload, byte_len, encoding) = build_serial_payload(frame, config);
        let metadata: Map<String, serde_json::Value> = Map::from_iter([(
            "serial".to_owned(),
            serde_json::json!({
                "node_id": node_id,
                "port_path": config.port_path.as_str(),
                "connection_id": connection_id,
                "baud_rate": config.baud_rate,
                "data_bits": config.data_bits,
                "parity": config.parity.as_str(),
                "stop_bits": config.stop_bits,
                "flow_control": config.flow_control.as_str(),
                "encoding": encoding.as_str(),
                "byte_len": byte_len,
                "received_at": Utc::now().to_rfc3339(),
            }),
        )]);
        if let Err(error) = runtime.block_on(handle.emit(payload, metadata)) {
            tracing::warn!(node_id = %node_id, ?error, "serial emit 失败");
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn flush_idle(
        node_id: &str,
        config: &SerialTriggerNodeConfig,
        connection_id: &str,
        buffer: &mut Vec<u8>,
        last_byte_at: Option<Instant>,
        idle_gap: Duration,
        handle: &NodeHandle,
        runtime: &tokio::runtime::Handle,
    ) {
        if buffer.is_empty() {
            return;
        }
        if last_byte_at.is_some_and(|instant| instant.elapsed() >= idle_gap) {
            let frame = std::mem::take(buffer);
            submit_frame(node_id, config, connection_id, &frame, handle, runtime);
        }
    }

    /// 同步串口读循环（在 `tokio::task::spawn_blocking` 线程上跑）。
    ///
    /// 与原壳层 `run_serial_root_reader` 行为等价，主要差异：
    /// - 数据出口从 `dispatch_router.blocking_submit_trigger_to` 改为
    ///   `runtime.block_on(handle.emit(...))`
    /// - 取消信号从 `Arc<AtomicBool>` 改为 `CancellationToken`（同步 `is_cancelled()` 检查）
    /// - 移除了 `emit_trigger_failure` 的壳层 UI 通知（撤销路径会发
    ///   `ExecutionEvent::Failed` → 走 `NodeHandle::emit` 默认事件流，
    ///   前端仍可观察到失败）
    #[allow(clippy::too_many_lines, clippy::too_many_arguments)]
    pub(super) fn run_serial_loop(
        node_id: &str,
        config: &SerialTriggerNodeConfig,
        connection_id: &str,
        connection_manager: &SharedConnectionManager,
        handle: &NodeHandle,
        token: &CancellationToken,
        runtime: &tokio::runtime::Handle,
    ) {
        let read_timeout = Duration::from_millis(config.read_timeout_ms.clamp(10, 2_000));
        let idle_gap = Duration::from_millis(config.idle_gap_ms.clamp(1, 10_000));
        let max_frame_bytes = config.max_frame_bytes.clamp(1, 8_192);
        let delimiter = decode_serial_delimiter(&config.delimiter);

        while !token.is_cancelled() {
            let mut guard = match runtime.block_on(connection_manager.acquire(connection_id)) {
                Ok(guard) => guard,
                Err(error) => {
                    tracing::warn!(node_id = %node_id, ?error, "串口连接借出失败，800ms 后重试");
                    sleep_with_cancel(token, Duration::from_millis(800));
                    continue;
                }
            };
            let heartbeat_interval = Duration::from_millis(
                governance_u64(guard.metadata(), "heartbeat_interval_ms")
                    .unwrap_or(3_000)
                    .clamp(250, 30_000),
            );

            let connect_started_at = Instant::now();
            let port_result = serialport::new(config.port_path.clone(), config.baud_rate.max(1))
                .timeout(read_timeout)
                .data_bits(serial_data_bits(config.data_bits))
                .parity(serial_parity(&config.parity))
                .stop_bits(serial_stop_bits(config.stop_bits))
                .flow_control(serial_flow_control(&config.flow_control))
                .open();
            let mut port = match port_result {
                Ok(port) => {
                    let connect_latency_ms =
                        u64::try_from(connect_started_at.elapsed().as_millis())
                            .unwrap_or(u64::MAX);
                    let _ = runtime.block_on(connection_manager.record_connect_success(
                        connection_id,
                        format!("串口 {} 已建立监听，等待外设上报数据", config.port_path),
                        Some(connect_latency_ms),
                    ));
                    port
                }
                Err(error) => {
                    let reason = format!("串口打开失败: {error}");
                    guard.mark_failure(&reason);
                    let retry_after_ms = runtime
                        .block_on(connection_manager.record_connect_failure(connection_id, &reason))
                        .unwrap_or(800);
                    drop(guard);
                    tracing::warn!(node_id = %node_id, %reason, retry_after_ms, "串口打开失败");
                    sleep_with_cancel(token, Duration::from_millis(retry_after_ms));
                    continue;
                }
            };
            let mut last_heartbeat_sent_at = Instant::now();

            let mut buffer = Vec::with_capacity(max_frame_bytes.min(512));
            let mut scratch = [0_u8; 64];
            let mut last_byte_at: Option<Instant> = None;
            let mut disconnected_reason: Option<String> = None;

            while !token.is_cancelled() {
                match port.read(&mut scratch) {
                    Ok(0) => {
                        flush_idle(
                            node_id,
                            config,
                            connection_id,
                            &mut buffer,
                            last_byte_at,
                            idle_gap,
                            handle,
                            runtime,
                        );
                        if last_heartbeat_sent_at.elapsed() >= heartbeat_interval {
                            let _ = runtime.block_on(connection_manager.record_heartbeat(
                                connection_id,
                                format!("串口 {} 心跳正常，监听仍在进行中", config.port_path),
                            ));
                            last_heartbeat_sent_at = Instant::now();
                        }
                    }
                    Ok(bytes_read) => {
                        buffer.extend_from_slice(&scratch[..bytes_read]);
                        last_byte_at = Some(Instant::now());
                        let _ = runtime.block_on(connection_manager.record_heartbeat(
                            connection_id,
                            format!("串口 {} 收到 {} 字节输入", config.port_path, bytes_read),
                        ));
                        last_heartbeat_sent_at = Instant::now();
                        while let Some(frame) = drain_delimited_frame(&mut buffer, &delimiter) {
                            submit_frame(node_id, config, connection_id, &frame, handle, runtime);
                        }
                        if buffer.len() >= max_frame_bytes {
                            let frame = buffer.drain(..max_frame_bytes).collect::<Vec<_>>();
                            submit_frame(node_id, config, connection_id, &frame, handle, runtime);
                        }
                    }
                    Err(error) if error.kind() == std::io::ErrorKind::TimedOut => {
                        if buffer.is_empty() {
                            if last_heartbeat_sent_at.elapsed() >= heartbeat_interval {
                                let _ = runtime.block_on(connection_manager.record_heartbeat(
                                    connection_id,
                                    format!(
                                        "串口 {} 空闲等待中，链路仍存活",
                                        config.port_path
                                    ),
                                ));
                                last_heartbeat_sent_at = Instant::now();
                            }
                            continue;
                        }
                        let Some(last_byte_at_instant) = last_byte_at else {
                            continue;
                        };
                        if last_byte_at_instant.elapsed() < idle_gap {
                            continue;
                        }
                        flush_idle(
                            node_id,
                            config,
                            connection_id,
                            &mut buffer,
                            last_byte_at,
                            idle_gap,
                            handle,
                            runtime,
                        );
                    }
                    Err(error) => {
                        disconnected_reason = Some(format!("串口读取失败: {error}"));
                        break;
                    }
                }
            }

            if !token.is_cancelled() && !buffer.is_empty() {
                submit_frame(node_id, config, connection_id, &buffer, handle, runtime);
            }

            if token.is_cancelled() {
                guard.mark_success();
                let reason = format!("串口 {} 监听已停止", config.port_path);
                drop(guard);
                let _ = runtime
                    .block_on(connection_manager.mark_disconnected(connection_id, &reason));
                break;
            }

            let reason = disconnected_reason
                .unwrap_or_else(|| format!("串口 {} 连接已断开", config.port_path));
            guard.mark_failure(&reason);
            let retry_after_ms = runtime
                .block_on(connection_manager.record_connect_failure(connection_id, &reason))
                .unwrap_or(800);
            drop(guard);
            tracing::warn!(node_id = %node_id, %reason, retry_after_ms, "串口连接断开");
            sleep_with_cancel(token, Duration::from_millis(retry_after_ms));
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn build_serial_payload_包含三种格式() {
        let config = SerialTriggerNodeConfig {
            port_path: "/dev/null".to_owned(),
            baud_rate: 9600,
            data_bits: 8,
            parity: "none".to_owned(),
            stop_bits: 1,
            flow_control: "none".to_owned(),
            encoding: "ascii".to_owned(),
            delimiter: "\n".to_owned(),
            read_timeout_ms: 100,
            idle_gap_ms: 80,
            max_frame_bytes: 512,
            trim: true,
            inject: Map::new(),
        };
        let (payload, byte_len, encoding) = build_serial_payload(b"hello", &config);
        let obj = payload.as_object().unwrap();
        assert_eq!(obj.get("serial_data"), Some(&json!("hello")));
        assert_eq!(obj.get("serial_ascii"), Some(&json!("hello")));
        assert_eq!(obj.get("serial_hex"), Some(&json!("68 65 6C 6C 6F")));
        assert_eq!(byte_len, 5);
        assert_eq!(encoding, "ascii");
    }

    #[test]
    fn build_serial_payload_hex_encoding_切换主显示() {
        let mut config = SerialTriggerNodeConfig {
            port_path: String::new(),
            baud_rate: 9600,
            data_bits: 8,
            parity: "none".to_owned(),
            stop_bits: 1,
            flow_control: "none".to_owned(),
            encoding: "hex".to_owned(),
            delimiter: "\n".to_owned(),
            read_timeout_ms: 100,
            idle_gap_ms: 80,
            max_frame_bytes: 512,
            trim: true,
            inject: Map::new(),
        };
        config.inject.insert("source".to_owned(), json!("scanner"));
        let (payload, _byte_len, encoding) = build_serial_payload(&[0xAB, 0xCD], &config);
        let obj = payload.as_object().unwrap();
        assert_eq!(obj.get("serial_data"), Some(&json!("AB CD")));
        assert_eq!(obj.get("serial_hex"), Some(&json!("AB CD")));
        assert_eq!(obj.get("source"), Some(&json!("scanner")));
        assert_eq!(encoding, "hex");
    }

    #[test]
    fn is_serial_connection_kind_接受常见别名() {
        assert!(serial_helpers::is_serial_connection_kind("serial"));
        assert!(serial_helpers::is_serial_connection_kind("Serial"));
        assert!(serial_helpers::is_serial_connection_kind("UART"));
        assert!(serial_helpers::is_serial_connection_kind("RS485"));
        assert!(!serial_helpers::is_serial_connection_kind("mqtt"));
    }

    #[test]
    fn decode_serial_delimiter_支持转义与hex() {
        assert_eq!(serial_helpers::decode_serial_delimiter(""), Vec::<u8>::new());
        assert_eq!(serial_helpers::decode_serial_delimiter("\\n"), b"\n");
        assert_eq!(serial_helpers::decode_serial_delimiter("\\r\\n"), b"\r\n");
        assert_eq!(serial_helpers::decode_serial_delimiter("hex:0d0a"), b"\r\n");
        assert_eq!(serial_helpers::decode_serial_delimiter("0xFF"), vec![0xFF]);
    }

    #[test]
    fn bytes_to_hex_格式正确() {
        assert_eq!(serial_helpers::bytes_to_hex(&[]), "");
        assert_eq!(serial_helpers::bytes_to_hex(&[0xAB]), "AB");
        assert_eq!(serial_helpers::bytes_to_hex(&[0x00, 0xFF, 0x10]), "00 FF 10");
    }
}
