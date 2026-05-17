//! `SerialCommand` 串口帧监听后台循环（ADR-0024 Phase 3）。
//!
//! 被 `on_deploy` 通过 orchestrator task spawn。打开串口 →
//! 持续读取分隔帧 → 匹配 `SerialCommand` signal → 解码 → scale 求值 → emit。
//!
//! 与 CAN/MQTT 循环不同，串口是同步 I/O，因此整个读循环运行在 `spawn_blocking` 中。
//! emit 调用通过 `tokio::runtime::Handle::block_on` 桥接到异步 `NodeHandle::emit`。

use connections::SharedConnectionManager;
use nazh_core::{CancellationToken, NodeHandle, sleep_or_cancel};

use super::CompiledSignal;
use crate::signal_decode::{
    ByteOrderSnapshot, DataTypeSnapshot, apply_scale_with_engine, create_scale_engine,
    decode_raw_bytes,
};

/// 串口帧事件监听主循环。
#[allow(clippy::too_many_arguments, clippy::too_many_lines)]
pub(super) async fn run_serial_listen_loop(
    node_id: &str,
    connection_id: &str,
    signals: &[CompiledSignal],
    connection_manager: &SharedConnectionManager,
    handle: &NodeHandle,
    token: &CancellationToken,
    device_id: &str,
) {
    loop {
        if token.is_cancelled() {
            return;
        }

        let mut guard = match connection_manager.acquire(connection_id).await {
            Ok(g) => g,
            Err(error) => {
                tracing::warn!(node_id, ?error, "串口事件监听：连接借出失败，800ms 后重试");
                sleep_or_cancel(token, std::time::Duration::from_millis(800)).await;
                continue;
            }
        };

        let metadata = guard.lease().metadata.clone();
        let (port_path, baud_rate, delimiter) = match parse_serial_metadata(&metadata) {
            Ok(config) => config,
            Err(reason) => {
                guard.mark_failure(&reason);
                let retry_ms = connection_manager
                    .record_connect_failure(connection_id, &reason)
                    .await
                    .unwrap_or(800);
                drop(guard);
                tracing::warn!(node_id, %reason, retry_ms);
                sleep_or_cancel(token, std::time::Duration::from_millis(retry_ms)).await;
                continue;
            }
        };

        if port_path.is_empty() {
            let reason = "串口连接元数据缺少 port_path".to_owned();
            guard.mark_failure(&reason);
            let retry_ms = connection_manager
                .record_connect_failure(connection_id, &reason)
                .await
                .unwrap_or(800);
            drop(guard);
            tracing::warn!(node_id, %reason, retry_ms);
            sleep_or_cancel(token, std::time::Duration::from_millis(retry_ms)).await;
            continue;
        }

        let delim_bytes = parse_delimiter(&delimiter);

        // 将信号数据提取为可 Clone/Move 的扁平结构。
        let flat_signals: Vec<FlatSignal> = signals
            .iter()
            .map(|cs| FlatSignal {
                signal_id: cs.listener.signal_id.clone(),
                unit: cs.listener.unit.clone(),
                scale_ast: cs.scale_ast.clone(),
            })
            .collect();

        let node_id_owned = node_id.to_owned();
        let conn_id_owned = connection_id.to_owned();
        let device_id_owned = device_id.to_owned();
        let handle_owned = handle.clone();
        let token_owned = token.clone();
        let cm_owned = connection_manager.clone();
        let port_path_owned = port_path.clone();

        // 在 spawn_blocking 中运行同步串口读循环。
        let result = tokio::task::spawn_blocking(move || {
            run_serial_read_loop_sync(
                &node_id_owned,
                &port_path_owned,
                baud_rate,
                &delim_bytes,
                &flat_signals,
                &handle_owned,
                &token_owned,
                &device_id_owned,
            )
        })
        .await;

        match result {
            Ok(Ok(())) => {
                guard.mark_success();
                let reason = format!("串口事件监听 {port_path} 已停止");
                drop(guard);
                let _ = connection_manager
                    .mark_disconnected(connection_id, &reason)
                    .await;
                return;
            }
            Ok(Err(reason)) => {
                guard.mark_failure(&reason);
                let retry_ms = cm_owned
                    .record_connect_failure(&conn_id_owned, &reason)
                    .await
                    .unwrap_or(800);
                drop(guard);
                tracing::warn!(node_id, %reason, retry_ms);
                sleep_or_cancel(token, std::time::Duration::from_millis(retry_ms)).await;
            }
            Err(join_error) => {
                let reason = format!("串口事件监听任务异常: {join_error}");
                guard.mark_failure(&reason);
                let retry_ms = connection_manager
                    .record_connect_failure(connection_id, &reason)
                    .await
                    .unwrap_or(800);
                drop(guard);
                tracing::warn!(node_id, %reason, retry_ms);
                sleep_or_cancel(token, std::time::Duration::from_millis(retry_ms)).await;
            }
        }
    }
}

/// 扁平化的信号数据（可安全跨 `spawn_blocking` 移动）。
struct FlatSignal {
    signal_id: String,
    unit: Option<String>,
    scale_ast: Option<rhai::AST>,
}

/// 解析分隔符字符串为字节序列。
fn parse_delimiter(delimiter: &str) -> Vec<u8> {
    if delimiter == "\\n" {
        vec![b'\n']
    } else if delimiter == "\\r\\n" {
        vec![b'\r', b'\n']
    } else {
        delimiter.as_bytes().to_vec()
    }
}

/// 从连接元数据提取串口参数。
fn parse_serial_metadata(metadata: &serde_json::Value) -> Result<(String, u32, String), String> {
    let port_path = metadata
        .get("port_path")
        .or_else(|| metadata.get("port"))
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| "串口连接元数据缺少 port_path".to_owned())?
        .to_owned();
    let baud_rate = metadata
        .get("baud_rate")
        .and_then(serde_json::Value::as_u64)
        .and_then(|b| u32::try_from(b).ok())
        .filter(|value| *value > 0)
        .ok_or_else(|| "串口连接元数据缺少有效的 baud_rate".to_owned())?;
    let delimiter = metadata
        .get("delimiter")
        .and_then(serde_json::Value::as_str)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| "串口连接元数据缺少 delimiter".to_owned())?
        .to_owned();
    Ok((port_path, baud_rate, delimiter))
}

/// 同步串口读循环（运行在 `spawn_blocking` 中）。
#[allow(clippy::too_many_arguments)]
fn run_serial_read_loop_sync(
    node_id: &str,
    port_path: &str,
    baud_rate: u32,
    delim_bytes: &[u8],
    signals: &[FlatSignal],
    handle: &NodeHandle,
    token: &CancellationToken,
    device_id: &str,
) -> Result<(), String> {
    let mut port = serialport::new(port_path, baud_rate.max(1))
        .timeout(std::time::Duration::from_millis(100))
        .open()
        .map_err(|e| format!("串口打开失败 ({port_path}): {e}"))?;

    tracing::info!(node_id, port_path, baud_rate, "串口事件监听已启动");

    let mut buffer = Vec::with_capacity(256);
    let mut single = [0u8; 1];
    let engine = create_scale_engine();

    loop {
        if token.is_cancelled() {
            return Ok(());
        }

        match port.read(&mut single) {
            Ok(0) => {}
            Ok(n) => {
                buffer.extend_from_slice(&single[..n]);
                if !delim_bytes.is_empty() && buffer.len() >= delim_bytes.len() {
                    let tail = &buffer[buffer.len() - delim_bytes.len()..];
                    if tail == delim_bytes {
                        let frame_data = buffer[..buffer.len() - delim_bytes.len()].to_vec();
                        buffer.clear();

                        process_serial_frame(
                            node_id,
                            device_id,
                            &frame_data,
                            signals,
                            handle,
                            &engine,
                        );
                    }
                }
            }
            Err(ref e) if e.kind() == std::io::ErrorKind::TimedOut => {}
            Err(error) => {
                return Err(format!("串口读取错误: {error}"));
            }
        }
    }
}

/// 处理单帧串口数据：匹配 `SerialCommand` signal → 解码 → scale → emit。
fn process_serial_frame(
    node_id: &str,
    device_id: &str,
    frame_data: &[u8],
    signals: &[FlatSignal],
    handle: &NodeHandle,
    engine: &rhai::Engine,
) {
    let rt = tokio::runtime::Handle::current();

    for sig in signals {
        let decoded = decode_raw_bytes(
            frame_data,
            DataTypeSnapshot::Float32,
            ByteOrderSnapshot::BigEndian,
            None,
        );
        let value = match decoded {
            Ok(v) => v,
            Err(error) => {
                tracing::warn!(
                    node_id,
                    signal_id = %sig.signal_id,
                    ?error,
                    "串口帧解码失败"
                );
                continue;
            }
        };

        let scaled = apply_scale_with_engine(value, sig.scale_ast.as_ref(), engine);
        let value = match scaled {
            Ok(v) => v,
            Err(error) => {
                tracing::warn!(
                    node_id,
                    signal_id = %sig.signal_id,
                    ?error,
                    "串口事件监听 scale 求值失败"
                );
                continue;
            }
        };

        // 构造 payload/metadata 复用 DeviceEventTriggerNode 的工具方法。
        let mut result = serde_json::Map::new();
        result.insert("device_id".to_owned(), serde_json::json!(device_id));
        result.insert("signal_id".to_owned(), serde_json::json!(sig.signal_id));
        result.insert("event_type".to_owned(), serde_json::json!("signal_update"));
        result.insert("value".to_owned(), value);
        if let Some(unit) = &sig.unit {
            result.insert("unit".to_owned(), serde_json::json!(unit));
        }
        result.insert(
            "received_at".to_owned(),
            serde_json::json!(chrono::Utc::now().to_rfc3339()),
        );
        let event_payload = serde_json::Value::Object(result);

        let metadata = serde_json::json!({
            "device_event": {
                "device_id": device_id,
                "signal_id": sig.signal_id,
                "source_type": "serial_command",
                "simulated": false,
            }
        })
        .as_object()
        .cloned()
        .unwrap_or_default();

        rt.block_on(async {
            if let Err(error) = handle.emit(event_payload, metadata).await {
                tracing::warn!(node_id, ?error, "串口事件监听 emit 失败");
            }
        });
    }
}
