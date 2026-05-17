#![allow(clippy::unwrap_used, clippy::expect_used)]

use serde_json::Value;

use super::decode::{
    apply_scale_with_engine, compile_scale, create_scale_engine, decode_raw_bytes,
    decode_topic_payload, extract_pdo_bytes,
};
use super::types::{ByteOrderSnapshot, DataTypeSnapshot, DecodeError, ScaleError};

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
    let raw = [0xFF, 0xF6];
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
    let raw = [0xFF, 0xFF, 0xFF, 0xF6];
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
    let raw = [0x00, 0x0A];
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
    let raw = [0x00, 0x05];
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
    let result = apply_scale_with_engine(val.clone(), None, &engine).unwrap();
    assert_eq!(result, val);
}

#[test]
fn scale_doubles_value() {
    let engine = create_scale_engine();
    let ast = compile_scale(Some(&"raw * 2".to_owned())).unwrap().unwrap();
    let val = Value::Number(serde_json::Number::from(21));
    let result = apply_scale_with_engine(val, Some(&ast), &engine).unwrap();
    assert_eq!(result, Value::Number(serde_json::Number::from(42)));
}

#[test]
fn scale_expression_with_float() {
    let engine = create_scale_engine();
    let ast = compile_scale(Some(&"raw * 35 / 65535".to_owned()))
        .unwrap()
        .unwrap();
    let val = Value::Number(serde_json::Number::from(10000));
    let result = apply_scale_with_engine(val, Some(&ast), &engine).unwrap();
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
    let result = compile_scale(Some(&String::new())).unwrap();
    assert!(result.is_none());
}

#[test]
fn scale_invalid_expression_fails() {
    let err = compile_scale(Some(&"raw *".to_owned())).unwrap_err();
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

// ---- decode_topic_payload ----

#[test]
fn topic_json_number() {
    let val = decode_topic_payload(b"42.5", "test");
    assert_eq!(
        val,
        Value::Number(serde_json::Number::from_f64(42.5).unwrap())
    );
}

#[test]
fn topic_json_bool() {
    let val = decode_topic_payload(b"true", "test");
    assert_eq!(val, Value::Bool(true));
}

#[test]
fn topic_string_number() {
    let val = decode_topic_payload(b"100", "test");
    assert_eq!(val, Value::Number(serde_json::Number::from(100)));
}

#[test]
fn topic_utf8_text() {
    let val = decode_topic_payload("温度正常".as_bytes(), "test");
    assert_eq!(val, Value::String("温度正常".to_owned()));
}

#[test]
fn topic_hex_fallback() {
    let val = decode_topic_payload(&[0xFF, 0xFE], "test");
    assert_eq!(val, Value::String("FFFE".to_owned()));
}

// ---- extract_pdo_bytes ----

#[test]
fn pdo_extract_valid() {
    let data = [0x01, 0x02, 0x03, 0x04, 0x05, 0x06];
    let slice = extract_pdo_bytes(&data, 2, 2).unwrap();
    assert_eq!(slice, &[0x03, 0x04]);
}

#[test]
fn pdo_extract_out_of_bounds() {
    let data = [0x01, 0x02];
    let err = extract_pdo_bytes(&data, 1, 4).unwrap_err();
    assert!(matches!(
        err,
        DecodeError::BufferTooShort {
            needed: 5,
            actual: 2
        }
    ));
}
