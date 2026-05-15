//! MQTT Topic 事件监听后台循环（ADR-0024 Phase 2）。
//!
//! 被 `on_deploy` 通过 orchestrator task spawn。对每个 `Topic` signal
//! 执行 subscribe，收到消息后匹配 topic → 解码 payload → scale 求值 → emit。

use serde_json::Value;

use connections::SharedConnectionManager;
use nazh_core::{CancellationToken, NodeHandle, sleep_or_cancel};

use super::{CompiledSignal, DeviceEventTriggerNode, SignalListenerSnapshot};
use crate::signal_decode::{SignalSourceSnapshot, apply_scale_with_engine};

/// MQTT 事件监听主循环。
///
/// 对 `signals` 中的每个 `Topic` signal 订阅对应 MQTT topic，
/// 收到消息后按 signal 配置解码、scale 求值、emit 到 DAG。
#[allow(clippy::too_many_arguments)]
pub(super) async fn run_mqtt_listener_loop(
    node_id: &str,
    connection_id: &str,
    host: &str,
    port: u16,
    signals: &[CompiledSignal],
    connection_manager: &SharedConnectionManager,
    handle: &NodeHandle,
    token: &CancellationToken,
    device_id: &str,
) {
    let client_id = format!("nazh-evt-{}", truncate_id(node_id, 16));
    let mut mqttoptions = rumqttc::MqttOptions::new(client_id, host.to_owned(), port);
    mqttoptions.set_keep_alive(std::time::Duration::from_secs(30));

    while !token.is_cancelled() {
        let mut guard = match connection_manager.acquire(connection_id).await {
            Ok(g) => g,
            Err(error) => {
                tracing::warn!(node_id, ?error, "MQTT 事件监听：连接借出失败，800ms 后重试");
                sleep_or_cancel(token, std::time::Duration::from_millis(800)).await;
                continue;
            }
        };

        let (client, mut eventloop) = rumqttc::AsyncClient::new(mqttoptions.clone(), 10);

        let connected = wait_connack(&mut eventloop, token).await;
        if !connected {
            let reason = format!("MQTT 事件监听 {host}:{port} 连接失败");
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

        // 订阅所有 signal 的 topic。
        let sub_result = subscribe_all_topics(node_id, signals, &client).await;
        if let Err(reason) = sub_result {
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

        let topics_summary: Vec<&str> = signals
            .iter()
            .filter_map(|cs| {
                if let SignalSourceSnapshot::Topic { topic } = &cs.listener.source {
                    Some(topic.as_str())
                } else {
                    None
                }
            })
            .collect();
        let _ = connection_manager
            .record_connect_success(
                connection_id,
                format!(
                    "MQTT 事件监听已连接，订阅主题: {}",
                    topics_summary.join(", ")
                ),
                None,
            )
            .await;

        // 主消息循环。
        let disconnected = run_message_loop(
            node_id,
            connection_id,
            host,
            port,
            signals,
            connection_manager,
            handle,
            token,
            device_id,
            &mut eventloop,
        )
        .await;

        if token.is_cancelled() {
            guard.mark_success();
            let reason = format!("MQTT 事件监听 {host}:{port} 已停止");
            drop(guard);
            let _ = connection_manager
                .mark_disconnected(connection_id, &reason)
                .await;
            return;
        }

        let reason =
            disconnected.unwrap_or_else(|| format!("MQTT 事件监听 {host}:{port} 连接已断开"));
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

/// 等待 ConnAck，5s 超时 + cancel 监听。
async fn wait_connack(eventloop: &mut rumqttc::EventLoop, token: &CancellationToken) -> bool {
    loop {
        if token.is_cancelled() {
            return false;
        }
        let event = tokio::select! {
            biased;
            () = token.cancelled() => return false,
            event = tokio::time::timeout(std::time::Duration::from_secs(5), eventloop.poll()) => event,
        };
        match event {
            Ok(Ok(rumqttc::Event::Incoming(rumqttc::Packet::ConnAck(ack)))) => {
                return ack.code == rumqttc::ConnectReturnCode::Success;
            }
            Ok(Ok(rumqttc::Event::Incoming(rumqttc::Packet::Disconnect)) | Err(_)) | Err(_) => {
                return false;
            }
            Ok(Ok(_)) => {}
        }
    }
}

/// 主订阅消息循环。返回 `Some(reason)` 表示断连，`None` 表示被 cancel。
#[allow(clippy::too_many_arguments)]
async fn run_message_loop(
    node_id: &str,
    connection_id: &str,
    host: &str,
    port: u16,
    signals: &[CompiledSignal],
    connection_manager: &SharedConnectionManager,
    handle: &NodeHandle,
    token: &CancellationToken,
    device_id: &str,
    eventloop: &mut rumqttc::EventLoop,
) -> Option<String> {
    let heartbeat_interval = std::time::Duration::from_secs(3);
    let mut last_heartbeat_at = std::time::Instant::now();

    loop {
        if token.is_cancelled() {
            return None;
        }
        let event = tokio::select! {
            biased;
            () = token.cancelled() => return None,
            event = tokio::time::timeout(std::time::Duration::from_mins(1), eventloop.poll()) => event,
        };
        match event {
            Ok(Ok(rumqttc::Event::Incoming(rumqttc::Packet::Publish(message)))) => {
                process_mqtt_message(
                    node_id,
                    device_id,
                    &message.topic,
                    &message.payload,
                    signals,
                    handle,
                )
                .await;

                if last_heartbeat_at.elapsed() >= heartbeat_interval {
                    let _ = connection_manager
                        .record_heartbeat(
                            connection_id,
                            format!("MQTT 事件监听 {host}:{port} 收到消息"),
                        )
                        .await;
                    last_heartbeat_at = std::time::Instant::now();
                }
            }
            Ok(Ok(_)) | Err(_) => {}
            Ok(Err(error)) => {
                return Some(format!("MQTT 事件监听事件循环错误: {error}"));
            }
        }
    }
}

/// 处理单条 MQTT 消息：匹配 topic → 解码 → scale → emit。
async fn process_mqtt_message(
    node_id: &str,
    device_id: &str,
    topic: &str,
    payload: &[u8],
    signals: &[CompiledSignal],
    handle: &NodeHandle,
) {
    // 按 topic 匹配 signal。
    let matched: Vec<&CompiledSignal> = signals
        .iter()
        .filter(|cs| {
            if let SignalSourceSnapshot::Topic { topic: sig_topic } = &cs.listener.source {
                sig_topic == topic
            } else {
                false
            }
        })
        .collect();

    if matched.is_empty() {
        tracing::debug!(node_id, %topic, "MQTT 事件监听：topic 无匹配 signal，跳过");
        return;
    }

    for cs in matched {
        let value = decode_mqtt_payload(payload, &cs.listener);

        let scaled = apply_scale_with_engine(value, &cs.scale_ast, &cs.engine);
        let value = match scaled {
            Ok(v) => v,
            Err(error) => {
                tracing::warn!(
                    node_id,
                    signal_id = %cs.listener.signal_id,
                    ?error,
                    "MQTT 事件监听 scale 求值失败"
                );
                continue;
            }
        };

        let event_payload =
            DeviceEventTriggerNode::build_event_payload(device_id, &cs.listener, value);
        let metadata = DeviceEventTriggerNode::build_event_metadata(device_id, &cs.listener, false);

        if let Err(error) = handle.emit(event_payload, metadata).await {
            tracing::warn!(node_id, ?error, "MQTT 事件监听 emit 失败");
        }
    }
}

/// 解码 MQTT payload 字节为 JSON Value。
///
/// Topic 信号通常为纯数值载荷。若 payload 可解析为 JSON 数字则直接使用；
/// 否则尝试按 UTF-8 字符串解码；最后回退到原始字节十六进制表示。
fn decode_mqtt_payload(payload: &[u8], listener: &SignalListenerSnapshot) -> Value {
    // 优先尝试 JSON 解析。
    if let Ok(parsed) = serde_json::from_slice::<Value>(payload) {
        match parsed {
            v @ (Value::Number(_) | Value::Bool(_)) => return v,
            Value::String(s) => {
                // 尝试把字符串内容再解析为数值（如 "42.5"）。
                if let Some(n) = serde_json::Number::from_f64(s.parse::<f64>().unwrap_or(f64::NAN))
                {
                    return Value::Number(n);
                }
                return Value::String(s);
            }
            other => return other,
        }
    }

    // 回退：UTF-8 字符串。
    if let Ok(s) = std::str::from_utf8(payload) {
        if let Some(n) = serde_json::Number::from_f64(s.parse::<f64>().unwrap_or(f64::NAN)) {
            return Value::Number(n);
        }
        return Value::String(s.to_owned());
    }

    // Topic 源无 data_type 信息，回退到十六进制。
    let hex: String =
        payload
            .iter()
            .fold(String::with_capacity(payload.len() * 2), |mut acc, b| {
                use std::fmt::Write;
                let _ = write!(acc, "{b:02X}");
                acc
            });
    tracing::warn!(
        signal_id = %listener.signal_id,
        hex,
        "MQTT payload 无法解码，使用十六进制回退"
    );
    Value::String(hex)
}

/// 按**字符**截断节点 ID 用作 MQTT `client_id` 后缀。
fn truncate_id(id: &str, max_chars: usize) -> String {
    id.chars().take(max_chars).collect()
}

/// 订阅所有 signal 的 topic。第一个失败即返回错误。
async fn subscribe_all_topics(
    _node_id: &str,
    signals: &[CompiledSignal],
    client: &rumqttc::AsyncClient,
) -> Result<(), String> {
    for cs in signals {
        if let SignalSourceSnapshot::Topic { topic } = &cs.listener.source
            && let Err(error) = client.subscribe(topic, rumqttc::QoS::AtMostOnce).await
        {
            return Err(format!("MQTT 事件监听订阅 `{topic}` 失败: {error}"));
        }
    }
    Ok(())
}
