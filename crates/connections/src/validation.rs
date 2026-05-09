//! 连接定义校验。
//!
//! 该模块集中维护连接类型 allowlist、类型别名 normalize 与协议字段校验。

use serde_json::Value;
use url::Url;

const SUPPORTED_CONNECTION_TYPES: &[&str] = &[
    "serial", "modbus", "mqtt", "http", "bark", "can", "ethercat",
];

#[allow(clippy::too_many_lines)]
pub(crate) fn validate_connection_definition(kind: &str, metadata: &Value) -> Result<(), String> {
    let normalized_kind = normalize_connection_kind(kind);
    let metadata = metadata_object(metadata);

    match normalized_kind.as_str() {
        "serial" | "serialport" | "serial_port" | "uart" | "rs232" | "rs485" => {
            let port_path = metadata
                .and_then(|value| value.get("port_path"))
                .and_then(Value::as_str)
                .map(str::trim)
                .unwrap_or_default();
            if port_path.is_empty() {
                return Err("串口连接需要配置 port_path".to_owned());
            }

            let baud_rate = metadata
                .and_then(|value| value.get("baud_rate"))
                .and_then(Value::as_u64)
                .unwrap_or(0);
            if baud_rate == 0 {
                return Err("串口连接需要配置有效的 baud_rate".to_owned());
            }
        }
        "modbus" | "modbus_tcp" => {
            let host = metadata
                .and_then(|value| value.get("host"))
                .and_then(Value::as_str)
                .map(str::trim)
                .unwrap_or_default();
            if host.is_empty() {
                return Err("Modbus 连接需要配置 host".to_owned());
            }

            let port = metadata
                .and_then(|value| value.get("port"))
                .and_then(Value::as_u64)
                .unwrap_or(0);
            if port == 0 || port > u64::from(u16::MAX) {
                return Err("Modbus 连接需要配置 1-65535 之间的 port".to_owned());
            }
        }
        "mqtt" => {
            let host = metadata
                .and_then(|value| value.get("host"))
                .and_then(Value::as_str)
                .map(str::trim)
                .unwrap_or_default();
            if host.is_empty() {
                return Err("MQTT 连接需要配置 host".to_owned());
            }

            let topic = metadata
                .and_then(|value| value.get("topic"))
                .and_then(Value::as_str)
                .map(str::trim)
                .unwrap_or_default();
            if topic.is_empty() {
                return Err("MQTT 连接需要配置 topic".to_owned());
            }
        }
        "http" | "http_sink" => {
            let url = metadata
                .and_then(|value| value.get("url"))
                .and_then(Value::as_str)
                .map(str::trim)
                .unwrap_or_default();
            if url.is_empty() {
                return Err("HTTP 连接需要配置 URL".to_owned());
            }

            Url::parse(url).map_err(|error| format!("HTTP URL 无效: {error}"))?;
        }
        "bark" | "bark_push" => {
            let device_key = metadata
                .and_then(|value| value.get("device_key"))
                .and_then(Value::as_str)
                .map(str::trim)
                .unwrap_or_default();
            if device_key.is_empty() {
                return Err("Bark 连接需要配置 device_key 或完整推送 URL".to_owned());
            }

            let server_url = metadata
                .and_then(|value| value.get("server_url"))
                .and_then(Value::as_str)
                .map(str::trim)
                .unwrap_or_default();
            if !server_url.is_empty() {
                Url::parse(server_url).map_err(|error| format!("Bark server_url 无效: {error}"))?;
            }
        }
        "can" | "can-slcan" | "slcan" => {
            let interface = metadata
                .and_then(|value| value.get("interface"))
                .and_then(Value::as_str)
                .map(str::trim)
                .unwrap_or_default();
            if interface.is_empty() {
                return Err("CAN-SLCAN 连接需要配置 interface（slcan/mock/virtual）".to_owned());
            }
            if !matches!(interface, "slcan" | "mock" | "virtual") {
                return Err(format!("CAN 连接 interface 不支持: {interface}"));
            }

            let channel = metadata
                .and_then(|value| value.get("channel"))
                .and_then(Value::as_str)
                .map(str::trim)
                .unwrap_or_default();
            if channel.is_empty() {
                return Err("CAN-SLCAN 连接需要配置 channel（串口设备路径）".to_owned());
            }

            let baud_rate = metadata
                .and_then(|value| value.get("baud_rate"))
                .and_then(Value::as_u64)
                .unwrap_or(0);
            if baud_rate == 0 {
                return Err("CAN-SLCAN 连接需要配置有效的 baud_rate".to_owned());
            }

            let bitrate = metadata
                .and_then(|value| value.get("bitrate"))
                .and_then(Value::as_u64)
                .unwrap_or(0);
            if bitrate == 0 {
                return Err("CAN-SLCAN 连接需要配置 CAN 总线 bitrate".to_owned());
            }
            if !matches!(
                bitrate,
                10_000
                    | 20_000
                    | 50_000
                    | 100_000
                    | 125_000
                    | 250_000
                    | 500_000
                    | 800_000
                    | 1_000_000
            ) {
                return Err(format!("CAN-SLCAN 连接不支持 bitrate: {bitrate}"));
            }
        }
        "ethercat" | "ethercat-soem" | "ecat" => {
            let backend = metadata
                .and_then(|value| value.get("backend"))
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(str::to_ascii_lowercase)
                .unwrap_or_default();
            if backend.is_empty() {
                return Err("EtherCAT 连接需要配置 backend（ethercrab/mock）".to_owned());
            }
            if !matches!(backend.as_str(), "ethercrab" | "mock") {
                return Err(format!("EtherCAT 连接不支持 backend: {backend}"));
            }

            let interface = metadata
                .and_then(|value| value.get("interface"))
                .and_then(Value::as_str)
                .map(str::trim)
                .unwrap_or_default();
            if interface.is_empty() {
                return Err("EtherCAT 连接需要配置 interface（网络接口名）".to_owned());
            }

            let cycle_time_ms = metadata
                .and_then(|value| value.get("cycle_time_ms"))
                .and_then(Value::as_u64)
                .unwrap_or(0);
            if cycle_time_ms == 0 {
                return Err("EtherCAT 连接 cycle_time_ms 必须大于 0".to_owned());
            }

            let op_timeout_ms = metadata
                .and_then(|value| value.get("op_timeout_ms"))
                .and_then(Value::as_u64)
                .unwrap_or(0);
            if op_timeout_ms == 0 {
                return Err("EtherCAT 连接 op_timeout_ms 必须大于 0".to_owned());
            }
        }
        _ => {
            return Err(format!(
                "不支持的连接类型 `{kind}`；支持类型: {}",
                SUPPORTED_CONNECTION_TYPES.join(", ")
            ));
        }
    }

    Ok(())
}

fn metadata_object(value: &Value) -> Option<&serde_json::Map<String, Value>> {
    value.as_object()
}

fn normalize_connection_kind(value: &str) -> String {
    value.trim().to_ascii_lowercase()
}
