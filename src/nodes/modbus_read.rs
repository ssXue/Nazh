//! Modbus 寄存器读取节点（当前为模拟实现）。
//!
//! 根据配置的基准值和振幅，通过正弦函数模拟传感器读数，
//! 并将 `_modbus` 元数据写入 payload。若配置了 `connection_id`，
//! 则通过 [`with_connection`](super::helpers::with_connection) 借出连接。

use async_trait::async_trait;
use chrono::Utc;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use super::helpers::{insert_connection_lease, into_payload_map, with_connection};
use super::{NodeExecution, NodeTrait};
use crate::{ConnectionLease, EngineError, SharedConnectionManager, WorkflowContext};

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
    #[serde(default = "default_modbus_base_value")]
    pub base_value: f64,
    #[serde(default = "default_modbus_amplitude")]
    pub amplitude: f64,
}

/// Modbus 寄存器读取节点。
pub struct ModbusReadNode {
    id: String,
    ai_description: String,
    config: ModbusReadNodeConfig,
    connection_manager: SharedConnectionManager,
}

impl ModbusReadNode {
    pub fn new(
        id: impl Into<String>,
        config: ModbusReadNodeConfig,
        ai_description: impl Into<String>,
        connection_manager: SharedConnectionManager,
    ) -> Self {
        Self {
            id: id.into(),
            ai_description: ai_description.into(),
            config,
            connection_manager,
        }
    }

    fn simulate_and_build(
        &self,
        ctx: WorkflowContext,
        lease: Option<&ConnectionLease>,
    ) -> Result<WorkflowContext, EngineError> {
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

        let mut payload_map = into_payload_map(ctx.payload);

        if quantity == 1 {
            if let Some(value) = values.first() {
                payload_map.insert("value".to_owned(), value.clone());
            }
        } else {
            payload_map.insert("values".to_owned(), Value::Array(values));
        }

        payload_map.insert(
            "_modbus".to_owned(),
            json!({
                "simulated": true,
                "unit_id": self.config.unit_id,
                "register": self.config.register,
                "quantity": quantity,
                "sampled_at": Utc::now().to_rfc3339(),
            }),
        );

        if let Some(lease) = lease {
            insert_connection_lease(&self.id, &mut payload_map, lease)?;
        }

        Ok(WorkflowContext::from_parts(
            ctx.trace_id,
            Utc::now(),
            Value::Object(payload_map),
        ))
    }
}

#[async_trait]
impl NodeTrait for ModbusReadNode {
    impl_node_meta!("modbusRead");

    async fn execute(&self, ctx: WorkflowContext) -> Result<NodeExecution, EngineError> {
        let result = with_connection(
            &self.connection_manager,
            self.config.connection_id.as_deref(),
            |lease| self.simulate_and_build(ctx, lease),
        )
        .await?;
        Ok(NodeExecution::broadcast(result))
    }
}
