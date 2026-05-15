//! 设备信号读取节点（ADR-0024 Phase 1）。
//!
//! 按 `SignalSourceSnapshot` 从设备读取原始数据，经 `DataType` 解码、
//! `scale` 缩放后输出语义化值。Phase 1 仅实现 `Register`（Modbus TCP）源。
//!
//! 生命周期模型：poll 语义——exec 触发 + data 缓存（对标 `modbusRead`）。
//! 无 `on_deploy`/`on_undeploy`。

use async_trait::async_trait;
use chrono::Utc;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};
use uuid::Uuid;

use connections::{SharedConnectionManager, connection_metadata};
use nazh_core::{EngineError, NodeExecution, NodeTrait, PinDefinition, PinType, into_payload_map};

use crate::signal_decode::{
    ByteOrderSnapshot, DataTypeSnapshot, SignalSourceSnapshot, apply_scale_with_engine,
    compile_scale, create_scale_engine,
};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceSignalReadConfig {
    #[serde(default)]
    pub connection_id: Option<String>,
    pub device_id: String,
    pub signal_id: String,
    pub source: SignalSourceSnapshot,
    /// Rhai 缩放表达式（如 `"raw * 35 / 65535"`）。
    #[serde(default)]
    pub scale: Option<String>,
    #[serde(default)]
    pub unit: Option<String>,
    #[serde(default)]
    pub simulation: bool,
}

/// 设备信号读取节点。
pub struct DeviceSignalReadNode {
    id: String,
    config: DeviceSignalReadConfig,
    connection_manager: SharedConnectionManager,
    /// 预编译 Rhai AST（scale 为空时为 None）。
    scale_ast: Option<rhai::AST>,
    /// Rhai Engine 复用实例。
    engine: rhai::Engine,
}

impl DeviceSignalReadNode {
    /// 创建节点。编译 scale 表达式，无效时 fail-fast。
    pub fn new(
        id: impl Into<String>,
        config: DeviceSignalReadConfig,
        connection_manager: SharedConnectionManager,
    ) -> Result<Self, EngineError> {
        let id = id.into();
        let scale_ast = compile_scale(&config.scale).map_err(|e| {
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
            _ => Value::Number(serde_json::Number::from(42)),
        }
    }

    /// 读取 Modbus 寄存器原始字并转为字节切片。
    async fn read_register_raw(
        &self,
        trace_id: Uuid,
        host: &str,
        port: u16,
        unit_id: u8,
        register: u16,
        quantity: u16,
    ) -> Result<Vec<u8>, EngineError> {
        use tokio_modbus::client::Reader;

        let socket_addr = std::net::SocketAddr::from((
            host.parse::<std::net::IpAddr>().map_err(|error| {
                EngineError::stage_execution(
                    self.id.clone(),
                    trace_id,
                    format!("Modbus TCP 地址解析失败 ({host}): {error}"),
                )
            })?,
            port,
        ));

        let slave = tokio_modbus::Slave(unit_id);
        let mut ctx = tokio_modbus::client::tcp::connect_slave(socket_addr, slave)
            .await
            .map_err(|error| {
                EngineError::stage_execution(
                    self.id.clone(),
                    trace_id,
                    format!("Modbus TCP 连接失败 ({host}:{port}): {error}"),
                )
            })?;

        let words = ctx
            .read_holding_registers(register, quantity)
            .await
            .map_err(|error| {
                EngineError::stage_execution(
                    self.id.clone(),
                    trace_id,
                    format!("Modbus 读保持寄存器失败: {error}"),
                )
            })?
            .map_err(|error| {
                EngineError::stage_execution(
                    self.id.clone(),
                    trace_id,
                    format!("Modbus 协议错误: {error}"),
                )
            })?;

        // 将 u16 字数组转为大端字节序列（Modbus 标准字节序）。
        let bytes: Vec<u8> = words.iter().flat_map(|w| w.to_be_bytes()).collect();
        Ok(bytes)
    }

    /// 按信号源类型读取原始数据并解码。
    async fn read_and_decode(
        &self,
        trace_id: Uuid,
        guard: &mut Option<connections::ConnectionGuard>,
    ) -> Result<(Value, bool), EngineError> {
        if let Some(guard_ref) = guard {
            let metadata_value = &guard_ref.lease().metadata;

            match &self.config.source {
                SignalSourceSnapshot::Register {
                    register,
                    data_type,
                    bit,
                } => {
                    let host = metadata_value
                        .get("host")
                        .and_then(Value::as_str)
                        .unwrap_or_default();
                    let port = metadata_value
                        .get("port")
                        .and_then(Value::as_u64)
                        .and_then(|p| u16::try_from(p).ok())
                        .ok_or_else(|| {
                            EngineError::node_config(
                                self.id.clone(),
                                "Modbus 连接元数据缺少有效的 host 或 port".to_owned(),
                            )
                        })?;
                    let unit_id = metadata_value
                        .get("unit")
                        .and_then(Value::as_u64)
                        .and_then(|u| u8::try_from(u).ok())
                        .unwrap_or(1);

                    let quantity = data_type.modbus_register_count();
                    let raw_bytes = self
                        .read_register_raw(trace_id, host, port, unit_id, *register, quantity)
                        .await?;

                    let val = crate::signal_decode::decode_raw_bytes(
                        &raw_bytes,
                        *data_type,
                        ByteOrderSnapshot::BigEndian,
                        *bit,
                    )
                    .map_err(|e| {
                        EngineError::stage_execution(
                            self.id.clone(),
                            trace_id,
                            format!("信号解码失败: {e}"),
                        )
                    })?;

                    if let Some(g) = guard.as_mut() {
                        g.mark_success();
                    }

                    Ok((val, false))
                }
                _ => Err(EngineError::node_config(
                    self.id.clone(),
                    "Phase 1 仅支持 Register 源".to_owned(),
                )),
            }
        } else {
            Ok((self.simulate_value(), true))
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
            let mut modbus_meta = Map::from_iter([
                ("simulated".to_owned(), json!(false)),
                ("sampled_at".to_owned(), json!(Utc::now().to_rfc3339())),
            ]);
            let (key, value) = connection_metadata(&self.id, guard_ref.lease())?;
            modbus_meta.insert(key, value);
            metadata_map.insert("modbus".to_owned(), Value::Object(modbus_meta));
        } else if simulated {
            metadata_map.insert(
                "modbus".to_owned(),
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

        let value =
            apply_scale_with_engine(decoded_value, &self.scale_ast, &self.engine).map_err(|e| {
                EngineError::stage_execution(
                    self.id.clone(),
                    trace_id,
                    format!("scale 求值失败: {e}"),
                )
            })?;

        let result = self.build_payload(payload, value);
        let metadata = self.build_metadata(simulated, guard.as_ref())?;

        Ok(NodeExecution::broadcast(result).with_metadata(Some(metadata)))
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use connections::shared_connection_manager;
    use nazh_core::PinKind;

    fn register_source() -> SignalSourceSnapshot {
        SignalSourceSnapshot::Register {
            register: 40001,
            data_type: DataTypeSnapshot::Float32,
            bit: None,
        }
    }

    fn make_config() -> DeviceSignalReadConfig {
        DeviceSignalReadConfig {
            connection_id: None,
            device_id: "test_device".to_owned(),
            signal_id: "pressure".to_owned(),
            source: register_source(),
            scale: None,
            unit: Some("MPa".to_owned()),
            simulation: true,
        }
    }

    fn make_node() -> DeviceSignalReadNode {
        DeviceSignalReadNode::new("dsr-1", make_config(), shared_connection_manager()).unwrap()
    }

    #[test]
    fn output_pins_声明_out_exec_与_latest_data() {
        let node = make_node();
        let pins = node.output_pins();
        assert_eq!(pins.len(), 2, "deviceSignalRead 应声明两个输出端口");

        let out_pin = pins.iter().find(|p| p.id == "out").expect("缺 out 引脚");
        assert_eq!(out_pin.pin_type, PinType::Json);
        assert_eq!(out_pin.kind, PinKind::Exec);

        let latest_pin = pins
            .iter()
            .find(|p| p.id == "latest")
            .expect("缺 latest 引脚");
        assert_eq!(latest_pin.pin_type, PinType::Json);
        assert_eq!(latest_pin.kind, PinKind::Data);
        assert!(!latest_pin.required, "Data 拉取式引脚 required=false");
    }

    #[test]
    fn input_pin_保留默认_any() {
        let node = make_node();
        let pins = node.input_pins();
        assert_eq!(pins.len(), 1);
        assert_eq!(pins[0].pin_type, PinType::Any);
    }

    #[tokio::test]
    async fn 缺少连接且未显式模拟时拒绝运行() {
        let config = DeviceSignalReadConfig {
            simulation: false,
            connection_id: None,
            ..make_config()
        };
        let node = DeviceSignalReadNode::new("dsr-1", config, shared_connection_manager()).unwrap();

        let err = node
            .transform(Uuid::new_v4(), Value::Null)
            .await
            .unwrap_err();

        assert!(
            matches!(err, EngineError::NodeConfig { .. }),
            "未配置连接或 simulation=true 时不应静默模拟: {err:?}"
        );
    }

    #[tokio::test]
    async fn simulation_模式返回语义化输出() {
        let node = make_node();
        let execution = node.transform(Uuid::new_v4(), Value::Null).await.unwrap();

        let output = &execution.outputs[0];
        let payload = &output.payload;
        assert_eq!(payload["device_id"], "test_device");
        assert_eq!(payload["signal_id"], "pressure");
        assert!(payload.get("value").is_some());
        assert_eq!(payload["unit"], "MPa");
        assert!(payload.get("sampled_at").is_some());

        let metadata = output.metadata.as_ref().unwrap();
        assert_eq!(metadata["device_signal"]["simulated"], Value::Bool(true));
        assert_eq!(metadata["device_signal"]["source_type"], "register");
    }

    #[test]
    fn signal_source_snapshot_serde_round_trip() {
        let source = register_source();
        let json = serde_json::to_string(&source).unwrap();
        let back: SignalSourceSnapshot = serde_json::from_str(&json).unwrap();

        if let SignalSourceSnapshot::Register {
            register,
            data_type,
            bit,
        } = &back
        {
            assert_eq!(*register, 40001);
            assert_eq!(*data_type, DataTypeSnapshot::Float32);
            assert!(bit.is_none());
        } else {
            panic!("期望 Register 变体");
        }
    }

    #[test]
    fn signal_source_snapshot_json_format() {
        let source = register_source();
        let val = serde_json::to_value(&source).unwrap();
        assert_eq!(val["type"], "register");
        assert_eq!(val["register"], 40001);
        assert_eq!(val["data_type"], "float32");
    }

    #[test]
    fn 无效_scale_表达式_构造时失败() {
        let config = DeviceSignalReadConfig {
            scale: Some("raw * / 2".to_owned()),
            ..make_config()
        };
        let result = DeviceSignalReadNode::new("dsr-1", config, shared_connection_manager());
        assert!(result.is_err(), "无效 scale 表达式应在构造时失败");
    }

    #[test]
    fn 有效_scale_表达式_构造成功() {
        let config = DeviceSignalReadConfig {
            scale: Some("raw * 35 / 65535".to_owned()),
            ..make_config()
        };
        let result = DeviceSignalReadNode::new("dsr-1", config, shared_connection_manager());
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn simulation_scale_求值正确() {
        let config = DeviceSignalReadConfig {
            scale: Some("raw * 2".to_owned()),
            ..make_config()
        };
        let node = DeviceSignalReadNode::new("dsr-1", config, shared_connection_manager()).unwrap();
        let execution = node.transform(Uuid::new_v4(), Value::Null).await.unwrap();

        let payload = &execution.outputs[0].payload;
        // simulate_value 对 Float32 返回 42.5, scale 后应为 85.0
        let val = payload["value"].as_f64().unwrap();
        assert!((val - 85.0).abs() < 0.01, "期望 85.0，得到 {val}");
    }
}
