use serde_json::{Map, json};

use super::*;

#[test]
fn build_serial_payload_包含三种格式() {
    let config = SerialTriggerNodeConfig {
        port_path: "/dev/null".to_owned(),
        baud_rate: 9600,
        data_bits: 8,
        parity: "none".to_owned(),
        stop_bits: 1,
        flow_control: "none".to_owned(),
        encoding: "ascii".to_owned(),
        delimiter: "\n".to_owned(),
        read_timeout_ms: 100,
        idle_gap_ms: 80,
        max_frame_bytes: 512,
        trim: true,
        inject: Map::new(),
    };
    let (payload, byte_len, encoding) = frame::build_serial_payload(b"hello", &config);
    let obj = payload.as_object().unwrap();
    assert_eq!(obj.get("serial_data"), Some(&json!("hello")));
    assert_eq!(obj.get("serial_ascii"), Some(&json!("hello")));
    assert_eq!(obj.get("serial_hex"), Some(&json!("68 65 6C 6C 6F")));
    assert_eq!(byte_len, 5);
    assert_eq!(encoding, "ascii");
}

#[test]
fn build_serial_payload_hex_encoding_切换主显示() {
    let mut config = SerialTriggerNodeConfig {
        port_path: String::new(),
        baud_rate: 9600,
        data_bits: 8,
        parity: "none".to_owned(),
        stop_bits: 1,
        flow_control: "none".to_owned(),
        encoding: "hex".to_owned(),
        delimiter: "\n".to_owned(),
        read_timeout_ms: 100,
        idle_gap_ms: 80,
        max_frame_bytes: 512,
        trim: true,
        inject: Map::new(),
    };
    config.inject.insert("source".to_owned(), json!("scanner"));
    let (payload, _byte_len, encoding) = frame::build_serial_payload(&[0xAB, 0xCD], &config);
    let obj = payload.as_object().unwrap();
    assert_eq!(obj.get("serial_data"), Some(&json!("AB CD")));
    assert_eq!(obj.get("serial_hex"), Some(&json!("AB CD")));
    assert_eq!(obj.get("source"), Some(&json!("scanner")));
    assert_eq!(encoding, "hex");
}

#[test]
fn is_serial_connection_kind_接受常见别名() {
    assert!(serial_loop::is_serial_connection_kind("serial"));
    assert!(serial_loop::is_serial_connection_kind("Serial"));
    assert!(serial_loop::is_serial_connection_kind("UART"));
    assert!(serial_loop::is_serial_connection_kind("RS485"));
    assert!(!serial_loop::is_serial_connection_kind("mqtt"));
}

#[test]
fn decode_serial_delimiter_支持转义与hex() {
    assert_eq!(serial_loop::decode_serial_delimiter(""), Vec::<u8>::new());
    assert_eq!(serial_loop::decode_serial_delimiter("\\n"), b"\n");
    assert_eq!(serial_loop::decode_serial_delimiter("\\r\\n"), b"\r\n");
    assert_eq!(serial_loop::decode_serial_delimiter("hex:0d0a"), b"\r\n");
    assert_eq!(serial_loop::decode_serial_delimiter("0xFF"), vec![0xFF]);
}

#[test]
fn bytes_to_hex_格式正确() {
    assert_eq!(frame::bytes_to_hex(&[]), "");
    assert_eq!(frame::bytes_to_hex(&[0xAB]), "AB");
    assert_eq!(frame::bytes_to_hex(&[0x00, 0xFF, 0x10]), "00 FF 10");
}
