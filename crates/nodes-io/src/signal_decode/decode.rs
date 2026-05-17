//! 原始字节解码、Rhai scale 求值、Topic payload 解析。

use rhai::{Dynamic, Engine, Scope, packages::Package};
use scripting::NazhScriptPackage;
use serde_json::Value;

use super::types::{ByteOrderSnapshot, DataTypeSnapshot, DecodeError, ScaleError};

/// 按 `DataType` 解码原始字节数组为目标 JSON Value。
pub fn decode_raw_bytes(
    raw: &[u8],
    data_type: DataTypeSnapshot,
    byte_order: ByteOrderSnapshot,
    bit: Option<u8>,
) -> Result<Value, DecodeError> {
    if bit.is_some()
        && !matches!(
            data_type,
            DataTypeSnapshot::Bool | DataTypeSnapshot::U16 | DataTypeSnapshot::I16
        )
    {
        return Err(DecodeError::BitExtractionUnsupported(data_type));
    }

    match data_type {
        DataTypeSnapshot::Bool => decode_bool(raw, byte_order, bit),
        DataTypeSnapshot::U16 => decode_u16_value(raw, byte_order, bit),
        DataTypeSnapshot::I16 => decode_i16_value(raw, byte_order, bit),
        DataTypeSnapshot::U32 => decode_u32_value(raw, byte_order),
        DataTypeSnapshot::I32 => decode_i32_value(raw, byte_order),
        DataTypeSnapshot::Float32 => decode_float32_value(raw, byte_order),
        DataTypeSnapshot::Float64 => decode_float64_value(raw, byte_order),
        DataTypeSnapshot::String => {
            let s = String::from_utf8(raw.to_vec())?;
            Ok(Value::String(s))
        }
    }
}

/// 对已解码值执行预编译的 Rhai scale 表达式。
pub fn apply_scale_with_engine(
    raw_value: Value,
    scale_ast: Option<&rhai::AST>,
    engine: &Engine,
) -> Result<Value, ScaleError> {
    let Some(ast) = scale_ast else {
        return Ok(raw_value);
    };

    let raw_dynamic = rhai::serde::to_dynamic(raw_value)
        .map_err(|e| ScaleError::EvalFailed(format!("raw 值转 Dynamic 失败: {e}")))?;

    let mut scope = Scope::new();
    scope.push_dynamic("raw", raw_dynamic);

    let result = engine
        .eval_ast_with_scope::<Dynamic>(&mut scope, ast)
        .map_err(|e| ScaleError::EvalFailed(e.to_string()))?;

    let value: Value = rhai::serde::from_dynamic(&result)
        .map_err(|e| ScaleError::EvalFailed(format!("Rhai 结果转 JSON 失败: {e}")))?;

    Ok(value)
}

/// 构造用于 scale 求值的 Rhai Engine。
pub fn create_scale_engine() -> Engine {
    let mut engine = Engine::new();
    NazhScriptPackage::new().register_into_engine(&mut engine);
    engine.set_max_operations(scripting::default_max_operations());
    engine
}

/// 编译 scale 表达式为 Rhai AST。
pub fn compile_scale(scale: Option<&String>) -> Result<Option<rhai::AST>, ScaleError> {
    let Some(expr) = scale else {
        return Ok(None);
    };
    if expr.is_empty() {
        return Ok(None);
    }
    let engine = Engine::new();
    engine
        .compile(expr)
        .map(Some)
        .map_err(|e| ScaleError::CompileFailed(e.to_string()))
}

/// 解码 MQTT Topic payload 字节为 JSON `Value`。
pub fn decode_topic_payload(payload: &[u8], signal_id: &str) -> Value {
    if let Ok(parsed) = serde_json::from_slice::<Value>(payload) {
        match parsed {
            v @ (Value::Number(_) | Value::Bool(_)) => return v,
            Value::String(s) => {
                if let Some(n) = serde_json::Number::from_f64(s.parse::<f64>().unwrap_or(f64::NAN))
                {
                    return Value::Number(n);
                }
                return Value::String(s);
            }
            other => return other,
        }
    }

    if let Ok(s) = std::str::from_utf8(payload) {
        if let Some(n) = serde_json::Number::from_f64(s.parse::<f64>().unwrap_or(f64::NAN)) {
            return Value::Number(n);
        }
        return Value::String(s.to_owned());
    }

    let hex: String =
        payload
            .iter()
            .fold(String::with_capacity(payload.len() * 2), |mut acc, b| {
                use std::fmt::Write;
                let _ = write!(acc, "{b:02X}");
                acc
            });
    tracing::warn!(signal_id, hex, "Topic payload 无法解码，使用十六进制回退");
    Value::String(hex)
}

/// 从 `EtherCAT` PDO 输入字节流中按字节偏移提取指定长度的数据。
pub fn extract_pdo_bytes(
    pdo_data: &[u8],
    byte_offset: usize,
    byte_len: usize,
) -> Result<&[u8], DecodeError> {
    let end = byte_offset
        .checked_add(byte_len)
        .ok_or(DecodeError::BufferTooShort {
            needed: usize::MAX,
            actual: pdo_data.len(),
        })?;
    if end > pdo_data.len() {
        return Err(DecodeError::BufferTooShort {
            needed: end,
            actual: pdo_data.len(),
        });
    }
    Ok(&pdo_data[byte_offset..end])
}

// ---- 内部辅助 ----

fn decode_bool(
    raw: &[u8],
    byte_order: ByteOrderSnapshot,
    bit: Option<u8>,
) -> Result<Value, DecodeError> {
    let val = decode_u16(raw, byte_order)?;
    match bit {
        Some(idx) => {
            check_bit_index(idx, 16)?;
            Ok(Value::Bool(val & (1 << idx) != 0))
        }
        None => Ok(Value::Bool(val != 0)),
    }
}

fn decode_u16_value(
    raw: &[u8],
    byte_order: ByteOrderSnapshot,
    bit: Option<u8>,
) -> Result<Value, DecodeError> {
    let val = decode_u16(raw, byte_order)?;
    match bit {
        Some(idx) => {
            check_bit_index(idx, 16)?;
            Ok(Value::Bool(val & (1 << idx) != 0))
        }
        None => Ok(Value::Number(serde_json::Number::from(val))),
    }
}

fn decode_i16_value(
    raw: &[u8],
    byte_order: ByteOrderSnapshot,
    bit: Option<u8>,
) -> Result<Value, DecodeError> {
    let val = decode_u16(raw, byte_order)?;
    if let Some(idx) = bit {
        check_bit_index(idx, 16)?;
        Ok(Value::Bool(val & (1 << idx) != 0))
    } else {
        let signed = i16::from_ne_bytes(val.to_ne_bytes());
        Ok(Value::Number(serde_json::Number::from(signed)))
    }
}

fn decode_u32_value(raw: &[u8], byte_order: ByteOrderSnapshot) -> Result<Value, DecodeError> {
    let bytes = read_bytes::<4>(raw)?;
    let val = match byte_order {
        ByteOrderSnapshot::BigEndian => u32::from_be_bytes(bytes),
        ByteOrderSnapshot::LittleEndian => u32::from_le_bytes(bytes),
    };
    Ok(Value::Number(serde_json::Number::from(val)))
}

fn decode_i32_value(raw: &[u8], byte_order: ByteOrderSnapshot) -> Result<Value, DecodeError> {
    let bytes = read_bytes::<4>(raw)?;
    let val = match byte_order {
        ByteOrderSnapshot::BigEndian => i32::from_be_bytes(bytes),
        ByteOrderSnapshot::LittleEndian => i32::from_le_bytes(bytes),
    };
    Ok(Value::Number(serde_json::Number::from(val)))
}

fn decode_float32_value(raw: &[u8], byte_order: ByteOrderSnapshot) -> Result<Value, DecodeError> {
    let bytes = read_bytes::<4>(raw)?;
    let bits = match byte_order {
        ByteOrderSnapshot::BigEndian => u32::from_be_bytes(bytes),
        ByteOrderSnapshot::LittleEndian => u32::from_le_bytes(bytes),
    };
    Ok(float_to_value(f64::from(f32::from_bits(bits))))
}

fn decode_float64_value(raw: &[u8], byte_order: ByteOrderSnapshot) -> Result<Value, DecodeError> {
    let bytes = read_bytes::<8>(raw)?;
    let bits = match byte_order {
        ByteOrderSnapshot::BigEndian => u64::from_be_bytes(bytes),
        ByteOrderSnapshot::LittleEndian => u64::from_le_bytes(bytes),
    };
    Ok(float_to_value(f64::from_bits(bits)))
}

fn decode_u16(raw: &[u8], byte_order: ByteOrderSnapshot) -> Result<u16, DecodeError> {
    let bytes = read_bytes::<2>(raw)?;
    Ok(match byte_order {
        ByteOrderSnapshot::BigEndian => u16::from_be_bytes(bytes),
        ByteOrderSnapshot::LittleEndian => u16::from_le_bytes(bytes),
    })
}

fn read_bytes<const N: usize>(raw: &[u8]) -> Result<[u8; N], DecodeError> {
    if raw.len() < N {
        return Err(DecodeError::BufferTooShort {
            needed: N,
            actual: raw.len(),
        });
    }
    raw[..N]
        .try_into()
        .map_err(|_| DecodeError::BufferTooShort {
            needed: N,
            actual: raw.len(),
        })
}

fn check_bit_index(idx: u8, width: u8) -> Result<(), DecodeError> {
    if idx >= width {
        return Err(DecodeError::BitOutOfRange { index: idx, width });
    }
    Ok(())
}

fn float_to_value(f: f64) -> Value {
    serde_json::Number::from_f64(f).map_or(Value::Null, Value::Number)
}
