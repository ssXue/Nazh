//! MQTT 客户端节点，支持发布和订阅两种工作模式。
//!
//! - **发布模式（publish）**：作为普通变换节点，将 payload 发布到 MQTT broker。
//! - **订阅模式（subscribe）**：作为触发节点，由 Tauri 壳层在部署时启动后台订阅任务，
//!   收到消息后注入 `_mqtt_message` 到 payload 并投递到 DAG。

use async_trait::async_trait;
use chrono::Utc;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};
use uuid::Uuid;

use connections::{SharedConnectionManager, connection_metadata};
use nazh_core::{EngineError, NodeExecution, NodeTrait, into_payload_map};

fn default_mqtt_mode() -> String {
    "publish".to_owned()
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
    /// 工作模式: "publish" 或 "subscribe"。
    #[serde(default = "default_mqtt_mode")]
    pub mode: String,
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

        let client_id = format!("nazh-{}", &self.id[..self.id.len().min(20)]);
        let mut mqttoptions = rumqttc::MqttOptions::new(client_id, host, port);
        mqttoptions.set_keep_alive(std::time::Duration::from_secs(5));

        let (client, mut eventloop) = rumqttc::AsyncClient::new(mqttoptions, 10);

        // 等待连接确认
        tokio::time::timeout(
            std::time::Duration::from_secs(10),
            async {
                loop {
                    match eventloop.poll().await {
                        Ok(rumqttc::Event::Incoming(rumqttc::Packet::ConnAck(ack))) => {
                            if ack.code == rumqttc::ConnectReturnCode::Success {
                                return Ok(());
                            }
                            return Err(format!(
                                "MQTT broker 拒绝连接: {:?}",
                                ack.code
                            ));
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
            },
        )
        .await
        .map_err(|_| {
            EngineError::stage_execution(
                self.id.clone(),
                trace_id,
                "MQTT 连接超时（10 秒）".to_owned(),
            )
        })?
        .map_err(|msg: String| {
            EngineError::stage_execution(self.id.clone(), trace_id, msg)
        })?;

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
        let _ = tokio::time::timeout(
            std::time::Duration::from_secs(5),
            eventloop.poll(),
        )
        .await;

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
}

#[async_trait]
impl NodeTrait for MqttClientNode {
    nazh_core::impl_node_meta!("mqttClient");

    async fn transform(
        &self,
        trace_id: Uuid,
        payload: Value,
    ) -> Result<NodeExecution, EngineError> {
        let is_subscribe = self.config.mode.trim().eq_ignore_ascii_case("subscribe");

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
        let publish_info = self.publish_payload(trace_id, &mut guard, payload.clone()).await?;

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
}
