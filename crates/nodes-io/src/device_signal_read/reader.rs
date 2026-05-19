//! 设备信号读取节点核心逻辑（ADR-0024 Phase 1/3）。
//!
//! 协议特化实现分别位于 [`reader_modbus`] 和 [`reader_protocols`] 模块。

use async_trait::async_trait;
use chrono::Utc;
use serde_json::{Map, Value, json};
use uuid::Uuid;

use connections::{SharedConnectionManager, connection_metadata};
use nazh_core::{EngineError, NodeExecution, NodeTrait, PinDefinition, PinType, into_payload_map};

use crate::signal_decode::{
    DataTypeSnapshot, SignalSourceSnapshot, apply_scale_with_engine, compile_scale,
    create_scale_engine,
};

use super::config::DeviceSignalReadConfig;

/// 设备信号读取节点。
pub struct DeviceSignalReadNode {
    pub(crate) id: String,
    pub(crate) config: DeviceSignalReadConfig,
    pub(crate) connection_manager: SharedConnectionManager,
    scale_ast: Option<rhai::AST>,
    engine: rhai::Engine,
}

/// Modbus Register 参数提取结果。
pub(crate) type RegisterParams = (String, u16, u8, u16, DataTypeSnapshot, Option<u8>);

impl DeviceSignalReadNode {
    /// 创建节点。编译 scale 表达式，无效时 fail-fast。
    pub fn new(
        id: impl Into<String>,
        config: DeviceSignalReadConfig,
        connection_manager: SharedConnectionManager,
    ) -> Result<Self, EngineError> {
        let id = id.into();
        let scale_ast = compile_scale(config.scale.as_ref()).map_err(|e| {
            EngineError::node_config(id.clone(), format!("scale 表达式编译失败: {e}"))
        })?;

        let engine = create_scale_engine();

        Ok(Self {
            id,
            config,
            connection_manager,
            scale_ast,
            engine,
        })
    }

    /// 模拟模式下生成测试值。
    fn simulate_value(&self) -> Value {
        match &self.config.source {
            SignalSourceSnapshot::Register { data_type, .. } => match data_type {
                DataTypeSnapshot::Bool => Value::Bool(true),
                DataTypeSnapshot::U16 | DataTypeSnapshot::U32 => {
                    Value::Number(serde_json::Number::from(100))
                }
                DataTypeSnapshot::I16 | DataTypeSnapshot::I32 => {
                    Value::Number(serde_json::Number::from(-10))
                }
                DataTypeSnapshot::Float32 | DataTypeSnapshot::Float64 => {
                    serde_json::Number::from_f64(42.5).map_or(Value::Null, Value::Number)
                }
                DataTypeSnapshot::String => Value::String("simulated".to_owned()),
            },
            SignalSourceSnapshot::CanFrame { .. }
            | SignalSourceSnapshot::Topic { .. }
            | SignalSourceSnapshot::SerialCommand { .. }
            | SignalSourceSnapshot::EthercatPdo { .. } => {
                serde_json::Number::from_f64(42.5).map_or(Value::Null, Value::Number)
            }
        }
    }

    /// 按信号源类型分发读取并解码。
    async fn read_and_decode(
        &self,
        trace_id: Uuid,
        guard: &mut Option<connections::ConnectionGuard>,
    ) -> Result<(Value, bool), EngineError> {
        if guard.is_none() {
            return Ok((self.simulate_value(), true));
        }
        // 提前 clone metadata，避免 guard 的不可变借用与可变借用冲突。
        let metadata = guard
            .as_ref()
            .map(|g| g.lease().metadata.clone())
            .unwrap_or_default();
        match &self.config.source {
            SignalSourceSnapshot::Register { .. } => {
                self.read_register(trace_id, &metadata, guard).await
            }
            #[cfg(feature = "io-can")]
            SignalSourceSnapshot::CanFrame { .. } => self.read_can_frame(trace_id, guard).await,
            #[cfg(feature = "io-mqtt")]
            SignalSourceSnapshot::Topic { .. } => self.read_topic(trace_id, &metadata, guard).await,
            #[cfg(feature = "io-ethercat")]
            SignalSourceSnapshot::EthercatPdo { .. } => {
                self.read_ethercat_pdo(trace_id, guard).await
            }
            #[cfg(feature = "io-serial")]
            SignalSourceSnapshot::SerialCommand { .. } => {
                self.read_serial_command(trace_id, &metadata, guard).await
            }
            #[allow(unreachable_patterns)]
            _ => Err(EngineError::node_config(
                self.id.clone(),
                format!("当前构建不支持 {} 源", self.config.source.type_tag()),
            )),
        }
    }

    /// 构造输出 payload。
    fn build_payload(&self, base: Value, value: Value) -> Value {
        let mut result = into_payload_map(base);
        result.insert("device_id".to_owned(), json!(self.config.device_id));
        result.insert("signal_id".to_owned(), json!(self.config.signal_id));
        result.insert("value".to_owned(), value);
        if let Some(unit) = &self.config.unit {
            result.insert("unit".to_owned(), json!(unit));
        }
        result.insert("sampled_at".to_owned(), json!(Utc::now().to_rfc3339()));
        Value::Object(result)
    }

    /// 构造 metadata。
    fn build_metadata(
        &self,
        simulated: bool,
        guard: Option<&connections::ConnectionGuard>,
    ) -> Result<Map<String, Value>, EngineError> {
        let device_signal_meta = Map::from_iter([
            ("device_id".to_owned(), json!(self.config.device_id)),
            ("signal_id".to_owned(), json!(self.config.signal_id)),
            (
                "source_type".to_owned(),
                json!(self.config.source.type_tag()),
            ),
            ("simulated".to_owned(), json!(simulated)),
        ]);

        let mut metadata_map = Map::new();
        metadata_map.insert(
            "device_signal".to_owned(),
            Value::Object(device_signal_meta),
        );

        if let Some(guard_ref) = guard {
            let source_key = self.config.source.type_tag();
            let mut source_meta = Map::from_iter([
                ("simulated".to_owned(), json!(false)),
                ("sampled_at".to_owned(), json!(Utc::now().to_rfc3339())),
            ]);
            let (key, value) = connection_metadata(&self.id, guard_ref.lease())?;
            source_meta.insert(key, value);
            metadata_map.insert(source_key.to_owned(), Value::Object(source_meta));
        } else if simulated {
            metadata_map.insert(
                self.config.source.type_tag().to_owned(),
                json!({
                    "simulated": true,
                    "sampled_at": Utc::now().to_rfc3339(),
                }),
            );
        }

        Ok(metadata_map)
    }
}

#[async_trait]
impl NodeTrait for DeviceSignalReadNode {
    nazh_core::impl_node_meta!("deviceSignalRead");

    /// 输出引脚（对标 `modbusRead`）：
    /// - `out`（Json/Exec）：每次 exec 触发时的读取结果。
    /// - `latest`（Json/Data）：拉取式槽位，缓存最近读数。
    fn output_pins(&self) -> Vec<PinDefinition> {
        vec![
            PinDefinition::output(PinType::Json, "设备信号读取结果 JSON"),
            PinDefinition::output_named_data(
                "latest",
                "最近读数",
                PinType::Json,
                "拉取式槽位：缓存最近一次信号读数",
            ),
        ]
    }

    async fn transform(
        &self,
        trace_id: Uuid,
        payload: Value,
    ) -> Result<NodeExecution, EngineError> {
        if self.config.connection_id.is_none() && !self.config.simulation {
            return Err(EngineError::node_config(
                self.id.clone(),
                "deviceSignalRead 缺少 connection_id；仅测试/demo 可显式设置 simulation=true"
                    .to_owned(),
            ));
        }

        let mut guard = if let Some(conn_id) = &self.config.connection_id {
            Some(self.connection_manager.acquire(conn_id).await?)
        } else {
            None
        };

        let (decoded_value, simulated) = self.read_and_decode(trace_id, &mut guard).await?;

        let value = apply_scale_with_engine(decoded_value, self.scale_ast.as_ref(), &self.engine)
            .map_err(|e| {
            EngineError::stage_execution(self.id.clone(), trace_id, format!("scale 求值失败: {e}"))
        })?;

        let result = self.build_payload(payload, value);
        let metadata = self.build_metadata(simulated, guard.as_ref())?;

        Ok(NodeExecution::broadcast(result).with_metadata(Some(metadata)))
    }
}
