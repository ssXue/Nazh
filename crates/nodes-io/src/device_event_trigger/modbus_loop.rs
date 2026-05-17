//! Modbus Register 定时轮询后台循环（ADR-0024 Phase 3）。
//!
//! 被 `on_deploy` 通过 orchestrator task spawn。按 `poll_interval_ms` 间隔
//! 遍历 Register 信号，建立 Modbus TCP 连接读取寄存器 → 解码 → scale 求值 → emit。

use connections::SharedConnectionManager;
use nazh_core::{CancellationToken, NodeHandle, sleep_or_cancel};

use super::{CompiledSignal, DeviceEventTriggerNode};
use crate::signal_decode::{
    ByteOrderSnapshot, SignalSourceSnapshot, apply_scale_with_engine, decode_raw_bytes,
};

/// Modbus Register 定时轮询主循环。
#[allow(clippy::too_many_arguments)]
pub(super) async fn run_modbus_poll_loop(
    node_id: &str,
    connection_id: &str,
    signals: &[CompiledSignal],
    connection_manager: &SharedConnectionManager,
    handle: &NodeHandle,
    token: &CancellationToken,
    device_id: &str,
    poll_interval_ms: u64,
) {
    let mut interval =
        tokio::time::interval(std::time::Duration::from_millis(poll_interval_ms.max(100)));

    loop {
        tokio::select! {
            biased;
            () = token.cancelled() => return,
            _ = interval.tick() => {}
        }

        if let Err(error) = poll_all_signals(
            node_id,
            connection_id,
            signals,
            connection_manager,
            handle,
            token,
            device_id,
        )
        .await
        {
            tracing::warn!(node_id, ?error, "Modbus 事件轮询失败");
            let retry_ms = connection_manager
                .record_connect_failure(connection_id, &error)
                .await
                .unwrap_or(800);
            sleep_or_cancel(token, std::time::Duration::from_millis(retry_ms)).await;
        }
    }
}

/// 单次轮询：连接 → 遍历 Register 信号 → 读取 → 解码 → emit。
async fn poll_all_signals(
    node_id: &str,
    connection_id: &str,
    signals: &[CompiledSignal],
    connection_manager: &SharedConnectionManager,
    handle: &NodeHandle,
    token: &CancellationToken,
    device_id: &str,
) -> Result<(), String> {
    let mut guard = connection_manager
        .acquire(connection_id)
        .await
        .map_err(|e| format!("Modbus 事件轮询连接借出失败: {e}"))?;

    let metadata = guard.lease().metadata.clone();
    let host = metadata
        .get("host")
        .and_then(|v| v.as_str())
        .unwrap_or_default();
    let port = metadata
        .get("port")
        .and_then(serde_json::Value::as_u64)
        .and_then(|p| u16::try_from(p).ok())
        .ok_or_else(|| "Modbus 连接元数据缺少有效的 port".to_owned())?;

    if token.is_cancelled() {
        guard.mark_success();
        return Ok(());
    }

    // 按单元分组信号以减少连接次数。
    let mut any_error = false;
    for cs in signals {
        let (register, data_type, bit) = match &cs.listener.source {
            SignalSourceSnapshot::Register {
                register,
                data_type,
                bit,
            } => (*register, *data_type, *bit),
            _ => continue,
        };

        let quantity = data_type.modbus_register_count();
        let unit_id = extract_unit_id(&metadata)?;

        match read_register_once(host, port, unit_id, register, quantity).await {
            Ok(raw_bytes) => {
                let decoded =
                    decode_raw_bytes(&raw_bytes, data_type, ByteOrderSnapshot::BigEndian, bit);
                let value = match decoded {
                    Ok(v) => v,
                    Err(error) => {
                        tracing::warn!(
                            node_id,
                            signal_id = %cs.listener.signal_id,
                            ?error,
                            "Modbus 事件轮询解码失败"
                        );
                        any_error = true;
                        continue;
                    }
                };

                let scaled = apply_scale_with_engine(value, cs.scale_ast.as_ref(), &cs.engine);
                let value = match scaled {
                    Ok(v) => v,
                    Err(error) => {
                        tracing::warn!(
                            node_id,
                            signal_id = %cs.listener.signal_id,
                            ?error,
                            "Modbus 事件轮询 scale 求值失败"
                        );
                        any_error = true;
                        continue;
                    }
                };

                let event_payload =
                    DeviceEventTriggerNode::build_event_payload(device_id, &cs.listener, value);
                let metadata =
                    DeviceEventTriggerNode::build_event_metadata(device_id, &cs.listener, false);

                if let Err(error) = handle.emit(event_payload, metadata).await {
                    tracing::warn!(node_id, ?error, "Modbus 事件轮询 emit 失败");
                }
            }
            Err(error) => {
                tracing::warn!(
                    node_id,
                    signal_id = %cs.listener.signal_id,
                    %error,
                    "Modbus 事件轮询读取失败"
                );
                any_error = true;
            }
        }
    }

    if any_error {
        guard.mark_failure("Modbus 事件轮询部分信号失败");
        Err("Modbus 事件轮询部分信号失败".to_owned())
    } else {
        guard.mark_success();
        let _ = connection_manager
            .record_heartbeat(connection_id, format!("Modbus 事件轮询 {host}:{port} 正常"))
            .await;
        Ok(())
    }
}

/// 从 connection metadata 提取 Modbus 单元 ID。
fn extract_unit_id(metadata: &serde_json::Value) -> Result<u8, String> {
    metadata
        .get("unit")
        .and_then(serde_json::Value::as_u64)
        .and_then(|u| u8::try_from(u).ok())
        .filter(|unit| *unit > 0)
        .ok_or_else(|| "Modbus 连接元数据缺少有效的 unit".to_owned())
}

/// 单次 Modbus TCP 寄存器读取。
async fn read_register_once(
    host: &str,
    port: u16,
    unit_id: u8,
    register: u16,
    quantity: u16,
) -> Result<Vec<u8>, String> {
    use tokio_modbus::client::Reader;

    let socket_addr = std::net::SocketAddr::from((
        host.parse::<std::net::IpAddr>()
            .map_err(|e| format!("Modbus TCP 地址解析失败 ({host}): {e}"))?,
        port,
    ));

    let slave = tokio_modbus::Slave(unit_id);
    let mut ctx = tokio_modbus::client::tcp::connect_slave(socket_addr, slave)
        .await
        .map_err(|e| format!("Modbus TCP 连接失败 ({host}:{port}): {e}"))?;

    let words = ctx
        .read_holding_registers(register, quantity)
        .await
        .map_err(|e| format!("Modbus 读保持寄存器失败: {e}"))?
        .map_err(|e| format!("Modbus 协议错误: {e}"))?;

    Ok(words.iter().flat_map(|w| w.to_be_bytes()).collect())
}
