//! CAN / MQTT / EtherCAT / Serial 协议源读取实现（ADR-0024 Phase 2/3）。

use std::io::Write as IoWrite;

use uuid::Uuid;
use serde_json::Value;

use nazh_core::EngineError;

use super::reader::DeviceSignalReadNode;
use crate::signal_decode::{
    ByteOrderSnapshot, DataTypeSnapshot, SignalSourceSnapshot, decode_topic_payload,
    extract_pdo_bytes,
};

// -- CAN --

#[cfg(feature = "io-can")]
impl DeviceSignalReadNode {
    pub(crate) async fn read_can_frame(
        &self,
        trace_id: Uuid,
        guard: &mut Option<connections::ConnectionGuard>,
    ) -> Result<(Value, bool), EngineError> {
        use crate::can::session::CanBusRuntime;

        let (can_id, is_extended, byte_offset, byte_length, data_type, byte_order) =
            match &self.config.source {
                SignalSourceSnapshot::CanFrame {
                    can_id,
                    is_extended,
                    byte_offset,
                    byte_length,
                    data_type,
                    byte_order,
                    ..
                } => (
                    *can_id,
                    *is_extended,
                    *byte_offset,
                    *byte_length,
                    *data_type,
                    *byte_order,
                ),
                _ => unreachable!(),
            };

        let conn_id = self.config.connection_id.as_deref().unwrap_or_default();
        let runtime = CanBusRuntime::new(self.connection_manager.clone(), conn_id.to_owned());
        let session = runtime
            .ensure_session(&self.id, |_| Ok(()))
            .await
            .map_err(|e| EngineError::stage_execution(self.id.clone(), trace_id, e.to_string()))?;

        let timeout = std::time::Duration::from_millis(self.config.poll_timeout_ms);
        let bus = session
            .bus_handle(&self.id)
            .await
            .map_err(|e| EngineError::stage_execution(self.id.clone(), trace_id, e.to_string()))?;

        let frame = loop {
            match bus.recv(timeout).await {
                Ok(Some(f)) if f.id == can_id && f.is_extended == is_extended => break f,
                Ok(Some(_)) => {}
                Ok(None) => {
                    return Err(EngineError::stage_execution(
                        self.id.clone(),
                        trace_id,
                        "CAN 接收超时".to_owned(),
                    ));
                }
                Err(error) => {
                    runtime.shutdown().await;
                    return Err(EngineError::stage_execution(
                        self.id.clone(),
                        trace_id,
                        format!("CAN 接收错误: {error}"),
                    ));
                }
            }
        };

        let start = usize::from(byte_offset);
        let end = start + usize::from(byte_length);
        if end > frame.data.len() {
            return Err(EngineError::stage_execution(
                self.id.clone(),
                trace_id,
                format!(
                    "CAN 帧 payload 越界: offset={start}, length={}, frame_len={}",
                    usize::from(byte_length),
                    frame.data.len()
                ),
            ));
        }

        let raw = &frame.data[start..end];
        let val = crate::signal_decode::decode_raw_bytes(raw, data_type, byte_order, None)
            .map_err(|e| {
                EngineError::stage_execution(
                    self.id.clone(),
                    trace_id,
                    format!("CAN 帧解码失败: {e}"),
                )
            })?;

        if let Some(g) = guard.as_mut() {
            g.mark_success();
        }
        Ok((val, false))
    }
}

// -- MQTT --

#[cfg(feature = "io-mqtt")]
impl DeviceSignalReadNode {
    pub(crate) async fn read_topic(
        &self,
        trace_id: Uuid,
        metadata: &Value,
        guard: &mut Option<connections::ConnectionGuard>,
    ) -> Result<(Value, bool), EngineError> {
        let topic = match &self.config.source {
            SignalSourceSnapshot::Topic { topic } => topic.clone(),
            _ => unreachable!(),
        };

        let host = metadata
            .get("host")
            .and_then(Value::as_str)
            .unwrap_or_default();
        let port = metadata
            .get("port")
            .and_then(Value::as_u64)
            .and_then(|p| u16::try_from(p).ok())
            .unwrap_or(1883);

        if host.is_empty() {
            return Err(EngineError::node_config(
                self.id.clone(),
                "MQTT 连接元数据缺少 host".to_owned(),
            ));
        }

        let client_id = format!("nazh-dsr-{}", self.id.chars().take(12).collect::<String>());
        let mut mqttoptions = rumqttc::MqttOptions::new(client_id, host, port);
        mqttoptions.set_keep_alive(std::time::Duration::from_secs(5));

        let (_client, mut eventloop) = rumqttc::AsyncClient::new(mqttoptions, 10);

        // 等 ConnAck（5s 超时）。
        let connected = loop {
            let event = tokio::time::timeout(std::time::Duration::from_secs(5), eventloop.poll())
                .await
                .map_err(|_| {
                    EngineError::stage_execution(
                        self.id.clone(),
                        trace_id,
                        "MQTT 连接超时（5 秒）".to_owned(),
                    )
                })?;
            match event {
                Ok(rumqttc::Event::Incoming(rumqttc::Packet::ConnAck(ack))) => {
                    break ack.code == rumqttc::ConnectReturnCode::Success;
                }
                Ok(rumqttc::Event::Incoming(rumqttc::Packet::Disconnect)) | Err(_) => {
                    break false;
                }
                Ok(_) => {}
            }
        };

        if !connected {
            return Err(EngineError::stage_execution(
                self.id.clone(),
                trace_id,
                format!("MQTT {host}:{port} 连接失败"),
            ));
        }

        // 等待一条 Publish 消息（poll_timeout_ms 超时）。
        let timeout = std::time::Duration::from_millis(self.config.poll_timeout_ms);
        let message_result: Result<Vec<u8>, EngineError> = async {
            let result = tokio::time::timeout(timeout, async {
                loop {
                    match eventloop.poll().await {
                        Ok(rumqttc::Event::Incoming(rumqttc::Packet::Publish(msg))) => {
                            if msg.topic == topic {
                                return Ok::<Vec<u8>, String>(msg.payload.to_vec());
                            }
                        }
                        Ok(_) => {}
                        Err(error) => return Err(format!("MQTT 事件循环错误: {error}")),
                    }
                }
            })
            .await
            .map_err(|_| {
                EngineError::stage_execution(
                    self.id.clone(),
                    trace_id,
                    format!("MQTT Topic `{topic}` 等待消息超时"),
                )
            })?;

            result.map_err(|e: String| EngineError::stage_execution(self.id.clone(), trace_id, e))
        }
        .await;
        let message = message_result?;

        let val = decode_topic_payload(&message, &self.config.signal_id);

        if let Some(g) = guard.as_mut() {
            g.mark_success();
        }
        Ok((val, false))
    }
}

// -- EtherCAT --

#[cfg(feature = "io-ethercat")]
impl DeviceSignalReadNode {
    pub(crate) async fn read_ethercat_pdo(
        &self,
        trace_id: Uuid,
        guard: &mut Option<connections::ConnectionGuard>,
    ) -> Result<(Value, bool), EngineError> {
        use crate::ethercat::session::EthercatRuntime;

        let (slave_address, byte_offset, byte_len) = match &self.config.source {
            SignalSourceSnapshot::EthercatPdo {
                slave_address,
                pdo_index,
                entry_index,
                sub_index: _,
                bit_len,
            } => {
                let slave = slave_address.unwrap_or(1);
                let offset = usize::from(*pdo_index) * 2 + usize::from(*entry_index);
                let len = usize::from(*bit_len).div_ceil(8);
                (slave, offset, len)
            }
            _ => unreachable!(),
        };

        let conn_id = self.config.connection_id.as_deref().unwrap_or_default();
        let runtime = EthercatRuntime::new(self.connection_manager.clone(), conn_id.to_owned());
        let session = runtime
            .ensure_session(&self.id)
            .await
            .map_err(|e| EngineError::stage_execution(self.id.clone(), trace_id, e.to_string()))?;

        let inputs: Vec<u8> = {
            let guard_inner = session.bus(&self.id).await.map_err(|e| {
                EngineError::stage_execution(self.id.clone(), trace_id, e.to_string())
            })?;
            let bus = guard_inner.as_ref().ok_or_else(|| {
                EngineError::stage_execution(
                    self.id.clone(),
                    trace_id,
                    "EtherCAT 总线会话已释放".to_owned(),
                )
            })?;
            bus.read_inputs(slave_address).await.map_err(|error| {
                EngineError::stage_execution(
                    self.id.clone(),
                    trace_id,
                    format!("EtherCAT PDO 读取失败: {error}"),
                )
            })?
        };

        let raw = extract_pdo_bytes(&inputs, byte_offset, byte_len).map_err(|e| {
            EngineError::stage_execution(
                self.id.clone(),
                trace_id,
                format!("EtherCAT PDO 字节提取失败: {e}"),
            )
        })?;

        // EtherCAT PDO 数据默认为小端（EtherCAT 标准）。
        let val = crate::signal_decode::decode_raw_bytes(
            raw,
            DataTypeSnapshot::U16,
            ByteOrderSnapshot::LittleEndian,
            None,
        )
        .map_err(|e| {
            EngineError::stage_execution(
                self.id.clone(),
                trace_id,
                format!("EtherCAT PDO 解码失败: {e}"),
            )
        })?;

        if let Some(g) = guard.as_mut() {
            g.mark_success();
        }
        Ok((val, false))
    }
}

// -- Serial --

#[cfg(feature = "io-serial")]
impl DeviceSignalReadNode {
    pub(crate) async fn read_serial_command(
        &self,
        trace_id: Uuid,
        metadata: &Value,
        guard: &mut Option<connections::ConnectionGuard>,
    ) -> Result<(Value, bool), EngineError> {
        let command = match &self.config.source {
            SignalSourceSnapshot::SerialCommand { command } => command.clone(),
            _ => unreachable!(),
        };

        let port_path = metadata
            .get("port_path")
            .or_else(|| metadata.get("port"))
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned();
        let baud_rate = metadata
            .get("baud_rate")
            .and_then(Value::as_u64)
            .and_then(|b| u32::try_from(b).ok())
            .unwrap_or(9600);

        if port_path.is_empty() {
            return Err(EngineError::node_config(
                self.id.clone(),
                "串口连接元数据缺少 port_path".to_owned(),
            ));
        }

        let timeout_ms = self.config.poll_timeout_ms;
        let delimiter = metadata
            .get("delimiter")
            .and_then(Value::as_str)
            .unwrap_or("\n")
            .to_owned();
        let node_id = self.id.clone();

        let raw_bytes = tokio::task::spawn_blocking(move || {
            serial_read_command(
                &node_id, &port_path, baud_rate, &command, &delimiter, timeout_ms,
            )
        })
        .await
        .map_err(|e| {
            EngineError::stage_execution(self.id.clone(), trace_id, format!("串口任务失败: {e}"))
        })??;

        let val = crate::signal_decode::decode_raw_bytes(
            &raw_bytes,
            DataTypeSnapshot::Float32,
            ByteOrderSnapshot::BigEndian,
            None,
        )
        .map_err(|e| {
            EngineError::stage_execution(
                self.id.clone(),
                trace_id,
                format!("串口数据解码失败: {e}"),
            )
        })?;

        if let Some(g) = guard.as_mut() {
            g.mark_success();
        }
        Ok((val, false))
    }
}

/// 串口发送命令并读取分隔帧响应。
#[cfg(feature = "io-serial")]
fn serial_read_command(
    node_id: &str,
    port_path: &str,
    baud_rate: u32,
    command: &str,
    delimiter: &str,
    timeout_ms: u64,
) -> Result<Vec<u8>, EngineError> {
    let timeout = std::time::Duration::from_millis(timeout_ms);
    let mut port = serialport::new(port_path, baud_rate.max(1))
        .timeout(timeout)
        .open()
        .map_err(|error| {
            EngineError::stage_execution(
                node_id.to_owned(),
                Uuid::nil(),
                format!("串口打开失败 ({port_path}): {error}"),
            )
        })?;

    // 发送命令。
    port.write_all(command.as_bytes()).map_err(|error| {
        EngineError::stage_execution(
            node_id.to_owned(),
            Uuid::nil(),
            format!("串口写入失败: {error}"),
        )
    })?;

    // 读取直到分隔符或超时。
    let delim_bytes: Vec<u8> = if delimiter == "\\n" {
        vec![b'\n']
    } else if delimiter == "\\r\\n" {
        vec![b'\r', b'\n']
    } else {
        delimiter.as_bytes().to_vec()
    };

    let mut buffer = Vec::with_capacity(256);
    let start = std::time::Instant::now();
    let mut single = [0u8; 1];

    loop {
        if start.elapsed() > timeout {
            // 超时但已收到部分数据——返回已有缓冲。
            break;
        }
        match port.read(&mut single) {
            Ok(0) => {}
            Ok(n) => {
                buffer.extend_from_slice(&single[..n]);
                if !delim_bytes.is_empty() && buffer.len() >= delim_bytes.len() {
                    let tail = &buffer[buffer.len() - delim_bytes.len()..];
                    if tail == delim_bytes.as_slice() {
                        buffer.truncate(buffer.len() - delim_bytes.len());
                        break;
                    }
                }
            }
            Err(ref e) if e.kind() == std::io::ErrorKind::TimedOut => break,
            Err(error) => {
                return Err(EngineError::stage_execution(
                    node_id.to_owned(),
                    Uuid::nil(),
                    format!("串口读取失败: {error}"),
                ));
            }
        }
    }

    if buffer.is_empty() {
        return Err(EngineError::stage_execution(
            node_id.to_owned(),
            Uuid::nil(),
            "串口未收到响应数据".to_owned(),
        ));
    }

    Ok(buffer)
}
