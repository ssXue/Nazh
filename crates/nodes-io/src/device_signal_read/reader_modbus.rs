//! Modbus Register 源读取实现（ADR-0024 Phase 1）。

use uuid::Uuid;
use serde_json::Value;

use nazh_core::EngineError;

use super::reader::{DeviceSignalReadNode, RegisterParams};
use crate::signal_decode::ByteOrderSnapshot;

impl DeviceSignalReadNode {
    /// Modbus Register 源读取。
    pub(crate) async fn read_register(
        &self,
        trace_id: Uuid,
        metadata: &Value,
        guard: &mut Option<connections::ConnectionGuard>,
    ) -> Result<(Value, bool), EngineError> {
        let (host, port, unit_id, register, data_type, bit) =
            self.extract_register_params(metadata)?;
        let quantity = data_type.modbus_register_count();
        let raw_bytes = self
            .read_register_raw(trace_id, &host, port, unit_id, register, quantity)
            .await?;
        let val = crate::signal_decode::decode_raw_bytes(
            &raw_bytes,
            data_type,
            ByteOrderSnapshot::BigEndian,
            bit,
        )
        .map_err(|e| {
            EngineError::stage_execution(self.id.clone(), trace_id, format!("信号解码失败: {e}"))
        })?;
        if let Some(g) = guard.as_mut() {
            g.mark_success();
        }
        Ok((val, false))
    }

    pub(crate) fn extract_register_params(
        &self,
        metadata: &Value,
    ) -> Result<RegisterParams, EngineError> {
        let (register, data_type, bit) = match &self.config.source {
            crate::signal_decode::SignalSourceSnapshot::Register {
                register,
                data_type,
                bit,
            } => (*register, *data_type, *bit),
            _ => unreachable!(),
        };
        let host = metadata
            .get("host")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned();
        let port = metadata
            .get("port")
            .and_then(Value::as_u64)
            .and_then(|p| u16::try_from(p).ok())
            .ok_or_else(|| {
                EngineError::node_config(
                    self.id.clone(),
                    "Modbus 连接元数据缺少有效的 host 或 port".to_owned(),
                )
            })?;
        let unit_id = metadata
            .get("unit")
            .and_then(Value::as_u64)
            .and_then(|u| u8::try_from(u).ok())
            .unwrap_or(1);
        Ok((host, port, unit_id, register, data_type, bit))
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

        let bytes: Vec<u8> = words.iter().flat_map(|w| w.to_be_bytes()).collect();
        Ok(bytes)
    }
}
