#![allow(clippy::unwrap_used, clippy::expect_used)]

use connections::shared_connection_manager;
use nazh_core::{NodeTrait, PinKind};
use serde_json::Value;
use uuid::Uuid;

use crate::signal_decode::{ByteOrderSnapshot, DataTypeSnapshot, SignalSourceSnapshot};

use super::config::DeviceSignalReadConfig;
use super::reader::DeviceSignalReadNode;

fn register_source() -> SignalSourceSnapshot {
    SignalSourceSnapshot::Register {
        register: 40001,
        data_type: DataTypeSnapshot::Float32,
        bit: None,
    }
}

fn can_frame_source() -> SignalSourceSnapshot {
    SignalSourceSnapshot::CanFrame {
        can_id: 0x100,
        is_extended: false,
        byte_offset: 0,
        byte_length: 4,
        data_type: DataTypeSnapshot::Float32,
        byte_order: ByteOrderSnapshot::BigEndian,
    }
}

fn topic_source() -> SignalSourceSnapshot {
    SignalSourceSnapshot::Topic {
        topic: "factory/press/pressure".to_owned(),
    }
}

fn make_config() -> DeviceSignalReadConfig {
    DeviceSignalReadConfig {
        connection_id: None,
        device_id: "test_device".to_owned(),
        signal_id: "pressure".to_owned(),
        source: register_source(),
        scale: None,
        unit: Some("MPa".to_owned()),
        simulation: true,
        poll_timeout_ms: 2000,
    }
}

fn make_node() -> DeviceSignalReadNode {
    DeviceSignalReadNode::new("dsr-1", make_config(), shared_connection_manager()).unwrap()
}

#[test]
fn output_pins_声明_out_exec_与_latest_data() {
    let node = make_node();
    let pins = node.output_pins();
    assert_eq!(pins.len(), 2, "deviceSignalRead 应声明两个输出端口");

    let out_pin = pins.iter().find(|p| p.id == "out").expect("缺 out 引脚");
    assert_eq!(out_pin.pin_type, nazh_core::PinType::Json);
    assert_eq!(out_pin.kind, PinKind::Exec);

    let latest_pin = pins
        .iter()
        .find(|p| p.id == "latest")
        .expect("缺 latest 引脚");
    assert_eq!(latest_pin.pin_type, nazh_core::PinType::Json);
    assert_eq!(latest_pin.kind, PinKind::Data);
    assert!(!latest_pin.required, "Data 拉取式引脚 required=false");
}

#[test]
fn input_pin_保留默认_any() {
    let node = make_node();
    let pins = node.input_pins();
    assert_eq!(pins.len(), 1);
    assert_eq!(pins[0].pin_type, nazh_core::PinType::Any);
}

#[tokio::test]
async fn 缺少连接且未显式模拟时拒绝运行() {
    let config = DeviceSignalReadConfig {
        simulation: false,
        connection_id: None,
        ..make_config()
    };
    let node = DeviceSignalReadNode::new("dsr-1", config, shared_connection_manager()).unwrap();

    let err = node
        .transform(Uuid::new_v4(), Value::Null)
        .await
        .unwrap_err();

    assert!(
        matches!(err, nazh_core::EngineError::NodeConfig { .. }),
        "未配置连接或 simulation=true 时不应静默模拟: {err:?}"
    );
}

#[tokio::test]
async fn simulation_模式返回语义化输出() {
    let node = make_node();
    let execution = node.transform(Uuid::new_v4(), Value::Null).await.unwrap();

    let output = &execution.outputs[0];
    let payload = &output.payload;
    assert_eq!(payload["device_id"], "test_device");
    assert_eq!(payload["signal_id"], "pressure");
    assert!(payload.get("value").is_some());
    assert_eq!(payload["unit"], "MPa");
    assert!(payload.get("sampled_at").is_some());

    let metadata = output.metadata.as_ref().unwrap();
    assert_eq!(metadata["device_signal"]["simulated"], Value::Bool(true));
    assert_eq!(metadata["device_signal"]["source_type"], "register");
}

#[tokio::test]
async fn simulation_can_frame_源返回模拟值() {
    let config = DeviceSignalReadConfig {
        source: can_frame_source(),
        ..make_config()
    };
    let node = DeviceSignalReadNode::new("dsr-1", config, shared_connection_manager()).unwrap();
    let execution = node.transform(Uuid::new_v4(), Value::Null).await.unwrap();
    let metadata = execution.outputs[0].metadata.as_ref().unwrap();
    assert_eq!(metadata["device_signal"]["source_type"], "can_frame");
}

#[tokio::test]
async fn simulation_topic_源返回模拟值() {
    let config = DeviceSignalReadConfig {
        source: topic_source(),
        ..make_config()
    };
    let node = DeviceSignalReadNode::new("dsr-1", config, shared_connection_manager()).unwrap();
    let execution = node.transform(Uuid::new_v4(), Value::Null).await.unwrap();
    let metadata = execution.outputs[0].metadata.as_ref().unwrap();
    assert_eq!(metadata["device_signal"]["source_type"], "topic");
}

#[test]
fn signal_source_snapshot_serde_round_trip() {
    let source = register_source();
    let json = serde_json::to_string(&source).unwrap();
    let back: SignalSourceSnapshot = serde_json::from_str(&json).unwrap();

    if let SignalSourceSnapshot::Register {
        register,
        data_type,
        bit,
    } = &back
    {
        assert_eq!(*register, 40001);
        assert_eq!(*data_type, DataTypeSnapshot::Float32);
        assert!(bit.is_none());
    } else {
        panic!("期望 Register 变体");
    }
}

#[test]
fn signal_source_snapshot_json_format() {
    let source = register_source();
    let val = serde_json::to_value(&source).unwrap();
    assert_eq!(val["type"], "register");
    assert_eq!(val["register"], 40001);
    assert_eq!(val["data_type"], "float32");
}

#[test]
fn 无效_scale_表达式_构造时失败() {
    let config = DeviceSignalReadConfig {
        scale: Some("raw * / 2".to_owned()),
        ..make_config()
    };
    let result = DeviceSignalReadNode::new("dsr-1", config, shared_connection_manager());
    assert!(result.is_err(), "无效 scale 表达式应在构造时失败");
}

#[test]
fn 有效_scale_表达式_构造成功() {
    let config = DeviceSignalReadConfig {
        scale: Some("raw * 35 / 65535".to_owned()),
        ..make_config()
    };
    let result = DeviceSignalReadNode::new("dsr-1", config, shared_connection_manager());
    assert!(result.is_ok());
}

#[tokio::test]
async fn simulation_scale_求值正确() {
    let config = DeviceSignalReadConfig {
        scale: Some("raw * 2".to_owned()),
        ..make_config()
    };
    let node = DeviceSignalReadNode::new("dsr-1", config, shared_connection_manager()).unwrap();
    let execution = node.transform(Uuid::new_v4(), Value::Null).await.unwrap();

    let payload = &execution.outputs[0].payload;
    // simulate_value 对 Float32 返回 42.5, scale 后应为 85.0
    let val = payload["value"].as_f64().unwrap();
    assert!((val - 85.0).abs() < 0.01, "期望 85.0，得到 {val}");
}
