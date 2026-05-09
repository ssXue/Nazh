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
    EngineError, LifecycleGuard, NodeExecution, NodeLifecycleContext, NodeTrait, PinDefinition,
    PinType, into_payload_map,
};

mod subscribe;

use subscribe::mqtt_subscribe_loop;

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

    /// 输入引脚按 [`MqttMode`] 分支：
    ///
    /// - `Publish` 模式 → `Json`：必须收到结构化 payload 才能发布到 broker
    /// - `Subscribe` 模式 → `Any`：subscribe 由 [`Self::on_deploy`] 触发，
    ///   `transform` 路径仅在手动 dispatch 时被调用，input 形状不重要
    ///
    /// pin 类型由 config 决定（mode 切换会镜像翻转输入输出形态），所以
    /// `input_pins` / `output_pins` 必须读 `&self.config` 才能给出准确声明——
    /// 这是 [`NodeTrait`] 把这两个方法设计为 `&self` 实例方法（而非 `'static`
    /// 表）的典型场景。
    fn input_pins(&self) -> Vec<PinDefinition> {
        let (pin_type, description) = match self.config.mode {
            MqttMode::Publish => (PinType::Json, "要发布到 broker 的 payload（JSON 对象）"),
            MqttMode::Subscribe => (
                PinType::Any,
                "trigger / 手动 dispatch 信号；订阅模式实际触发走 on_deploy",
            ),
        };
        vec![PinDefinition::required_input(pin_type, description)]
    }

    /// 输出引脚按 [`MqttMode`] 分支：
    ///
    /// - `Publish` 模式 → `Any`：output 仅 echo 上游 payload，下游基本不消费
    /// - `Subscribe` 模式 → `Json`：[`Self::on_deploy`] 中的订阅循环把消息
    ///   规范化后通过 [`NodeHandle::emit`] 推进 DAG，output 形状是结构化 JSON
    fn output_pins(&self) -> Vec<PinDefinition> {
        let (pin_type, description) = match self.config.mode {
            MqttMode::Publish => (PinType::Any, "echo 上游 payload；下游基本不消费"),
            MqttMode::Subscribe => (
                PinType::Json,
                "订阅消息规范化后的 JSON 对象（含 topic / payload 字段）",
            ),
        };
        vec![PinDefinition::output(pin_type, description)]
    }

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

            return Ok(NodeExecution::broadcast(result).with_metadata(Some(metadata)));
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

        Ok(NodeExecution::broadcast(payload).with_metadata(Some(metadata)))
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
#[cfg(test)]
#[allow(clippy::unwrap_used)]
#[path = "tests.rs"]
mod tests;
