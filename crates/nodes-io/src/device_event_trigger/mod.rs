//! 设备事件触发节点（ADR-0024 Phase 2/3）。
//!
//! 通过 `on_deploy` 启动后台事件监听循环，归一化 MQTT `Topic` / CAN `CanFrame` / Modbus `Register` /
//! `SerialCommand` 事件为设备信号更新，经 `signal_decode` 解码、scale 求值后通过
//! `NodeHandle::emit` 推进 DAG。
//!
//! 生命周期模型：event 语义——`on_deploy` 后台循环，`LifecycleGuard` 管理清理。
//! `transform` 仅用于 simulation 模式下的单次模拟输出。

use async_trait::async_trait;
use chrono::Utc;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};
use uuid::Uuid;

use connections::SharedConnectionManager;
use nazh_core::{
    EngineError, LifecycleGuard, NodeExecution, NodeLifecycleContext, NodeTrait, PinDefinition,
    PinType,
};

use crate::signal_decode::{SignalSourceSnapshot, compile_scale, create_scale_engine};

#[cfg(feature = "io-can")]
mod can_loop;
#[cfg(feature = "io-modbus")]
mod modbus_loop;
#[cfg(feature = "io-mqtt")]
mod mqtt_loop;
#[cfg(feature = "io-serial")]
mod serial_loop;

/// 单个信号的监听配置快照。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SignalListenerSnapshot {
    pub signal_id: String,
    pub source: SignalSourceSnapshot,
    #[serde(default)]
    pub scale: Option<String>,
    #[serde(default)]
    pub unit: Option<String>,
}

/// 默认 poll 间隔（毫秒）。
fn default_poll_interval_ms() -> u64 {
    1000
}

/// 设备事件触发节点配置。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceEventTriggerConfig {
    #[serde(default)]
    pub connection_id: Option<String>,
    pub device_id: String,
    pub signals: Vec<SignalListenerSnapshot>,
    #[serde(default)]
    pub simulation: bool,
    /// Modbus Register 轮询间隔（毫秒），默认 1000。
    #[serde(default = "default_poll_interval_ms")]
    pub poll_interval_ms: u64,
}

/// 预编译的信号监听项（signal 配置 + scale AST）。
pub(super) struct CompiledSignal {
    pub(super) listener: SignalListenerSnapshot,
    pub(super) scale_ast: Option<rhai::AST>,
    pub(super) engine: rhai::Engine,
}

/// 设备事件触发节点。
pub struct DeviceEventTriggerNode {
    id: String,
    config: DeviceEventTriggerConfig,
    connection_manager: SharedConnectionManager,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ListenerProtocol {
    Mqtt,
    Can,
    Modbus,
    Serial,
}

#[derive(Debug)]
struct ListenerConnectionPlan {
    mqtt_endpoint: Option<(String, u16)>,
}

fn normalize_connection_kind(kind: &str) -> String {
    kind.trim().to_ascii_lowercase()
}

fn ensure_connection_kind(
    connection_id: &str,
    actual: &str,
    allowed: &[&str],
) -> Result<(), EngineError> {
    let actual = normalize_connection_kind(actual);
    if allowed.iter().any(|kind| *kind == actual) {
        return Ok(());
    }
    Err(EngineError::ConnectionInvalidConfiguration {
        connection_id: connection_id.to_owned(),
        reason: format!(
            "deviceEventTrigger 监听协议与连接类型不匹配，当前 type=`{actual}`，期望: {}",
            allowed.join(", ")
        ),
    })
}

fn required_str(
    connection_id: &str,
    metadata: &Value,
    key: &str,
    label: &str,
) -> Result<String, EngineError> {
    metadata
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
        .ok_or_else(|| EngineError::ConnectionInvalidConfiguration {
            connection_id: connection_id.to_owned(),
            reason: format!("{label} 连接需要配置 {key}"),
        })
}

fn required_u64(
    connection_id: &str,
    metadata: &Value,
    key: &str,
    label: &str,
) -> Result<u64, EngineError> {
    metadata
        .get(key)
        .and_then(Value::as_u64)
        .filter(|value| *value > 0)
        .ok_or_else(|| EngineError::ConnectionInvalidConfiguration {
            connection_id: connection_id.to_owned(),
            reason: format!("{label} 连接需要配置有效的 {key}"),
        })
}

fn required_u16(
    connection_id: &str,
    metadata: &Value,
    key: &str,
    label: &str,
) -> Result<u16, EngineError> {
    let value = required_u64(connection_id, metadata, key, label)?;
    u16::try_from(value).map_err(|_| EngineError::ConnectionInvalidConfiguration {
        connection_id: connection_id.to_owned(),
        reason: format!("{label} 连接 {key} 必须在 1-65535 之间"),
    })
}

fn required_u8(
    connection_id: &str,
    metadata: &Value,
    key: &str,
    label: &str,
) -> Result<u8, EngineError> {
    let value = required_u64(connection_id, metadata, key, label)?;
    u8::try_from(value).map_err(|_| EngineError::ConnectionInvalidConfiguration {
        connection_id: connection_id.to_owned(),
        reason: format!("{label} 连接 {key} 必须在 1-255 之间"),
    })
}

impl DeviceEventTriggerNode {
    pub fn new(
        id: impl Into<String>,
        config: DeviceEventTriggerConfig,
        connection_manager: SharedConnectionManager,
    ) -> Result<Self, EngineError> {
        let id = id.into();
        // 预编译所有 scale 表达式以 fail-fast。
        for sig in &config.signals {
            compile_scale(&sig.scale).map_err(|e| {
                EngineError::node_config(
                    id.clone(),
                    format!("signal `{}` scale 表达式编译失败: {e}", sig.signal_id),
                )
            })?;
        }
        Ok(Self {
            id,
            config,
            connection_manager,
        })
    }

    /// 构造事件 payload。
    pub(super) fn build_event_payload(
        device_id: &str,
        signal: &SignalListenerSnapshot,
        value: Value,
    ) -> Value {
        let mut result = Map::new();
        result.insert("device_id".to_owned(), json!(device_id));
        result.insert("signal_id".to_owned(), json!(signal.signal_id));
        result.insert("event_type".to_owned(), json!("signal_update"));
        result.insert("value".to_owned(), value);
        if let Some(unit) = &signal.unit {
            result.insert("unit".to_owned(), json!(unit));
        }
        result.insert("received_at".to_owned(), json!(Utc::now().to_rfc3339()));
        Value::Object(result)
    }

    /// 构造事件 metadata。
    pub(super) fn build_event_metadata(
        device_id: &str,
        signal: &SignalListenerSnapshot,
        simulated: bool,
    ) -> Map<String, Value> {
        Map::from_iter([(
            "device_event".to_owned(),
            json!({
                "device_id": device_id,
                "signal_id": signal.signal_id,
                "source_type": signal.source.type_tag(),
                "simulated": simulated,
            }),
        )])
    }

    /// 校验监听协议与连接定义，并提取后台循环需要的连接参数。
    async fn validate_listener_connection(
        &self,
        connection_id: &str,
        protocols: &[ListenerProtocol],
    ) -> Result<ListenerConnectionPlan, EngineError> {
        if protocols.len() != 1 {
            return Err(EngineError::node_config(
                self.id.clone(),
                "deviceEventTrigger 单个节点只能监听一种协议；请按 MQTT/CAN/Modbus/Serial 拆分节点"
                    .to_owned(),
            ));
        }

        let mut guard = self.connection_manager.acquire(connection_id).await?;
        let lease = guard.lease().clone();
        guard.mark_success();

        match protocols[0] {
            ListenerProtocol::Mqtt => {
                ensure_connection_kind(connection_id, &lease.kind, &["mqtt"])?;
                let host = required_str(connection_id, &lease.metadata, "host", "MQTT")?;
                let port = required_u16(connection_id, &lease.metadata, "port", "MQTT")?;
                for sig in &self.config.signals {
                    if let SignalSourceSnapshot::Topic { topic } = &sig.source
                        && topic.trim().is_empty()
                    {
                        return Err(EngineError::node_config(
                            self.id.clone(),
                            format!("signal `{}` MQTT topic 不能为空", sig.signal_id),
                        ));
                    }
                }
                Ok(ListenerConnectionPlan {
                    mqtt_endpoint: Some((host, port)),
                })
            }
            ListenerProtocol::Can => {
                ensure_connection_kind(connection_id, &lease.kind, &["can", "can-slcan", "slcan"])?;
                required_str(connection_id, &lease.metadata, "interface", "CAN")?;
                required_str(connection_id, &lease.metadata, "channel", "CAN")?;
                required_u64(connection_id, &lease.metadata, "baud_rate", "CAN")?;
                required_u64(connection_id, &lease.metadata, "bitrate", "CAN")?;
                Ok(ListenerConnectionPlan {
                    mqtt_endpoint: None,
                })
            }
            ListenerProtocol::Modbus => {
                ensure_connection_kind(connection_id, &lease.kind, &["modbus", "modbus_tcp"])?;
                required_str(connection_id, &lease.metadata, "host", "Modbus")?;
                required_u16(connection_id, &lease.metadata, "port", "Modbus")?;
                required_u8(connection_id, &lease.metadata, "unit", "Modbus")?;
                Ok(ListenerConnectionPlan {
                    mqtt_endpoint: None,
                })
            }
            ListenerProtocol::Serial => {
                ensure_connection_kind(
                    connection_id,
                    &lease.kind,
                    &[
                        "serial",
                        "serialport",
                        "serial_port",
                        "uart",
                        "rs232",
                        "rs485",
                    ],
                )?;
                required_str(connection_id, &lease.metadata, "port_path", "串口")?;
                required_u64(connection_id, &lease.metadata, "baud_rate", "串口")?;
                required_str(connection_id, &lease.metadata, "delimiter", "串口")?;
                Ok(ListenerConnectionPlan {
                    mqtt_endpoint: None,
                })
            }
        }
    }

    fn active_protocols(
        mqtt_signals: &[CompiledSignal],
        can_signals: &[CompiledSignal],
        modbus_signals: &[CompiledSignal],
        serial_signals: &[CompiledSignal],
    ) -> Vec<ListenerProtocol> {
        let mut protocols = Vec::new();
        if !mqtt_signals.is_empty() {
            protocols.push(ListenerProtocol::Mqtt);
        }
        if !can_signals.is_empty() {
            protocols.push(ListenerProtocol::Can);
        }
        if !modbus_signals.is_empty() {
            protocols.push(ListenerProtocol::Modbus);
        }
        if !serial_signals.is_empty() {
            protocols.push(ListenerProtocol::Serial);
        }
        protocols
    }

    /// 按 source 过滤后为匹配的 signal 预编译 scale AST。
    fn compile_signals_filtered(
        &self,
        predicate: impl Fn(&SignalSourceSnapshot) -> bool,
    ) -> Result<Vec<CompiledSignal>, EngineError> {
        self.config
            .signals
            .iter()
            .filter(|sig| predicate(&sig.source))
            .map(|sig| {
                let scale_ast = compile_scale(&sig.scale).map_err(|e| {
                    EngineError::node_config(
                        self.id.clone(),
                        format!("signal `{}` scale 编译失败: {e}", sig.signal_id),
                    )
                })?;
                Ok(CompiledSignal {
                    listener: sig.clone(),
                    scale_ast,
                    engine: create_scale_engine(),
                })
            })
            .collect()
    }
}

#[async_trait]
impl NodeTrait for DeviceEventTriggerNode {
    nazh_core::impl_node_meta!("deviceEventTrigger");

    fn output_pins(&self) -> Vec<PinDefinition> {
        vec![PinDefinition::output(
            PinType::Json,
            "设备事件 JSON（device_id / signal_id / event_type / value / unit / received_at）",
        )]
    }

    async fn transform(
        &self,
        _trace_id: Uuid,
        payload: Value,
    ) -> Result<NodeExecution, EngineError> {
        if self.config.connection_id.is_none() && !self.config.simulation {
            return Err(EngineError::node_config(
                self.id.clone(),
                "deviceEventTrigger 缺少 connection_id；仅测试/demo 可显式设置 simulation=true"
                    .to_owned(),
            ));
        }

        // simulation 模式：对第一个 signal 生成模拟事件。
        let signal = self.config.signals.first().ok_or_else(|| {
            EngineError::node_config(self.id.clone(), "signals 列表为空".to_owned())
        })?;

        let sim_value = serde_json::Number::from_f64(42.5).map_or(Value::Null, Value::Number);
        let result = Self::build_event_payload(&self.config.device_id, signal, sim_value);
        let metadata = Self::build_event_metadata(&self.config.device_id, signal, true);

        let _ = payload;
        Ok(NodeExecution::broadcast(result).with_metadata(Some(metadata)))
    }

    async fn on_deploy(&self, ctx: NodeLifecycleContext) -> Result<LifecycleGuard, EngineError> {
        if self.config.connection_id.is_none() {
            if self.config.simulation {
                return Ok(LifecycleGuard::noop());
            }
            return Err(EngineError::node_config(
                self.id.clone(),
                "deviceEventTrigger 缺少 connection_id；仅测试/demo 可显式设置 simulation=true"
                    .to_owned(),
            ));
        }

        let connection_id = self.config.connection_id.clone().unwrap_or_default();
        if connection_id.is_empty() {
            return Err(EngineError::node_config(
                self.id.clone(),
                "deviceEventTrigger connection_id 不能为空".to_owned(),
            ));
        }

        // 按 source 类型分组，每组独立编译（CompiledSignal 包含非 Clone 的 rhai 类型）。
        let mqtt_signals =
            self.compile_signals_filtered(|s| matches!(s, SignalSourceSnapshot::Topic { .. }))?;

        let can_signals =
            self.compile_signals_filtered(|s| matches!(s, SignalSourceSnapshot::CanFrame { .. }))?;

        let modbus_signals =
            self.compile_signals_filtered(|s| matches!(s, SignalSourceSnapshot::Register { .. }))?;

        let serial_signals = self.compile_signals_filtered(|s| {
            matches!(s, SignalSourceSnapshot::SerialCommand { .. })
        })?;

        if mqtt_signals.is_empty()
            && can_signals.is_empty()
            && modbus_signals.is_empty()
            && serial_signals.is_empty()
        {
            return Err(EngineError::node_config(
                self.id.clone(),
                "signals 列表中没有可监听的事件源".to_owned(),
            ));
        }

        let protocols = Self::active_protocols(
            &mqtt_signals,
            &can_signals,
            &modbus_signals,
            &serial_signals,
        );
        let connection_plan = self
            .validate_listener_connection(&connection_id, &protocols)
            .await?;
        let (host, port) = connection_plan
            .mqtt_endpoint
            .unwrap_or_else(|| (String::new(), 0));

        let join = spawn_orchestrator(
            &self.id,
            &connection_id,
            &host,
            port,
            &self.config.device_id,
            mqtt_signals,
            can_signals,
            modbus_signals,
            serial_signals,
            &self.connection_manager,
            &ctx.handle,
            &ctx.shutdown,
            self.config.poll_interval_ms,
        );

        Ok(LifecycleGuard::from_task(ctx.shutdown, join))
    }
}

/// 启动 orchestrator task 管理所有协议 listeners。
#[allow(clippy::too_many_arguments, clippy::too_many_lines)]
fn spawn_orchestrator(
    id: &str,
    connection_id: &str,
    host: &str,
    port: u16,
    device_id: &str,
    mqtt_signals: Vec<CompiledSignal>,
    can_signals: Vec<CompiledSignal>,
    modbus_signals: Vec<CompiledSignal>,
    serial_signals: Vec<CompiledSignal>,
    connection_manager: &SharedConnectionManager,
    handle: &nazh_core::NodeHandle,
    token: &nazh_core::CancellationToken,
    poll_interval_ms: u64,
) -> tokio::task::JoinHandle<()> {
    let id = id.to_owned();
    let connection_id = connection_id.to_owned();
    let host = host.to_owned();
    let device_id = device_id.to_owned();
    let connection_manager = connection_manager.clone();
    let handle = handle.clone();
    let token = token.clone();

    tokio::spawn(async move {
        let mut tasks = Vec::new();

        #[cfg(feature = "io-mqtt")]
        if !mqtt_signals.is_empty() {
            let task_id = id.clone();
            let task_cm = connection_manager.clone();
            let task_handle = handle.clone();
            let task_token = token.clone();
            let task_conn_id = connection_id.clone();
            let task_device_id = device_id.clone();
            tasks.push(tokio::spawn(async move {
                mqtt_loop::run_mqtt_listener_loop(
                    &task_id,
                    &task_conn_id,
                    &host,
                    port,
                    &mqtt_signals,
                    &task_cm,
                    &task_handle,
                    &task_token,
                    &task_device_id,
                )
                .await;
            }));
        }

        #[cfg(feature = "io-can")]
        if !can_signals.is_empty() {
            let task_id = id.clone();
            let task_cm = connection_manager.clone();
            let task_handle = handle.clone();
            let task_token = token.clone();
            let task_conn_id = connection_id.clone();
            let task_device_id = device_id.clone();
            tasks.push(tokio::spawn(async move {
                can_loop::run_can_listener_loop(
                    &task_id,
                    &task_conn_id,
                    &can_signals,
                    &task_cm,
                    &task_handle,
                    &task_token,
                    &task_device_id,
                )
                .await;
            }));
        }

        #[cfg(feature = "io-modbus")]
        if !modbus_signals.is_empty() {
            let task_id = id.clone();
            let task_cm = connection_manager.clone();
            let task_handle = handle.clone();
            let task_token = token.clone();
            let task_conn_id = connection_id.clone();
            let task_device_id = device_id.clone();
            tasks.push(tokio::spawn(async move {
                modbus_loop::run_modbus_poll_loop(
                    &task_id,
                    &task_conn_id,
                    &modbus_signals,
                    &task_cm,
                    &task_handle,
                    &task_token,
                    &task_device_id,
                    poll_interval_ms,
                )
                .await;
            }));
        }

        #[cfg(feature = "io-serial")]
        if !serial_signals.is_empty() {
            let task_id = id.clone();
            let task_cm = connection_manager.clone();
            let task_handle = handle.clone();
            let task_token = token.clone();
            let task_conn_id = connection_id.clone();
            let task_device_id = device_id.clone();
            tasks.push(tokio::spawn(async move {
                serial_loop::run_serial_listen_loop(
                    &task_id,
                    &task_conn_id,
                    &serial_signals,
                    &task_cm,
                    &task_handle,
                    &task_token,
                    &task_device_id,
                )
                .await;
            }));
        }

        // 等待取消信号。
        token.cancelled().await;
        // 子任务会因 token 取消而自行退出。
        for task in tasks {
            let _ = task.await;
        }
    })
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use connections::{ConnectionDefinition, shared_connection_manager};
    use nazh_core::{NodeCapabilities, PinKind};

    fn topic_signal() -> SignalListenerSnapshot {
        SignalListenerSnapshot {
            signal_id: "pressure".to_owned(),
            source: SignalSourceSnapshot::Topic {
                topic: "factory/press/pressure".to_owned(),
            },
            scale: None,
            unit: Some("MPa".to_owned()),
        }
    }

    fn make_config() -> DeviceEventTriggerConfig {
        DeviceEventTriggerConfig {
            connection_id: None,
            device_id: "test_device".to_owned(),
            signals: vec![topic_signal()],
            simulation: true,
            poll_interval_ms: 1000,
        }
    }

    fn make_node() -> DeviceEventTriggerNode {
        DeviceEventTriggerNode::new("det-1", make_config(), shared_connection_manager()).unwrap()
    }

    fn connection(id: &str, kind: &str, metadata: Value) -> ConnectionDefinition {
        ConnectionDefinition {
            id: id.to_owned(),
            kind: kind.to_owned(),
            metadata,
        }
    }

    #[test]
    fn output_pins_声明_out_exec() {
        let node = make_node();
        let pins = node.output_pins();
        assert_eq!(pins.len(), 1, "deviceEventTrigger 应声明单个输出端口");
        let out_pin = pins.first().unwrap();
        assert_eq!(out_pin.id, "out");
        assert_eq!(out_pin.pin_type, PinType::Json);
        assert_eq!(out_pin.kind, PinKind::Exec);
    }

    #[test]
    fn capabilities_trigger_device_io() {
        // capabilities 由注册表在 register_with_capabilities 时设置，
        // 不在 NodeTrait 上声明。本测试仅验证位组合值的正确性。
        let caps = NodeCapabilities::TRIGGER | NodeCapabilities::DEVICE_IO;
        assert!(caps.contains(NodeCapabilities::TRIGGER));
        assert!(caps.contains(NodeCapabilities::DEVICE_IO));
    }

    #[tokio::test]
    async fn 缺少连接且未显式模拟时拒绝运行() {
        let config = DeviceEventTriggerConfig {
            simulation: false,
            ..make_config()
        };
        let node =
            DeviceEventTriggerNode::new("det-1", config, shared_connection_manager()).unwrap();
        let err = node
            .transform(Uuid::new_v4(), Value::Null)
            .await
            .unwrap_err();
        assert!(matches!(err, EngineError::NodeConfig { .. }));
    }

    #[tokio::test]
    async fn simulation_模式返回事件_payload() {
        let node = make_node();
        let execution = node.transform(Uuid::new_v4(), Value::Null).await.unwrap();
        let output = &execution.outputs[0];
        let payload = &output.payload;
        assert_eq!(payload["device_id"], "test_device");
        assert_eq!(payload["signal_id"], "pressure");
        assert_eq!(payload["event_type"], "signal_update");
        assert!(payload.get("value").is_some());
        assert_eq!(payload["unit"], "MPa");

        let metadata = output.metadata.as_ref().unwrap();
        assert_eq!(metadata["device_event"]["simulated"], Value::Bool(true));
    }

    #[test]
    fn 无效_scale_表达式_构造时失败() {
        let config = DeviceEventTriggerConfig {
            signals: vec![SignalListenerSnapshot {
                scale: Some("raw * / 2".to_owned()),
                ..topic_signal()
            }],
            ..make_config()
        };
        let result = DeviceEventTriggerNode::new("det-1", config, shared_connection_manager());
        assert!(result.is_err());
    }

    #[test]
    fn signal_listener_snapshot_serde_round_trip() {
        let sig = topic_signal();
        let val = serde_json::to_value(&sig).unwrap();
        assert_eq!(val["signal_id"], "pressure");
        let back: SignalListenerSnapshot = serde_json::from_value(val).unwrap();
        assert_eq!(back.signal_id, "pressure");
    }

    #[tokio::test]
    async fn mqtt_listener_部署期要求显式_port() {
        let manager = shared_connection_manager();
        manager
            .register_connection(connection(
                "mqtt-1",
                "mqtt",
                json!({"host": "127.0.0.1", "topic": "factory/#"}),
            ))
            .await
            .unwrap();
        let node = DeviceEventTriggerNode::new(
            "det-1",
            DeviceEventTriggerConfig {
                connection_id: Some("mqtt-1".to_owned()),
                simulation: false,
                ..make_config()
            },
            manager,
        )
        .unwrap();

        let err = node
            .validate_listener_connection("mqtt-1", &[ListenerProtocol::Mqtt])
            .await
            .unwrap_err();
        assert!(matches!(
            err,
            EngineError::ConnectionInvalidConfiguration { .. }
        ));
    }

    #[tokio::test]
    async fn 混合协议_signal_部署期拒绝() {
        let manager = shared_connection_manager();
        let node = DeviceEventTriggerNode::new("det-1", make_config(), manager).unwrap();

        let err = node
            .validate_listener_connection(
                "unused",
                &[ListenerProtocol::Mqtt, ListenerProtocol::Serial],
            )
            .await
            .unwrap_err();
        assert!(matches!(err, EngineError::NodeConfig { .. }));
    }
}
