use serde_json::{Map, Value, json};

use super::SerialTriggerNodeConfig;

pub(super) fn bytes_to_hex(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789ABCDEF";
    let mut output = String::with_capacity(bytes.len().saturating_mul(3).saturating_sub(1));
    for (index, byte) in bytes.iter().enumerate() {
        if index > 0 {
            output.push(' ');
        }
        output.push(HEX[(*byte >> 4) as usize] as char);
        output.push(HEX[(*byte & 0x0F) as usize] as char);
    }
    output
}

pub(super) fn normalize_hex(value: &str) -> String {
    value
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_uppercase()
}

pub(super) fn normalize_ascii(value: &str, trim: bool) -> String {
    if trim {
        value.trim().to_owned()
    } else {
        value.to_owned()
    }
}

pub(super) fn frame_string<'a>(
    frame: &'a Map<String, Value>,
    key: &str,
    fallback: &'a str,
) -> &'a str {
    frame
        .get(key)
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .unwrap_or(fallback)
}

pub(super) fn frame_u64(frame: &Map<String, Value>, key: &str, fallback: u64) -> u64 {
    frame.get(key).and_then(Value::as_u64).unwrap_or(fallback)
}

/// 把已收到的帧字节解码为标准化 payload 字段。emit 路径与 transform 路径共用。
pub(super) fn build_serial_payload(
    frame_bytes: &[u8],
    config: &SerialTriggerNodeConfig,
) -> (Value, u64, String) {
    let ascii_raw = String::from_utf8_lossy(frame_bytes).to_string();
    let ascii = normalize_ascii(&ascii_raw, config.trim);
    let hex_raw = bytes_to_hex(frame_bytes);
    let hex = normalize_hex(&hex_raw);
    let encoding = config.encoding.trim().to_ascii_lowercase();
    let serial_data = if encoding == "hex" { &hex } else { &ascii };

    let mut payload_map = Map::new();
    for (key, value) in &config.inject {
        payload_map.insert(key.clone(), value.clone());
    }
    payload_map.insert("serial_data".to_owned(), json!(serial_data));
    payload_map.insert("serial_ascii".to_owned(), json!(ascii));
    payload_map.insert("serial_hex".to_owned(), json!(hex));

    #[allow(clippy::cast_possible_truncation)]
    let byte_len = frame_bytes.len() as u64;
    (Value::Object(payload_map), byte_len, encoding)
}
