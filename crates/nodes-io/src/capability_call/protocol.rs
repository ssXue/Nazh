//! capabilityCall 协议参数解析 helper。

#[cfg(feature = "io-can")]
use crate::can::hex;
use nazh_core::EngineError;
#[cfg(feature = "io-mqtt")]
use rumqttc::QoS;
use serde_json::Value;

pub(super) fn connection_kind_matches(kind: &str, allowed_kinds: &[&str]) -> bool {
    let normalized = kind.trim().to_ascii_lowercase().replace('_', "-");
    allowed_kinds
        .iter()
        .map(|kind| kind.replace('_', "-"))
        .any(|allowed| allowed == normalized)
}

pub(super) fn required_metadata_str(
    metadata: &Value,
    key: &str,
    node_id: &str,
    protocol: &str,
) -> Result<String, EngineError> {
    metadata
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
        .ok_or_else(|| {
            EngineError::node_config(
                node_id.to_owned(),
                format!("{protocol} 连接元数据缺少 `{key}`"),
            )
        })
}

pub(super) fn required_metadata_u16(
    metadata: &Value,
    key: &str,
    node_id: &str,
    protocol: &str,
) -> Result<u16, EngineError> {
    let Some(raw) = metadata.get(key).and_then(Value::as_u64) else {
        return Err(EngineError::node_config(
            node_id.to_owned(),
            format!("{protocol} 连接元数据缺少 `{key}`"),
        ));
    };
    u16::try_from(raw).map_err(|_| {
        EngineError::node_config(
            node_id.to_owned(),
            format!("{protocol} 连接元数据 `{key}` 超过 u16 上限"),
        )
    })
}

pub(super) fn required_metadata_u8(
    metadata: &Value,
    key: &str,
    node_id: &str,
    protocol: &str,
) -> Result<u8, EngineError> {
    let Some(raw) = metadata.get(key).and_then(Value::as_u64) else {
        return Err(EngineError::node_config(
            node_id.to_owned(),
            format!("{protocol} 连接元数据缺少 `{key}`"),
        ));
    };
    u8::try_from(raw).map_err(|_| {
        EngineError::node_config(
            node_id.to_owned(),
            format!("{protocol} 连接元数据 `{key}` 超过 u8 上限"),
        )
    })
}

pub(super) fn metadata_u16_or(
    metadata: &Value,
    key: &str,
    fallback: u16,
    node_id: &str,
    protocol: &str,
) -> Result<u16, EngineError> {
    match metadata.get(key).and_then(Value::as_u64) {
        Some(raw) => u16::try_from(raw).map_err(|_| {
            EngineError::node_config(
                node_id.to_owned(),
                format!("{protocol} 连接元数据 `{key}` 超过 u16 上限"),
            )
        }),
        None => Ok(fallback),
    }
}

pub(super) fn metadata_u32_or(
    metadata: &Value,
    key: &str,
    fallback: u32,
    node_id: &str,
    protocol: &str,
) -> Result<u32, EngineError> {
    match metadata.get(key).and_then(Value::as_u64) {
        Some(raw) => u32::try_from(raw).map_err(|_| {
            EngineError::node_config(
                node_id.to_owned(),
                format!("{protocol} 连接元数据 `{key}` 超过 u32 上限"),
            )
        }),
        None => Ok(fallback),
    }
}

pub(super) fn metadata_u8_or(
    metadata: &Value,
    key: &str,
    fallback: u8,
    node_id: &str,
    protocol: &str,
) -> Result<u8, EngineError> {
    match metadata.get(key).and_then(Value::as_u64) {
        Some(raw) => u8::try_from(raw).map_err(|_| {
            EngineError::node_config(
                node_id.to_owned(),
                format!("{protocol} 连接元数据 `{key}` 超过 u8 上限"),
            )
        }),
        None => Ok(fallback),
    }
}

pub(super) fn parse_u16_value(value: &str, node_id: &str, label: &str) -> Result<u16, EngineError> {
    value.trim().parse::<u16>().map_err(|error| {
        EngineError::node_config(
            node_id.to_owned(),
            format!("{label} `{value}` 不是有效 u16: {error}"),
        )
    })
}

pub(super) fn parse_hex_bytes(value: &str) -> Result<Vec<u8>, String> {
    let without_prefix = value.replace("0x", "").replace("0X", "");
    let mut cleaned = String::with_capacity(without_prefix.len());
    for ch in without_prefix.chars() {
        if ch.is_ascii_hexdigit() {
            cleaned.push(ch);
        } else if ch.is_ascii_whitespace() || matches!(ch, '_' | '-' | ':' | ',') {
        } else {
            return Err(format!("非法十六进制字符: {ch}"));
        }
    }
    #[cfg(feature = "io-can")]
    {
        hex::decode(&cleaned)
    }
    #[cfg(not(feature = "io-can"))]
    {
        if cleaned.len().is_multiple_of(2) {
            Ok(Vec::new())
        } else {
            Err("十六进制字符串长度必须是偶数".to_owned())
        }
    }
}

#[cfg(feature = "io-mqtt")]
pub(super) fn mqtt_qos(value: u8) -> QoS {
    match value {
        1 => QoS::AtLeastOnce,
        2 => QoS::ExactlyOnce,
        _ => QoS::AtMostOnce,
    }
}

#[cfg(feature = "io-mqtt")]
pub(super) fn qos_value(value: QoS) -> u8 {
    match value {
        QoS::AtMostOnce => 0,
        QoS::AtLeastOnce => 1,
        QoS::ExactlyOnce => 2,
    }
}
