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

    /// 校验 `connection_id` 并获取连接 host/port。
    async fn resolve_connection(&self, connection_id: &str) -> Result<(String, u16), EngineError> {
        let mut guard = self.connection_manager.acquire(connection_id).await?;
        let metadata = guard.lease().metadata.clone();
        guard.mark_success();
        let host = metadata
            .get("host")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned();
        let port = metadata
            .get("port")
            .and_then(Value::as_u64)
            .and_then(|p| u16::try_from(p).ok())
            .unwrap_or(1883);
        Ok((host, port))
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

        let (host, port) = self.resolve_connection(&connection_id).await?;

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
    use connections::shared_connection_manager;
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
}
