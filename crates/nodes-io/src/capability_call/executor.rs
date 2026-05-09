//! capabilityCall 的协议执行 helper。
//!
//! 本模块只放底层协议动作，保持 `capability_call.rs` 里的节点配置、
//! 模板解析与测试更容易审阅。

use std::collections::HashMap;
#[cfg(feature = "io-serial")]
use std::{io::Write, time::Duration};

#[cfg(feature = "io-can")]
use crate::can::{CanFrame, hex, session::CanBusRuntime, validate_can_id};
use connections::connection_metadata;
use nazh_core::{EngineError, NodeExecution};
#[cfg(feature = "io-can")]
use serde_json::json;
use serde_json::{Map, Value};
use uuid::Uuid;

use super::CapabilityCallNode;
use super::protocol::{
    connection_kind_matches, metadata_u8_or, metadata_u16_or, metadata_u32_or, parse_hex_bytes,
    parse_u16_value, required_metadata_str, required_metadata_u8, required_metadata_u16,
};
#[cfg(feature = "io-mqtt")]
use super::protocol::{mqtt_qos, qos_value};

impl CapabilityCallNode {
    pub(super) async fn execute_modbus_write(
        &self,
        trace_id: Uuid,
        register: u16,
        resolved_value: String,
        resolved_args: HashMap<String, Value>,
    ) -> Result<NodeExecution, EngineError> {
        #[cfg(feature = "io-modbus")]
        {
            use tokio_modbus::client::Writer;

            let mut guard = self
                .acquire_for_protocol("Modbus", &["modbus", "modbus_tcp", "modbus-tcp"])
                .await?;
            let host = required_metadata_str(guard.metadata(), "host", &self.id, "Modbus")?;
            let port = required_metadata_u16(guard.metadata(), "port", &self.id, "Modbus")?;
            let unit_id = required_metadata_u8(guard.metadata(), "unit_id", &self.id, "Modbus")?;
            let word = parse_u16_value(&resolved_value, &self.id, "Modbus 写入值")?;

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
                    let reason = format!("Modbus TCP 连接失败 ({host}:{port}): {error}");
                    guard.mark_failure(&reason);
                    EngineError::stage_execution(self.id.clone(), trace_id, reason)
                })?;

            ctx.write_single_register(register, word)
                .await
                .map_err(|error| {
                    let reason = format!("Modbus 写保持寄存器失败: {error}");
                    guard.mark_failure(&reason);
                    EngineError::stage_execution(self.id.clone(), trace_id, reason)
                })?
                .map_err(|error| {
                    let reason = format!("Modbus 协议错误: {error}");
                    guard.mark_failure(&reason);
                    EngineError::stage_execution(self.id.clone(), trace_id, reason)
                })?;

            let mut modbus_meta = Map::from_iter([
                ("register".to_owned(), serde_json::json!(register)),
                ("value".to_owned(), serde_json::json!(word)),
                ("unit_id".to_owned(), serde_json::json!(unit_id)),
                (
                    "written_at".to_owned(),
                    serde_json::json!(chrono::Utc::now().to_rfc3339()),
                ),
            ]);
            let (key, value) = connection_metadata(&self.id, guard.lease())?;
            modbus_meta.insert(key, value);
            guard.mark_success();

            let payload = serde_json::json!({
                "capability_id": self.config.capability_id,
                "device_id": self.config.device_id,
                "operation": "modbus-write",
                "register": register,
                "value": word,
                "args": resolved_args,
            });
            Ok(self.output(payload, Some(("modbus", Value::Object(modbus_meta)))))
        }

        #[cfg(not(feature = "io-modbus"))]
        {
            let _ = (trace_id, register, resolved_value, resolved_args);
            Err(EngineError::node_config(
                self.id.clone(),
                "当前构建未启用 io-modbus，不能执行 Modbus capabilityCall",
            ))
        }
    }

    pub(super) async fn execute_mqtt_publish(
        &self,
        trace_id: Uuid,
        topic: String,
        resolved_payload: String,
        resolved_args: HashMap<String, Value>,
    ) -> Result<NodeExecution, EngineError> {
        #[cfg(feature = "io-mqtt")]
        {
            let mut guard = self.acquire_for_protocol("MQTT", &["mqtt"]).await?;
            let host = required_metadata_str(guard.metadata(), "host", &self.id, "MQTT")?;
            let port = metadata_u16_or(guard.metadata(), "port", 1883, &self.id, "MQTT")?;
            let qos = mqtt_qos(metadata_u8_or(
                guard.metadata(),
                "qos",
                0,
                &self.id,
                "MQTT",
            )?);
            let resolved_topic = if topic.trim().is_empty() {
                required_metadata_str(guard.metadata(), "topic", &self.id, "MQTT")?
            } else {
                topic
            };

            let client_id = format!("nazh-cap-{}", self.id.chars().take(20).collect::<String>());
            let mut options = rumqttc::MqttOptions::new(client_id, host, port);
            options.set_keep_alive(std::time::Duration::from_secs(5));
            let (client, mut eventloop) = rumqttc::AsyncClient::new(options, 10);

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
                        Err(error) => return Err(format!("MQTT 连接错误: {error}")),
                        _ => {}
                    }
                }
            })
            .await
            .map_err(|_| {
                let reason = "MQTT 连接超时（10 秒）".to_owned();
                guard.mark_failure(&reason);
                EngineError::stage_execution(self.id.clone(), trace_id, reason)
            })?
            .map_err(|reason| {
                guard.mark_failure(&reason);
                EngineError::stage_execution(self.id.clone(), trace_id, reason)
            })?;

            client
                .publish(
                    resolved_topic.clone(),
                    qos,
                    false,
                    resolved_payload.clone().into_bytes(),
                )
                .await
                .map_err(|error| {
                    let reason = format!("MQTT 发布失败: {error}");
                    guard.mark_failure(&reason);
                    EngineError::stage_execution(self.id.clone(), trace_id, reason)
                })?;

            let _ = tokio::time::timeout(std::time::Duration::from_secs(5), eventloop.poll()).await;
            let mut mqtt_meta = Map::from_iter([
                ("topic".to_owned(), serde_json::json!(resolved_topic)),
                ("qos".to_owned(), serde_json::json!(qos_value(qos))),
                (
                    "published_at".to_owned(),
                    serde_json::json!(chrono::Utc::now().to_rfc3339()),
                ),
            ]);
            let (key, value) = connection_metadata(&self.id, guard.lease())?;
            mqtt_meta.insert(key, value);
            guard.mark_success();

            let payload = serde_json::json!({
                "capability_id": self.config.capability_id,
                "device_id": self.config.device_id,
                "operation": "mqtt-publish",
                "topic": resolved_topic,
                "payload": resolved_payload,
                "args": resolved_args,
            });
            Ok(self.output(payload, Some(("mqtt", Value::Object(mqtt_meta)))))
        }

        #[cfg(not(feature = "io-mqtt"))]
        {
            let _ = (trace_id, topic, resolved_payload, resolved_args);
            Err(EngineError::node_config(
                self.id.clone(),
                "当前构建未启用 io-mqtt，不能执行 MQTT capabilityCall",
            ))
        }
    }

    pub(super) async fn execute_serial_command(
        &self,
        trace_id: Uuid,
        command: String,
        resolved_args: HashMap<String, Value>,
    ) -> Result<NodeExecution, EngineError> {
        #[cfg(feature = "io-serial")]
        {
            let mut guard = self
                .acquire_for_protocol(
                    "串口",
                    &[
                        "serial",
                        "serialport",
                        "serial_port",
                        "uart",
                        "rs232",
                        "rs485",
                    ],
                )
                .await?;
            let port_path = required_metadata_str(guard.metadata(), "port_path", &self.id, "串口")?;
            let baud_rate =
                metadata_u32_or(guard.metadata(), "baud_rate", 9_600, &self.id, "串口")?;
            let command_bytes = command.clone().into_bytes();
            let port_path_for_thread = port_path.clone();
            let command_for_thread = command_bytes.clone();

            tokio::task::spawn_blocking(move || {
                let mut port = serialport::new(port_path_for_thread, baud_rate)
                    .timeout(Duration::from_secs(1))
                    .open()
                    .map_err(|error| format!("串口打开失败: {error}"))?;
                port.write_all(&command_for_thread)
                    .map_err(|error| format!("串口写入失败: {error}"))?;
                port.flush()
                    .map_err(|error| format!("串口刷新失败: {error}"))
            })
            .await
            .map_err(|error| {
                let reason = format!("串口写入任务 join 失败: {error}");
                guard.mark_failure(&reason);
                EngineError::stage_execution(self.id.clone(), trace_id, reason)
            })?
            .map_err(|reason| {
                guard.mark_failure(&reason);
                EngineError::stage_execution(self.id.clone(), trace_id, reason)
            })?;

            let mut serial_meta = Map::from_iter([
                ("port_path".to_owned(), serde_json::json!(port_path)),
                ("baud_rate".to_owned(), serde_json::json!(baud_rate)),
                (
                    "byte_len".to_owned(),
                    serde_json::json!(command_bytes.len()),
                ),
                (
                    "sent_at".to_owned(),
                    serde_json::json!(chrono::Utc::now().to_rfc3339()),
                ),
            ]);
            let (key, value) = connection_metadata(&self.id, guard.lease())?;
            serial_meta.insert(key, value);
            guard.mark_success();

            let payload = serde_json::json!({
                "capability_id": self.config.capability_id,
                "device_id": self.config.device_id,
                "operation": "serial-command",
                "command": command,
                "args": resolved_args,
            });
            Ok(self.output(payload, Some(("serial", Value::Object(serial_meta)))))
        }

        #[cfg(not(feature = "io-serial"))]
        {
            let _ = (trace_id, command, resolved_args);
            Err(EngineError::node_config(
                self.id.clone(),
                "当前构建未启用 io-serial，不能执行串口 capabilityCall",
            ))
        }
    }

    pub(super) async fn execute_can_write(
        &self,
        trace_id: Uuid,
        can_id: u32,
        data: String,
        is_extended: bool,
        resolved_args: HashMap<String, Value>,
    ) -> Result<NodeExecution, EngineError> {
        #[cfg(feature = "io-can")]
        {
            let connection_id = self.connection_id()?.to_owned();
            let record = self
                .connection_manager
                .get(&connection_id)
                .await
                .ok_or_else(|| {
                    EngineError::node_config(
                        self.id.clone(),
                        format!("CAN 连接 `{connection_id}` 不存在"),
                    )
                })?;
            if !connection_kind_matches(&record.kind, &["can", "can-slcan", "slcan"]) {
                return Err(EngineError::node_config(
                    self.id.clone(),
                    format!(
                        "capabilityCall `{}` 需要 CAN 连接，实际连接 `{connection_id}` 类型为 `{}`",
                        self.config.capability_id, record.kind
                    ),
                ));
            }
            validate_can_id(can_id, is_extended)
                .map_err(|error| EngineError::node_config(self.id.clone(), error.to_string()))?;
            let data_bytes = parse_hex_bytes(&data)
                .map_err(|error| EngineError::node_config(self.id.clone(), error))?;
            let frame = if is_extended {
                CanFrame::new_extended(can_id, &data_bytes)
            } else {
                CanFrame::new_standard(can_id, &data_bytes)
            };

            let runtime =
                CanBusRuntime::new(self.connection_manager.clone(), connection_id.clone());
            let session = runtime
                .ensure_session(&self.id, |_| Ok(()))
                .await
                .map_err(|error| {
                    EngineError::stage_execution(self.id.clone(), trace_id, error.to_string())
                })?;
            let bus_guard = session.bus(&self.id)?;
            let send_result = match bus_guard.as_ref() {
                Some(bus) => bus.send(&frame).await,
                None => {
                    return Err(EngineError::stage_execution(
                        self.id.clone(),
                        trace_id,
                        "CAN 总线会话已被清理".to_owned(),
                    ));
                }
            };
            drop(bus_guard);

            if let Err(error) = send_result {
                let reason = error.to_string();
                runtime.shutdown().await;
                let _ = self
                    .connection_manager
                    .record_connect_failure(&connection_id, &reason)
                    .await;
                return Err(EngineError::stage_execution(
                    self.id.clone(),
                    trace_id,
                    reason,
                ));
            }

            let mut can_meta = Map::from_iter([
                ("simulated".to_owned(), json!(session.simulated())),
                ("channel_info".to_owned(), json!(session.channel_info())),
                ("sent_at".to_owned(), json!(chrono::Utc::now().to_rfc3339())),
            ]);
            if let Some(lease) = session.lease() {
                let (key, value) = connection_metadata(&self.id, lease)?;
                can_meta.insert(key, value);
            }

            let payload = serde_json::json!({
                "capability_id": self.config.capability_id,
                "device_id": self.config.device_id,
                "operation": "can-write",
                "sent": {
                    "id": frame.id,
                    "id_hex": format!("0x{:03X}", frame.id),
                    "data": frame.data,
                    "data_hex": hex::encode(&frame.data),
                    "dlc": frame.dlc,
                    "is_extended": frame.is_extended,
                },
                "args": resolved_args,
            });
            Ok(self.output(payload, Some(("can", Value::Object(can_meta)))))
        }

        #[cfg(not(feature = "io-can"))]
        {
            let _ = (trace_id, can_id, data, is_extended, resolved_args);
            Err(EngineError::node_config(
                self.id.clone(),
                "当前构建未启用 io-can，不能执行 CAN capabilityCall",
            ))
        }
    }
}
