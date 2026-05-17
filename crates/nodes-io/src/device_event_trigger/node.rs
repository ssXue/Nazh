//! 设备事件触发节点结构体、NodeTrait 实现与事件 payload/metadata 构造。

use async_trait::async_trait;
use chrono::Utc;
use serde_json::{Map, Value, json};
use uuid::Uuid;

use connections::SharedConnectionManager;
use nazh_core::{
    EngineError, LifecycleGuard, NodeExecution, NodeLifecycleContext, NodeTrait, PinDefinition,
    PinType,
};

use crate::signal_decode::{SignalSourceSnapshot, compile_scale, create_scale_engine};

use super::config::{
    CompiledSignal, DeviceEventTriggerConfig, ListenerConnectionPlan, ListenerProtocol,
    SignalListenerSnapshot,
};
use super::orchestrator::spawn_orchestrator;

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
            compile_scale(sig.scale.as_ref()).map_err(|e| {
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
    pub(crate) async fn validate_listener_connection(
        &self,
        connection_id: &str,
        protocols: &[ListenerProtocol],
    ) -> Result<ListenerConnectionPlan, EngineError> {
        use super::config::{
            ensure_connection_kind, required_str, required_u8, required_u16, required_u64,
        };

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
                let scale_ast = compile_scale(sig.scale.as_ref()).map_err(|e| {
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
