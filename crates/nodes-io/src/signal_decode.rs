//! 设备信号共享解码模块。
//!
//! 提供协议无关的原始字节 → `DataType` 解码、`ByteOrder` 转换、bit 提取、
//! Rhai scale 表达式求值。供 `deviceSignalRead`、`deviceEventTrigger`
//! 及未来的 `deviceSignalWrite` 共用。
//!
//! 快照类型（`DataTypeSnapshot` / `ByteOrderSnapshot`）镜像
//! `dsl-core::DataType` / `dsl-core::ByteOrder`，独立定义以避免 Ring 1 对
//! DSL 层的直接依赖。conformance test 守护两者 serde 格式一致。

use rhai::{Dynamic, Engine, Scope, packages::Package};
use scripting::NazhScriptPackage;
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// 信号数据类型快照——镜像 `dsl-core::DataType`。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DataTypeSnapshot {
    Bool,
    U16,
    I16,
    U32,
    I32,
    Float32,
    Float64,
    String,
}

impl DataTypeSnapshot {
    /// Modbus 寄存器数量（每寄存器 2 字节）。
    pub fn modbus_register_count(self) -> u16 {
        match self {
            Self::U32 | Self::I32 | Self::Float32 => 2,
            Self::Float64 => 4,
            Self::Bool | Self::U16 | Self::I16 | Self::String => 1,
        }
    }

    /// 所需最小字节数。
    pub fn byte_count(self) -> usize {
        match self {
            Self::U32 | Self::I32 | Self::Float32 => 4,
            Self::Float64 => 8,
            Self::Bool | Self::U16 | Self::I16 => 2,
            Self::String => 0,
        }
    }
}

/// 字节序快照——镜像 `dsl-core::ByteOrder`。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ByteOrderSnapshot {
    #[default]
    BigEndian,
    LittleEndian,
}

/// 信号源快照——编译期从 `SignalSpec.source` 复制。
///
/// serde 格式与 `dsl-core::SignalSource` 字段级兼容，
/// conformance test 守护一致性（对标 `CapabilityImplSnapshot` 模式）。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum SignalSourceSnapshot {
    Register {
        register: u16,
        data_type: DataTypeSnapshot,
        #[serde(default)]
        bit: Option<u8>,
    },
    CanFrame {
        can_id: u32,
        #[serde(default)]
        is_extended: bool,
        byte_offset: u8,
        byte_length: u8,
        data_type: DataTypeSnapshot,
        #[serde(default)]
        byte_order: ByteOrderSnapshot,
    },
    Topic {
        topic: String,
    },
    SerialCommand {
        command: String,
    },
    EthercatPdo {
        #[serde(default)]
        slave_address: Option<u16>,
        pdo_index: u16,
        entry_index: u16,
        sub_index: u8,
        bit_len: u16,
    },
}

impl SignalSourceSnapshot {
    /// 返回 serde tag 值（如 "register"、"topic"）。
    pub fn type_tag(&self) -> &'static str {
        match self {
            Self::Register { .. } => "register",
            Self::CanFrame { .. } => "can_frame",
            Self::Topic { .. } => "topic",
            Self::SerialCommand { .. } => "serial_command",
            Self::EthercatPdo { .. } => "ethercat_pdo",
        }
    }
}

// ---- 错误类型 ----

#[derive(Debug, thiserror::Error)]
pub enum DecodeError {
    #[error("buffer too short: need {needed} bytes, got {actual}")]
    BufferTooShort { needed: usize, actual: usize },
    #[error("unsupported data type for bit extraction: {0:?}")]
    BitExtractionUnsupported(DataTypeSnapshot),
    #[error("bit index {index} out of range for {width}-bit value")]
    BitOutOfRange { index: u8, width: u8 },
    #[error("invalid UTF-8 in string decode: {0}")]
    InvalidUtf8(#[from] std::string::FromUtf8Error),
}

#[derive(Debug, thiserror::Error)]
pub enum ScaleError {
    #[error("Rhai scale expression compile failed: {0}")]
    CompileFailed(String),
    #[error("Rhai scale expression evaluation failed: {0}")]
    EvalFailed(String),
    #[error("scale expression returned non-numeric type")]
    NonNumericResult,
}

// ---- 解码函数 ----

/// 按 `DataType` 解码原始字节数组为目标 JSON Value。
///
/// `bit` 仅对 Bool/U16/I16 有效：提取第 `n` 位后返回 `Value::Bool`。
/// 其他 `data_type` 传 `bit = Some(_)` 将返回 `BitExtractionUnsupported`。
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

/// 对已解码值执行预编译的 Rhai scale 表达式。
///
/// `scale_ast` 为 `None` 时直接返回原值。
/// `engine` 应已在调用方构造时注册 `NazhScriptPackage` 并设置步数上限。
pub fn apply_scale_with_engine(
    raw_value: Value,
    scale_ast: &Option<rhai::AST>,
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
///
/// 注册 `NazhScriptPackage`，设置 `max_operations = 50_000`。
/// 在节点构造时调用一次，后续复用。
pub fn create_scale_engine() -> Engine {
    let mut engine = Engine::new();
    NazhScriptPackage::new().register_into_engine(&mut engine);
    engine.set_max_operations(scripting::default_max_operations());
    engine
}

/// 编译 scale 表达式为 Rhai AST。
///
/// `None` 或空字符串返回 `Ok(None)`。
pub fn compile_scale(scale: &Option<String>) -> Result<Option<rhai::AST>, ScaleError> {
    let Some(expr) = scale.as_deref() else {
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

// ---- 内部辅助 ----

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

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    // ---- decode_raw_bytes ----

    #[test]
    fn u16_big_endian() {
        let raw = [0x00, 0x2A];
        let val = decode_raw_bytes(
            &raw,
            DataTypeSnapshot::U16,
            ByteOrderSnapshot::BigEndian,
            None,
        )
        .unwrap();
        assert_eq!(val, Value::Number(serde_json::Number::from(42)));
    }

    #[test]
    fn u16_little_endian() {
        let raw = [0x2A, 0x00];
        let val = decode_raw_bytes(
            &raw,
            DataTypeSnapshot::U16,
            ByteOrderSnapshot::LittleEndian,
            None,
        )
        .unwrap();
        assert_eq!(val, Value::Number(serde_json::Number::from(42)));
    }

    #[test]
    fn i16_negative() {
        let raw = [0xFF, 0xF6]; // -10 in BE
        let val = decode_raw_bytes(
            &raw,
            DataTypeSnapshot::I16,
            ByteOrderSnapshot::BigEndian,
            None,
        )
        .unwrap();
        assert_eq!(val, Value::Number(serde_json::Number::from(-10i16)));
    }

    #[test]
    fn u32_big_endian() {
        let raw = [0x00, 0x00, 0x01, 0x00];
        let val = decode_raw_bytes(
            &raw,
            DataTypeSnapshot::U32,
            ByteOrderSnapshot::BigEndian,
            None,
        )
        .unwrap();
        assert_eq!(val, Value::Number(serde_json::Number::from(256u32)));
    }

    #[test]
    fn i32_negative() {
        let raw = [0xFF, 0xFF, 0xFF, 0xF6]; // -10 in BE
        let val = decode_raw_bytes(
            &raw,
            DataTypeSnapshot::I32,
            ByteOrderSnapshot::BigEndian,
            None,
        )
        .unwrap();
        assert_eq!(val, Value::Number(serde_json::Number::from(-10i32)));
    }

    #[test]
    fn float32_known_value() {
        // 使用非近似 PI 的浮点值避免 clippy::approx_constant
        let test_val: f32 = 2.5;
        let bytes = test_val.to_be_bytes();
        let val = decode_raw_bytes(
            &bytes,
            DataTypeSnapshot::Float32,
            ByteOrderSnapshot::BigEndian,
            None,
        )
        .unwrap();
        assert_eq!(
            val,
            Value::Number(serde_json::Number::from_f64(2.5).unwrap())
        );
    }

    #[test]
    fn float64_known_value() {
        let test_val: f64 = 2.5;
        let bytes = test_val.to_be_bytes();
        let val = decode_raw_bytes(
            &bytes,
            DataTypeSnapshot::Float64,
            ByteOrderSnapshot::BigEndian,
            None,
        )
        .unwrap();
        assert_eq!(
            val,
            Value::Number(serde_json::Number::from_f64(2.5).unwrap())
        );
    }

    #[test]
    fn bool_bit_extraction() {
        // 0b1010, bit 0 = false, bit 1 = true
        let raw = [0x00, 0x0A]; // 10 in BE
        let val0 = decode_raw_bytes(
            &raw,
            DataTypeSnapshot::Bool,
            ByteOrderSnapshot::BigEndian,
            Some(0),
        )
        .unwrap();
        assert_eq!(val0, Value::Bool(false));
        let val1 = decode_raw_bytes(
            &raw,
            DataTypeSnapshot::Bool,
            ByteOrderSnapshot::BigEndian,
            Some(1),
        )
        .unwrap();
        assert_eq!(val1, Value::Bool(true));
        let val3 = decode_raw_bytes(
            &raw,
            DataTypeSnapshot::Bool,
            ByteOrderSnapshot::BigEndian,
            Some(3),
        )
        .unwrap();
        assert_eq!(val3, Value::Bool(true));
    }

    #[test]
    fn u16_bit_extraction() {
        let raw = [0x00, 0x05]; // 0b101
        let bit0 = decode_raw_bytes(
            &raw,
            DataTypeSnapshot::U16,
            ByteOrderSnapshot::BigEndian,
            Some(0),
        )
        .unwrap();
        assert_eq!(bit0, Value::Bool(true));
        let bit1 = decode_raw_bytes(
            &raw,
            DataTypeSnapshot::U16,
            ByteOrderSnapshot::BigEndian,
            Some(1),
        )
        .unwrap();
        assert_eq!(bit1, Value::Bool(false));
        let bit2 = decode_raw_bytes(
            &raw,
            DataTypeSnapshot::U16,
            ByteOrderSnapshot::BigEndian,
            Some(2),
        )
        .unwrap();
        assert_eq!(bit2, Value::Bool(true));
    }

    #[test]
    fn string_decode() {
        let raw = b"hello".as_slice();
        let val = decode_raw_bytes(
            raw,
            DataTypeSnapshot::String,
            ByteOrderSnapshot::BigEndian,
            None,
        )
        .unwrap();
        assert_eq!(val, Value::String("hello".to_owned()));
    }

    #[test]
    fn buffer_too_short() {
        let raw = [0x00];
        let err = decode_raw_bytes(
            &raw,
            DataTypeSnapshot::U16,
            ByteOrderSnapshot::BigEndian,
            None,
        )
        .unwrap_err();
        assert!(matches!(
            err,
            DecodeError::BufferTooShort {
                needed: 2,
                actual: 1
            }
        ));
    }

    #[test]
    fn bit_extraction_unsupported_for_float() {
        let raw = [0x00, 0x00, 0x00, 0x00];
        let err = decode_raw_bytes(
            &raw,
            DataTypeSnapshot::Float32,
            ByteOrderSnapshot::BigEndian,
            Some(0),
        )
        .unwrap_err();
        assert!(matches!(
            err,
            DecodeError::BitExtractionUnsupported(DataTypeSnapshot::Float32)
        ));
    }

    #[test]
    fn bit_out_of_range() {
        let raw = [0x00, 0x01];
        let err = decode_raw_bytes(
            &raw,
            DataTypeSnapshot::U16,
            ByteOrderSnapshot::BigEndian,
            Some(20),
        )
        .unwrap_err();
        assert!(matches!(
            err,
            DecodeError::BitOutOfRange {
                index: 20,
                width: 16
            }
        ));
    }

    // ---- apply_scale_with_engine ----

    #[test]
    fn scale_none_passthrough() {
        let engine = create_scale_engine();
        let val = Value::Number(serde_json::Number::from(42));
        let result = apply_scale_with_engine(val.clone(), &None, &engine).unwrap();
        assert_eq!(result, val);
    }

    #[test]
    fn scale_doubles_value() {
        let engine = create_scale_engine();
        let ast = compile_scale(&Some("raw * 2".to_owned())).unwrap().unwrap();
        let val = Value::Number(serde_json::Number::from(21));
        let result = apply_scale_with_engine(val, &Some(ast), &engine).unwrap();
        assert_eq!(result, Value::Number(serde_json::Number::from(42)));
    }

    #[test]
    fn scale_expression_with_float() {
        let engine = create_scale_engine();
        let ast = compile_scale(&Some("raw * 35 / 65535".to_owned()))
            .unwrap()
            .unwrap();
        // Rhai 整数除法: 10000 * 35 / 65535 = 350000 / 65535 = 5 (truncated)
        let val = Value::Number(serde_json::Number::from(10000));
        let result = apply_scale_with_engine(val, &Some(ast), &engine).unwrap();
        match result {
            Value::Number(n) => {
                let f = n.as_f64().unwrap();
                assert!((f - 5.0).abs() < 0.01, "期望 ~5.0，得到 {f}");
            }
            other => panic!("期望 Number，得到 {other:?}"),
        }
    }

    #[test]
    fn scale_empty_string_returns_none_ast() {
        let result = compile_scale(&Some(String::new())).unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn scale_invalid_expression_fails() {
        let err = compile_scale(&Some("raw *".to_owned())).unwrap_err();
        assert!(matches!(err, ScaleError::CompileFailed(_)));
    }

    // ---- serde conformance ----

    #[test]
    fn data_type_snapshot_serde_round_trip() {
        for dt in [
            DataTypeSnapshot::Bool,
            DataTypeSnapshot::U16,
            DataTypeSnapshot::I16,
            DataTypeSnapshot::U32,
            DataTypeSnapshot::I32,
            DataTypeSnapshot::Float32,
            DataTypeSnapshot::Float64,
            DataTypeSnapshot::String,
        ] {
            let json = serde_json::to_string(&dt).unwrap();
            let back: DataTypeSnapshot = serde_json::from_str(&json).unwrap();
            assert_eq!(dt, back, "DataTypeSnapshot round-trip 失败: {dt:?}");
        }
    }

    #[test]
    fn byte_order_snapshot_serde_round_trip() {
        for bo in [
            ByteOrderSnapshot::BigEndian,
            ByteOrderSnapshot::LittleEndian,
        ] {
            let json = serde_json::to_string(&bo).unwrap();
            let back: ByteOrderSnapshot = serde_json::from_str(&json).unwrap();
            assert_eq!(bo, back);
        }
    }

    #[test]
    fn data_type_snapshot_json_format_matches_dsl_core() {
        assert_eq!(
            serde_json::to_string(&DataTypeSnapshot::Bool).unwrap(),
            r#""bool""#
        );
        assert_eq!(
            serde_json::to_string(&DataTypeSnapshot::U16).unwrap(),
            r#""u16""#
        );
        assert_eq!(
            serde_json::to_string(&DataTypeSnapshot::Float32).unwrap(),
            r#""float32""#
        );
        assert_eq!(
            serde_json::to_string(&ByteOrderSnapshot::BigEndian).unwrap(),
            r#""big_endian""#
        );
        assert_eq!(
            serde_json::to_string(&ByteOrderSnapshot::LittleEndian).unwrap(),
            r#""little_endian""#
        );
    }
}
