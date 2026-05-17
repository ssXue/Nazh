//! CAN Frame 事件监听后台循环（ADR-0024 Phase 2）。
//!
//! 被 `on_deploy` 通过 orchestrator task spawn。通过共享 CAN 会话接收帧，
//! 按 `can_id` 匹配 signal → 提取字节切片 → 解码 → scale 求值 → emit。

use connections::SharedConnectionManager;
use nazh_core::{CancellationToken, NodeHandle};

use super::{CompiledSignal, DeviceEventTriggerNode};
use crate::can::session::CanBusRuntime;
use crate::signal_decode::{SignalSourceSnapshot, apply_scale_with_engine, decode_raw_bytes};

/// CAN 帧事件监听主循环。
#[allow(clippy::too_many_arguments)]
pub(super) async fn run_can_listener_loop(
    node_id: &str,
    connection_id: &str,
    signals: &[CompiledSignal],
    connection_manager: &SharedConnectionManager,
    handle: &NodeHandle,
    token: &CancellationToken,
    device_id: &str,
) {
    let runtime = CanBusRuntime::new(connection_manager.clone(), connection_id.to_owned());
    let session = match runtime.ensure_session(node_id, |_| Ok(())).await {
        Ok(s) => s,
        Err(error) => {
            tracing::error!(node_id, ?error, "CAN 事件监听会话建立失败");
            return;
        }
    };

    let bus = match session.bus_handle(node_id).await {
        Ok(b) => b,
        Err(error) => {
            tracing::error!(node_id, ?error, "CAN 事件监听获取总线句柄失败");
            return;
        }
    };

    tracing::info!(node_id, connection_id, "CAN 事件监听已启动");

    let recv_timeout = std::time::Duration::from_millis(100);

    loop {
        if token.is_cancelled() {
            break;
        }

        let frame_result = bus.recv(recv_timeout).await;
        match frame_result {
            Ok(Some(frame)) => {
                process_can_frame(node_id, device_id, &frame, signals, handle).await;
            }
            Ok(None) => {}
            Err(error) => {
                tracing::warn!(node_id, ?error, "CAN 事件监听接收错误");
                let err_str = error.to_string();
                if let Err(e) = connection_manager
                    .record_connect_failure(connection_id, &err_str)
                    .await
                {
                    tracing::warn!(node_id, ?e, "CAN 事件监听记录失败");
                }
                runtime.shutdown().await;
                return;
            }
        }
    }
}

/// 处理单帧 CAN 数据：按 `can_id` 匹配 signal → 解码 → scale → emit。
async fn process_can_frame(
    node_id: &str,
    device_id: &str,
    frame: &crate::can::CanFrame,
    signals: &[CompiledSignal],
    handle: &NodeHandle,
) {
    let matched: Vec<&CompiledSignal> = signals
        .iter()
        .filter(|cs| {
            if let SignalSourceSnapshot::CanFrame {
                can_id,
                is_extended,
                ..
            } = &cs.listener.source
            {
                frame.id == *can_id && frame.is_extended == *is_extended
            } else {
                false
            }
        })
        .collect();

    if matched.is_empty() {
        return;
    }

    for cs in matched {
        if let SignalSourceSnapshot::CanFrame {
            byte_offset,
            byte_length,
            data_type,
            byte_order,
            ..
        } = &cs.listener.source
        {
            let start = usize::from(*byte_offset);
            let end = start + usize::from(*byte_length);
            if end > frame.data.len() {
                tracing::warn!(
                    node_id,
                    signal_id = %cs.listener.signal_id,
                    can_id = frame.id,
                    byte_offset = *byte_offset,
                    byte_length = *byte_length,
                    frame_len = frame.data.len(),
                    "CAN 帧 payload 越界，跳过"
                );
                continue;
            }

            let raw = &frame.data[start..end];
            let decoded = decode_raw_bytes(raw, *data_type, *byte_order, None);
            let value = match decoded {
                Ok(v) => v,
                Err(error) => {
                    tracing::warn!(
                        node_id,
                        signal_id = %cs.listener.signal_id,
                        ?error,
                        "CAN 帧解码失败"
                    );
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
                        "CAN 事件监听 scale 求值失败"
                    );
                    continue;
                }
            };

            let event_payload =
                DeviceEventTriggerNode::build_event_payload(device_id, &cs.listener, value);
            let metadata =
                DeviceEventTriggerNode::build_event_metadata(device_id, &cs.listener, false);

            if let Err(error) = handle.emit(event_payload, metadata).await {
                tracing::warn!(node_id, ?error, "CAN 事件监听 emit 失败");
            }
        }
    }
}
