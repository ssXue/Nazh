use std::io::Read;
use std::time::{Duration, Instant};

use chrono::Utc;
use connections::SharedConnectionManager;
use nazh_core::{CancellationToken, NodeHandle, blocking_sleep_or_cancel};
use serde_json::Map;

use super::SerialTriggerNodeConfig;
use super::frame::build_serial_payload;

pub(super) fn is_serial_connection_kind(connection_kind: &str) -> bool {
    matches!(
        connection_kind.trim().to_ascii_lowercase().as_str(),
        "serial" | "serialport" | "serial_port" | "uart" | "rs232" | "rs485"
    )
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
/// 数据出口走 `runtime.block_on(handle.emit(...))` 桥接 async DAG；
/// 取消信号走 `CancellationToken::is_cancelled()` 同步轮询。
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
                blocking_sleep_or_cancel(token, Duration::from_millis(800));
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
                    u64::try_from(connect_started_at.elapsed().as_millis()).unwrap_or(u64::MAX);
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
                blocking_sleep_or_cancel(token, Duration::from_millis(retry_after_ms));
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
                    // 每字节调 record_heartbeat 会让 ConnectionManager 在
                    // 高吞吐设备上承担过度的写锁压力；按 heartbeat_interval 节流。
                    if last_heartbeat_sent_at.elapsed() >= heartbeat_interval {
                        let _ = runtime.block_on(connection_manager.record_heartbeat(
                            connection_id,
                            format!("串口 {} 收到 {} 字节输入", config.port_path, bytes_read),
                        ));
                        last_heartbeat_sent_at = Instant::now();
                    }
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
                                format!("串口 {} 空闲等待中，链路仍存活", config.port_path),
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
            let _ = runtime.block_on(connection_manager.mark_disconnected(connection_id, &reason));
            break;
        }

        let reason =
            disconnected_reason.unwrap_or_else(|| format!("串口 {} 连接已断开", config.port_path));
        guard.mark_failure(&reason);
        let retry_after_ms = runtime
            .block_on(connection_manager.record_connect_failure(connection_id, &reason))
            .unwrap_or(800);
        drop(guard);
        tracing::warn!(node_id = %node_id, %reason, retry_after_ms, "串口连接断开");
        blocking_sleep_or_cancel(token, Duration::from_millis(retry_after_ms));
    }
}
