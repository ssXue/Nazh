//! MQTT 客户端节点，支持发布和订阅两种工作模式。
//!
//! ## 工作模式
//!
//! - **发布模式（publish）**：`transform` 调用 broker 发布 payload，`on_deploy`
//!   返回 noop guard。
//! - **订阅模式（subscribe）**：`on_deploy` 中建立 broker 长连接 + 订阅 topic，
//!   收到消息后通过 `NodeHandle::emit` 推进 DAG。`transform` 路径仍可被手动
//!   dispatch 调用（带 `_mqtt_message` payload）并得到等价输出。
//!
//! ## 背压策略说明
//!
//! 同 [`crate::TimerNode`] / [`crate::SerialTriggerNode`]：emit 走 `NodeHandle`
//! 而非 `WorkflowDispatchRouter` 的 trigger lane，后者的 backpressure / DLQ /
//! retry / metrics 在本节点不生效。`MQTT` broker 端 `QoS` 与本端 channel buffer
//! 已提供基础背压，DLQ / retry 几乎无触发场景。

use async_trait::async_trait;
use chrono::Utc;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};
use uuid::Uuid;

use connections::{SharedConnectionManager, connection_metadata};
use nazh_core::{
    CancellationToken, EngineError, LifecycleGuard, NodeExecution, NodeHandle,
    NodeLifecycleContext, NodeTrait, into_payload_map, sleep_or_cancel,
};

/// MQTT 客户端工作模式。
///
/// 用 enum 而非字符串避免 typo 静默退化（如 `"subscrib"` 会被字符串比较
/// 默认走 publish 路径——enum 在反序列化时直接拒绝）。
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MqttMode {
    #[default]
    Publish,
    Subscribe,
}

fn default_mqtt_qos() -> u8 {
    0
}

fn normalize_mqtt_qos(value: u8) -> rumqttc::QoS {
    match value {
        1 => rumqttc::QoS::AtLeastOnce,
        2 => rumqttc::QoS::ExactlyOnce,
        _ => rumqttc::QoS::AtMostOnce,
    }
}

/// 按**字符**截断节点 ID 用作 `MQTT` `client_id` 后缀。
///
/// 直接用 `&id[..N]` 字节切片在中文 / Emoji 等多字节字符落在 N 边界时会 panic
/// （CLAUDE.md 显式允许中文 ID，如 `"温度采集"`）。
fn truncate_client_id(id: &str, max_chars: usize) -> String {
    id.chars().take(max_chars).collect()
}

fn extract_broker_addr(metadata: &Value) -> Result<(String, u16), EngineError> {
    let host = metadata
        .get("host")
        .and_then(Value::as_str)
        .unwrap_or_default();
    if host.is_empty() {
        return Err(EngineError::node_config(
            String::new(),
            "MQTT 连接元数据缺少 host".to_owned(),
        ));
    }

    let port = metadata
        .get("port")
        .and_then(Value::as_u64)
        .and_then(|p| u16::try_from(p).ok())
        .unwrap_or(1883);

    Ok((host.to_owned(), port))
}

/// MQTT 客户端节点配置。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MqttClientNodeConfig {
    #[serde(default)]
    pub connection_id: Option<String>,
    /// 工作模式：`publish` 或 `subscribe`（不区分大小写反序列化）。
    #[serde(default)]
    pub mode: MqttMode,
    /// 订阅或发布的主题。
    #[serde(default)]
    pub topic: String,
    /// `QoS` 级别（`0`、`1`、`2`）。
    #[serde(default = "default_mqtt_qos")]
    pub qos: u8,
    /// 发布模式下的载荷模板（预留）。
    #[serde(default)]
    pub payload_template: String,
}

/// MQTT 客户端节点。
pub struct MqttClientNode {
    id: String,
    config: MqttClientNodeConfig,
    connection_manager: SharedConnectionManager,
}

impl MqttClientNode {
    pub fn new(
        id: impl Into<String>,
        config: MqttClientNodeConfig,
        connection_manager: SharedConnectionManager,
    ) -> Self {
        Self {
            id: id.into(),
            config,
            connection_manager,
        }
    }

    /// 发布模式：将 payload 发布到 MQTT broker。
    async fn publish_payload(
        &self,
        trace_id: Uuid,
        guard: &mut connections::ConnectionGuard,
        payload: Value,
    ) -> Result<Value, EngineError> {
        let (host, port) = extract_broker_addr(&guard.lease().metadata)?;

        let topic = if self.config.topic.is_empty() {
            guard
                .lease()
                .metadata
                .get("topic")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_owned()
        } else {
            self.config.topic.clone()
        };

        if topic.is_empty() {
            return Err(EngineError::node_config(
                self.id.clone(),
                "MQTT 主题不能为空".to_owned(),
            ));
        }

        let mqtt_payload = serde_json::to_vec(&payload).map_err(|error| {
            EngineError::stage_execution(
                self.id.clone(),
                trace_id,
                format!("MQTT 载荷序列化失败: {error}"),
            )
        })?;

        let client_id = format!("nazh-{}", truncate_client_id(&self.id, 20));
        let mut mqttoptions = rumqttc::MqttOptions::new(client_id, host, port);
        mqttoptions.set_keep_alive(std::time::Duration::from_secs(5));

        let (client, mut eventloop) = rumqttc::AsyncClient::new(mqttoptions, 10);

        // 等待连接确认
        tokio::time::timeout(std::time::Duration::from_secs(10), async {
            loop {
                match eventloop.poll().await {
                    Ok(rumqttc::Event::Incoming(rumqttc::Packet::ConnAck(ack))) => {
                        if ack.code == rumqttc::ConnectReturnCode::Success {
                            return Ok(());
                        }
                        return Err(format!("MQTT broker 拒绝连接: {:?}", ack.code));
                    }
                    Ok(rumqttc::Event::Incoming(rumqttc::Packet::Disconnect)) => {
                        return Err("MQTT broker 断开连接".to_owned());
                    }
                    Err(error) => {
                        return Err(format!("MQTT 连接错误: {error}"));
                    }
                    _ => {}
                }
            }
        })
        .await
        .map_err(|_| {
            EngineError::stage_execution(
                self.id.clone(),
                trace_id,
                "MQTT 连接超时（10 秒）".to_owned(),
            )
        })?
        .map_err(|msg: String| EngineError::stage_execution(self.id.clone(), trace_id, msg))?;

        let qos = normalize_mqtt_qos(self.config.qos);
        client
            .publish(topic.clone(), qos, false, mqtt_payload)
            .await
            .map_err(|error| {
                EngineError::stage_execution(
                    self.id.clone(),
                    trace_id,
                    format!("MQTT 发布失败: {error}"),
                )
            })?;

        // 等待发送完成
        let _ = tokio::time::timeout(std::time::Duration::from_secs(5), eventloop.poll()).await;

        Ok(json!({
            "published_topic": topic,
            "published_at": Utc::now().to_rfc3339(),
        }))
    }

    /// 订阅模式：处理从 Tauri 壳层注入的 `_mqtt_message` payload。
    fn normalize_subscribed_payload(payload: Value) -> Value {
        let mut payload_map = into_payload_map(payload);

        if let Some(mqtt_msg) = payload_map.remove("_mqtt_message") {
            let msg_obj = match mqtt_msg {
                Value::Object(map) => map,
                other => {
                    let mut map = Map::new();
                    map.insert("raw".to_owned(), other);
                    map
                }
            };

            // 尝试解析 payload 字段中的 JSON
            if let Some(text) = msg_obj.get("payload").and_then(Value::as_str) {
                if let Ok(parsed) = serde_json::from_str::<Value>(text) {
                    if let Value::Object(parsed_map) = parsed {
                        for (key, value) in parsed_map {
                            payload_map.insert(key, value);
                        }
                    } else {
                        payload_map.insert("mqtt_payload".to_owned(), parsed);
                    }
                } else {
                    payload_map.insert("mqtt_payload".to_owned(), Value::String(text.to_owned()));
                }
            }

            payload_map.insert(
                "mqtt_topic".to_owned(),
                msg_obj.get("topic").cloned().unwrap_or(Value::Null),
            );
            payload_map.insert(
                "mqtt_received_at".to_owned(),
                msg_obj.get("received_at").cloned().unwrap_or(Value::Null),
            );
        }

        Value::Object(payload_map)
    }

    /// 把单条收到的 MQTT 消息封装成 `_mqtt_message` 后再走
    /// [`normalize_subscribed_payload`]——确保 emit 路径与 transform 路径
    /// 输出**结构等价**。
    fn build_message_payload(topic: &str, body: &[u8], qos: u8, retain: bool) -> Value {
        let payload_text = String::from_utf8_lossy(body).to_string();
        let received_at = Utc::now().to_rfc3339();
        let envelope = json!({
            "_mqtt_message": {
                "topic": topic,
                "payload": payload_text,
                "qos": qos,
                "retain": retain,
                "received_at": received_at,
            }
        });
        Self::normalize_subscribed_payload(envelope)
    }

    /// `on_deploy` 共用的订阅模式 metadata（含 `mode` / `topic` / `processed_at`）。
    fn subscribe_metadata(&self) -> Map<String, Value> {
        Map::from_iter([(
            "mqtt".to_owned(),
            json!({
                "mode": "subscribe",
                "topic": self.config.topic,
                "processed_at": Utc::now().to_rfc3339(),
            }),
        )])
    }
}

#[async_trait]
impl NodeTrait for MqttClientNode {
    nazh_core::impl_node_meta!("mqttClient");

    async fn transform(
        &self,
        trace_id: Uuid,
        payload: Value,
    ) -> Result<NodeExecution, EngineError> {
        let is_subscribe = matches!(self.config.mode, MqttMode::Subscribe);

        if is_subscribe {
            let result = Self::normalize_subscribed_payload(payload);

            let mut metadata = Map::from_iter([(
                "mqtt".to_owned(),
                json!({
                    "mode": "subscribe",
                    "topic": self.config.topic,
                    "processed_at": Utc::now().to_rfc3339(),
                }),
            )]);

            if let Some(conn_id) = &self.config.connection_id {
                let guard = self.connection_manager.acquire(conn_id).await?;
                let (key, value) = connection_metadata(&self.id, guard.lease())?;
                metadata.insert(key, value);
            }

            return Ok(NodeExecution::broadcast(result).with_metadata(metadata));
        }

        // 发布模式：必须绑定连接资源
        let Some(conn_id) = &self.config.connection_id else {
            return Err(EngineError::node_config(
                self.id.clone(),
                "MQTT 发布节点必须绑定连接资源".to_owned(),
            ));
        };

        let mut guard = self.connection_manager.acquire(conn_id).await?;
        let publish_info = self
            .publish_payload(trace_id, &mut guard, payload.clone())
            .await?;

        let mut metadata = Map::from_iter([(
            "mqtt".to_owned(),
            json!({
                "mode": "publish",
                "publish_info": publish_info,
            }),
        )]);

        let (key, value) = connection_metadata(&self.id, guard.lease())?;
        metadata.insert(key, value);
        guard.mark_success();

        Ok(NodeExecution::broadcast(payload).with_metadata(metadata))
    }

    async fn on_deploy(&self, ctx: NodeLifecycleContext) -> Result<LifecycleGuard, EngineError> {
        // 仅 subscribe 模式建连——publish 模式 transform 时按需借用
        if !matches!(self.config.mode, MqttMode::Subscribe) {
            return Ok(LifecycleGuard::noop());
        }

        // 1. 必须有 connection_id（无 connection 不能订阅）
        let Some(connection_id_str) = self.config.connection_id.as_deref() else {
            return Err(EngineError::node_config(
                self.id.clone(),
                "MQTT 订阅节点必须绑定连接资源".to_owned(),
            ));
        };
        let connection_id = connection_id_str.to_owned();

        // 2. async 阶段同步预校验：复刻原壳层 collect_mqtt_root_specs 中的
        //    "借连接 → 读 metadata（host/port/topic）→ 校验 → mark"
        let (host, port, topic, qos) = {
            let mut guard = self
                .connection_manager
                .acquire(&connection_id)
                .await
                .map_err(|error| {
                    EngineError::node_config(
                        self.id.clone(),
                        format!("MQTT 连接资源 `{connection_id}` 借出失败: {error}"),
                    )
                })?;

            let host = guard
                .metadata()
                .get("host")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_owned();
            let port = guard
                .metadata()
                .get("port")
                .and_then(Value::as_u64)
                .and_then(|p| u16::try_from(p).ok())
                .unwrap_or(1883);
            let topic = if self.config.topic.is_empty() {
                guard
                    .metadata()
                    .get("topic")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_owned()
            } else {
                self.config.topic.clone()
            };

            if topic.is_empty() {
                let reason = format!("MQTT 订阅节点 `{}` 的主题不能为空", self.id);
                guard.mark_failure(&reason);
                return Err(EngineError::node_config(self.id.clone(), reason));
            }
            if host.is_empty() {
                let reason = format!("MQTT 连接资源 `{connection_id}` 缺少 host 配置");
                guard.mark_failure(&reason);
                return Err(EngineError::node_config(self.id.clone(), reason));
            }
            guard.mark_success();
            (host, port, topic, normalize_mqtt_qos(self.config.qos))
        };

        // 3. spawn 后台订阅任务
        let id = self.id.clone();
        let connection_manager = self.connection_manager.clone();
        let handle = ctx.handle.clone();
        let token = ctx.shutdown.clone();
        let metadata_template = self.subscribe_metadata();
        let join = tokio::spawn(mqtt_subscribe_loop(
            id,
            connection_id,
            host,
            port,
            topic,
            qos,
            connection_manager,
            handle,
            token,
            metadata_template,
        ));

        Ok(LifecycleGuard::from_task(ctx.shutdown, join))
    }
}

/// MQTT 订阅后台循环（被 `on_deploy` 通过 `tokio::spawn` 拉起）。
///
/// 重连退避：连接失败后调用 `record_connect_failure` 取建议退避时长，否则 800ms。
/// 心跳上报：每条收到的消息触发一次 `record_heartbeat`。
/// 取消：`token.cancelled()` 在 `tokio::select!` 第一分支监听，确保撤销响应快。
#[allow(clippy::too_many_arguments)]
async fn mqtt_subscribe_loop(
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
            event = tokio::time::timeout(std::time::Duration::from_secs(60), eventloop.poll()) => event,
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
