use serde_json::{Map, Value};

use connections::SharedConnectionManager;
use nazh_core::{CancellationToken, NodeHandle, sleep_or_cancel};

use super::{MqttClientNode, truncate_client_id};

/// MQTT 订阅后台循环（被 `on_deploy` 通过 `tokio::spawn` 拉起）。
///
/// 重连退避：连接失败后调用 `record_connect_failure` 取建议退避时长，否则 800ms。
/// 心跳上报：每条收到的消息触发一次 `record_heartbeat`。
/// 取消：`token.cancelled()` 在 `tokio::select!` 第一分支监听，确保撤销响应快。
#[allow(clippy::too_many_arguments)]
pub(super) async fn mqtt_subscribe_loop(
    node_id: String,
    connection_id: String,
    host: String,
    port: u16,
    topic: String,
    qos: rumqttc::QoS,
    connection_manager: SharedConnectionManager,
    handle: NodeHandle,
    token: CancellationToken,
    metadata_template: Map<String, Value>,
) {
    let client_id = format!("nazh-sub-{}", truncate_client_id(&node_id, 16));
    let mut mqttoptions = rumqttc::MqttOptions::new(client_id, host.clone(), port);
    mqttoptions.set_keep_alive(std::time::Duration::from_secs(30));

    while !token.is_cancelled() {
        let mut guard = match connection_manager.acquire(&connection_id).await {
            Ok(guard) => guard,
            Err(error) => {
                tracing::warn!(node_id = %node_id, ?error, "MQTT 连接借出失败，800ms 后重试");
                sleep_or_cancel(&token, std::time::Duration::from_millis(800)).await;
                continue;
            }
        };

        let (client, mut eventloop) = rumqttc::AsyncClient::new(mqttoptions.clone(), 10);

        // 等 ConnAck（带超时和 cancel 监听）
        let connected = wait_connack(&mut eventloop, &token).await;

        if !connected {
            let reason = format!("MQTT {host}:{port} 连接失败");
            guard.mark_failure(&reason);
            let retry_after_ms = connection_manager
                .record_connect_failure(&connection_id, &reason)
                .await
                .unwrap_or(800);
            drop(guard);
            tracing::warn!(node_id = %node_id, %reason, retry_after_ms);
            sleep_or_cancel(&token, std::time::Duration::from_millis(retry_after_ms)).await;
            continue;
        }

        let _ = connection_manager
            .record_connect_success(
                &connection_id,
                format!("MQTT {host}:{port} 已连接，订阅主题 {topic}"),
                None,
            )
            .await;

        if let Err(error) = client.subscribe(&topic, qos).await {
            let reason = format!("MQTT 订阅主题 `{topic}` 失败: {error}");
            guard.mark_failure(&reason);
            let retry_after_ms = connection_manager
                .record_connect_failure(&connection_id, &reason)
                .await
                .unwrap_or(800);
            drop(guard);
            tracing::warn!(node_id = %node_id, %reason, retry_after_ms);
            sleep_or_cancel(&token, std::time::Duration::from_millis(retry_after_ms)).await;
            continue;
        }

        // 主消息循环
        let disconnected_reason = run_message_loop(
            &node_id,
            &connection_id,
            &topic,
            &host,
            port,
            &mut eventloop,
            &connection_manager,
            &handle,
            &token,
            &metadata_template,
        )
        .await;

        if token.is_cancelled() {
            guard.mark_success();
            let reason = format!("MQTT {host}:{port} 订阅已停止");
            drop(guard);
            let _ = connection_manager
                .mark_disconnected(&connection_id, &reason)
                .await;
            return;
        }

        let reason =
            disconnected_reason.unwrap_or_else(|| format!("MQTT {host}:{port} 连接已断开"));
        guard.mark_failure(&reason);
        let retry_after_ms = connection_manager
            .record_connect_failure(&connection_id, &reason)
            .await
            .unwrap_or(800);
        drop(guard);
        tracing::warn!(node_id = %node_id, %reason, retry_after_ms);
        sleep_or_cancel(&token, std::time::Duration::from_millis(retry_after_ms)).await;
    }
}

/// 等待 ConnAck，带 5s 超时与 cancel 监听。返回是否成功。
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

/// 主订阅消息循环。返回 `Some(reason)` 表示连接断开（外层重连），`None` 表示
/// 被 cancel（外层退出）。
#[allow(clippy::too_many_arguments)]
async fn run_message_loop(
    node_id: &str,
    connection_id: &str,
    topic: &str,
    host: &str,
    port: u16,
    eventloop: &mut rumqttc::EventLoop,
    connection_manager: &SharedConnectionManager,
    handle: &NodeHandle,
    token: &CancellationToken,
    metadata_template: &Map<String, Value>,
) -> Option<String> {
    // heartbeat 节流：高吞吐 topic 上每条消息都写心跳会让 ConnectionManager
    // 承担过度的写锁压力。改为按固定间隔节流，与 serial 读循环保持一致语义。
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
                let payload = MqttClientNode::build_message_payload(
                    &message.topic,
                    &message.payload,
                    message.qos as u8,
                    message.retain,
                );
                if let Err(error) = handle.emit(payload, metadata_template.clone()).await {
                    tracing::warn!(node_id = %node_id, ?error, "MQTT emit 失败");
                }
                if last_heartbeat_at.elapsed() >= heartbeat_interval {
                    let _ = connection_manager
                        .record_heartbeat(
                            connection_id,
                            format!("MQTT {host}:{port} 收到主题 {topic} 消息"),
                        )
                        .await;
                    last_heartbeat_at = std::time::Instant::now();
                }
                tracing::info!(node_id = %node_id, %topic, "MQTT 消息已投递到 DAG");
            }
            Ok(Ok(_)) | Err(_) => {}
            Ok(Err(error)) => {
                return Some(format!("MQTT 事件循环错误: {error}"));
            }
        }
    }
}
