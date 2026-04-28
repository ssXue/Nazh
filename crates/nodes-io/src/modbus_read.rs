//! Modbus 寄存器读取节点。
//!
//! 当配置了 `connection_id` 时，通过真实 Modbus TCP 协议读取寄存器值；
//! 否则使用正弦函数模拟传感器读数作为回退。

use async_trait::async_trait;
use chrono::Utc;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};
use uuid::Uuid;

use connections::{SharedConnectionManager, connection_metadata};
use nazh_core::{EngineError, NodeExecution, NodeTrait, PinDefinition, PinType, into_payload_map};
use tokio_modbus::client::Reader;

fn default_modbus_unit_id() -> u16 {
    1
}
fn default_modbus_register() -> u16 {
    40_001
}
fn default_modbus_quantity() -> u16 {
    1
}
fn default_modbus_base_value() -> f64 {
    64.0
}
fn default_modbus_amplitude() -> f64 {
    6.0
}
fn default_modbus_register_type() -> String {
    "holding".to_owned()
}

/// Modbus 寄存器类型。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ModbusRegisterType {
    Holding,
    Input,
    Coil,
    Discrete,
}

impl ModbusRegisterType {
    fn as_str(self) -> &'static str {
        match self {
            Self::Holding => "holding",
            Self::Input => "input",
            Self::Coil => "coil",
            Self::Discrete => "discrete",
        }
    }
}

fn parse_register_type(value: &str) -> ModbusRegisterType {
    match value.trim().to_ascii_lowercase().as_str() {
        "input" | "input_register" | "inputregister" => ModbusRegisterType::Input,
        "coil" | "coils" => ModbusRegisterType::Coil,
        "discrete" | "discrete_input" | "discreteinput" => ModbusRegisterType::Discrete,
        _ => ModbusRegisterType::Holding,
    }
}

fn number_to_value(value: f64) -> Value {
    if let Some(number) = serde_json::Number::from_f64(value) {
        Value::Number(number)
    } else {
        Value::Null
    }
}

fn round_measurement(value: f64) -> Value {
    number_to_value((value * 100.0).round() / 100.0)
}

/// 将 Modbus 字数组转换为 JSON 值列表。
fn word_values_to_json(words: &[u16]) -> Vec<Value> {
    words
        .iter()
        .map(|&w| Value::Number(serde_json::Number::from(w)))
        .collect()
}

/// 将 Modbus 线圈布尔数组转换为 JSON 值列表。
fn coil_values_to_json(coils: &[bool]) -> Vec<Value> {
    coils.iter().map(|&c| Value::Bool(c)).collect()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModbusReadNodeConfig {
    #[serde(default)]
    pub connection_id: Option<String>,
    #[serde(default = "default_modbus_unit_id")]
    pub unit_id: u16,
    #[serde(default = "default_modbus_register")]
    pub register: u16,
    #[serde(default = "default_modbus_quantity")]
    pub quantity: u16,
    /// 寄存器类型（holding / input / coil / discrete）。
    #[serde(default = "default_modbus_register_type")]
    pub register_type: String,
    /// 模拟模式基准值（仅当 `connection_id` 为空时使用）。
    #[serde(default = "default_modbus_base_value")]
    pub base_value: f64,
    /// 模拟模式波动幅度（仅当 `connection_id` 为空时使用）。
    #[serde(default = "default_modbus_amplitude")]
    pub amplitude: f64,
}

/// Modbus 寄存器读取节点。
pub struct ModbusReadNode {
    id: String,
    config: ModbusReadNodeConfig,
    connection_manager: SharedConnectionManager,
}

impl ModbusReadNode {
    pub fn new(
        id: impl Into<String>,
        config: ModbusReadNodeConfig,
        connection_manager: SharedConnectionManager,
    ) -> Self {
        Self {
            id: id.into(),
            config,
            connection_manager,
        }
    }

    fn simulate_and_build(&self, payload: Value) -> Value {
        #[allow(clippy::cast_precision_loss)]
        let now_seconds = Utc::now().timestamp_millis() as f64 / 1000.0;
        let quantity = self.config.quantity.clamp(1, 32);
        let values = (0..quantity)
            .map(|offset| {
                let phase = now_seconds / 4.8
                    + (f64::from(self.config.register) / 113.0)
                    + (f64::from(offset) * 0.41);
                round_measurement(self.config.base_value + self.config.amplitude * phase.sin())
            })
            .collect::<Vec<_>>();

        let mut payload_map = into_payload_map(payload);

        if quantity == 1 {
            if let Some(value) = values.first() {
                payload_map.insert("value".to_owned(), value.clone());
            }
        } else {
            payload_map.insert("values".to_owned(), Value::Array(values));
        }

        Value::Object(payload_map)
    }

    /// 通过真实 Modbus TCP 协议读取寄存器。
    #[allow(clippy::too_many_lines)]
    async fn read_modbus_tcp(
        &self,
        trace_id: Uuid,
        host: &str,
        port: u16,
    ) -> Result<Vec<Value>, EngineError> {
        let register_type = parse_register_type(&self.config.register_type);
        let quantity = self.config.quantity.clamp(1, 125);
        let slave = tokio_modbus::Slave(u8::try_from(self.config.unit_id).unwrap_or(1));

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

        let mut ctx = tokio_modbus::client::tcp::connect_slave(socket_addr, slave)
            .await
            .map_err(|error| {
                EngineError::stage_execution(
                    self.id.clone(),
                    trace_id,
                    format!("Modbus TCP 连接失败 ({host}:{port}): {error}"),
                )
            })?;

        let values = match register_type {
            ModbusRegisterType::Holding => {
                let words = ctx
                    .read_holding_registers(self.config.register, quantity)
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
                word_values_to_json(&words)
            }
            ModbusRegisterType::Input => {
                let words = ctx
                    .read_input_registers(self.config.register, quantity)
                    .await
                    .map_err(|error| {
                        EngineError::stage_execution(
                            self.id.clone(),
                            trace_id,
                            format!("Modbus 读输入寄存器失败: {error}"),
                        )
                    })?
                    .map_err(|error| {
                        EngineError::stage_execution(
                            self.id.clone(),
                            trace_id,
                            format!("Modbus 协议错误: {error}"),
                        )
                    })?;
                word_values_to_json(&words)
            }
            ModbusRegisterType::Coil => {
                let coils = ctx
                    .read_coils(self.config.register, quantity)
                    .await
                    .map_err(|error| {
                        EngineError::stage_execution(
                            self.id.clone(),
                            trace_id,
                            format!("Modbus 读线圈失败: {error}"),
                        )
                    })?
                    .map_err(|error| {
                        EngineError::stage_execution(
                            self.id.clone(),
                            trace_id,
                            format!("Modbus 协议错误: {error}"),
                        )
                    })?;
                coil_values_to_json(&coils)
            }
            ModbusRegisterType::Discrete => {
                let coils = ctx
                    .read_discrete_inputs(self.config.register, quantity)
                    .await
                    .map_err(|error| {
                        EngineError::stage_execution(
                            self.id.clone(),
                            trace_id,
                            format!("Modbus 读离散输入失败: {error}"),
                        )
                    })?
                    .map_err(|error| {
                        EngineError::stage_execution(
                            self.id.clone(),
                            trace_id,
                            format!("Modbus 协议错误: {error}"),
                        )
                    })?;
                coil_values_to_json(&coils)
            }
        };

        Ok(values)
    }
}

#[async_trait]
impl NodeTrait for ModbusReadNode {
    nazh_core::impl_node_meta!("modbusRead");

    /// 输出引脚：
    /// - `out`（Json/Exec）：每次执行向下游推送的寄存器读取结果。
    /// - `latest`（Json/Data）：拉取式槽位，缓存最近一次读数；下游 PURE 节点或
    ///   独立时钟触发的 transform 可在不重新执行 modbusRead 的情况下读到最新值
    ///   （ADR-0014 Phase 2 引入）。
    ///
    /// 注：[`Self::input_pins`] 保留 trait 默认（单 `Any` 输入）——modbusRead
    /// 常作为根节点或被 `timer`（输出 `Any`）触发，input 形状不重要。
    fn output_pins(&self) -> Vec<PinDefinition> {
        vec![
            PinDefinition::output(
                PinType::Json,
                "寄存器读取结果合并入 input payload 的 JSON 对象",
            ),
            PinDefinition::output_named_data(
                "latest",
                "最近读数",
                PinType::Json,
                "拉取式槽位：缓存最近一次寄存器读数，下游可在不触发 modbusRead 重读的前提下取最新值",
            ),
        ]
    }

    async fn transform(
        &self,
        trace_id: Uuid,
        payload: Value,
    ) -> Result<NodeExecution, EngineError> {
        let mut guard = if let Some(conn_id) = &self.config.connection_id {
            Some(self.connection_manager.acquire(conn_id).await?)
        } else {
            None
        };

        // 有连接资源时走真实 Modbus TCP
        if let Some(ref guard_ref) = guard {
            let metadata_value = &guard_ref.lease().metadata;

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

            let values = self.read_modbus_tcp(trace_id, host, port).await?;
            let register_type = parse_register_type(&self.config.register_type);

            let mut payload_map = into_payload_map(payload);
            if self.config.quantity <= 1 {
                if let Some(value) = values.first() {
                    payload_map.insert("value".to_owned(), value.clone());
                }
            } else {
                payload_map.insert("values".to_owned(), Value::Array(values));
            }

            let mut modbus_meta = Map::from_iter([
                ("simulated".to_owned(), json!(false)),
                ("unit_id".to_owned(), json!(self.config.unit_id)),
                ("register".to_owned(), json!(self.config.register)),
                ("register_type".to_owned(), json!(register_type.as_str())),
                (
                    "quantity".to_owned(),
                    json!(self.config.quantity.clamp(1, 125)),
                ),
                ("sampled_at".to_owned(), json!(Utc::now().to_rfc3339())),
            ]);

            let (key, value) = connection_metadata(&self.id, guard_ref.lease())?;
            modbus_meta.insert(key, value);

            if let Some(g) = guard.as_mut() {
                g.mark_success();
            }

            return Ok(
                NodeExecution::broadcast(Value::Object(payload_map)).with_metadata(Map::from_iter(
                    [("modbus".to_owned(), Value::Object(modbus_meta))],
                )),
            );
        }

        // 无连接资源时走模拟回退
        let result = self.simulate_and_build(payload);
        let metadata = Map::from_iter([(
            "modbus".to_owned(),
            json!({
                "simulated": true,
                "unit_id": self.config.unit_id,
                "register": self.config.register,
                "quantity": self.config.quantity.clamp(1, 32),
                "sampled_at": Utc::now().to_rfc3339(),
            }),
        )]);

        Ok(NodeExecution::broadcast(result).with_metadata(metadata))
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use connections::shared_connection_manager;
    use nazh_core::PinKind;
    use serde_json::json;

    fn make_node() -> ModbusReadNode {
        // 走 #[serde(default)] 路径构造默认 config，避免依赖 Default impl。
        let config: ModbusReadNodeConfig = serde_json::from_value(json!({})).unwrap();
        ModbusReadNode::new("modbus-1", config, shared_connection_manager())
    }

    #[test]
    fn output_pins_声明_out_exec_与_latest_data() {
        let node = make_node();
        let pins = node.output_pins();
        assert_eq!(
            pins.len(),
            2,
            "modbusRead 声明两个输出端口：out (Exec) + latest (Data)"
        );

        let out_pin = pins.iter().find(|p| p.id == "out").expect("缺 out 引脚");
        assert_eq!(out_pin.pin_type, PinType::Json);
        assert_eq!(out_pin.kind, PinKind::Exec);
        assert!(!out_pin.required);

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
        // modbusRead 常作为根节点或被 timer（Any 输出）触发，input 形状
        // 不重要，所以保持 trait 默认（单 Any 输入）。
        let node = make_node();
        let pins = node.input_pins();
        assert_eq!(pins.len(), 1);
        assert_eq!(pins[0].pin_type, PinType::Any);
    }
}
