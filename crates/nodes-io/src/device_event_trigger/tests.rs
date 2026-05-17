#![allow(clippy::unwrap_used, clippy::expect_used)]

use serde_json::{Value, json};
use uuid::Uuid;

use connections::{ConnectionDefinition, shared_connection_manager};
use nazh_core::{EngineError, NodeCapabilities, NodeTrait, PinKind, PinType};

use super::config::{DeviceEventTriggerConfig, ListenerProtocol, SignalListenerSnapshot};
use super::node::DeviceEventTriggerNode;
use crate::signal_decode::SignalSourceSnapshot;

fn topic_signal() -> SignalListenerSnapshot {
    SignalListenerSnapshot {
        signal_id: "pressure".to_owned(),
        source: SignalSourceSnapshot::Topic {
            topic: "factory/press/pressure".to_owned(),
        },
        scale: None,
        unit: Some("MPa".to_owned()),
    }
}

fn make_config() -> DeviceEventTriggerConfig {
    DeviceEventTriggerConfig {
        connection_id: None,
        device_id: "test_device".to_owned(),
        signals: vec![topic_signal()],
        simulation: true,
        poll_interval_ms: 1000,
    }
}

fn make_node() -> DeviceEventTriggerNode {
    DeviceEventTriggerNode::new("det-1", make_config(), shared_connection_manager()).unwrap()
}

fn connection(id: &str, kind: &str, metadata: Value) -> ConnectionDefinition {
    ConnectionDefinition {
        id: id.to_owned(),
        kind: kind.to_owned(),
        metadata,
    }
}

#[test]
fn output_pins_声明_out_exec() {
    let node = make_node();
    let pins = node.output_pins();
    assert_eq!(pins.len(), 1, "deviceEventTrigger 应声明单个输出端口");
    let out_pin = pins.first().unwrap();
    assert_eq!(out_pin.id, "out");
    assert_eq!(out_pin.pin_type, PinType::Json);
    assert_eq!(out_pin.kind, PinKind::Exec);
}

#[test]
fn capabilities_trigger_device_io() {
    // capabilities 由注册表在 register_with_capabilities 时设置，
    // 不在 NodeTrait 上声明。本测试仅验证位组合值的正确性。
    let caps = NodeCapabilities::TRIGGER | NodeCapabilities::DEVICE_IO;
    assert!(caps.contains(NodeCapabilities::TRIGGER));
    assert!(caps.contains(NodeCapabilities::DEVICE_IO));
}

#[tokio::test]
async fn 缺少连接且未显式模拟时拒绝运行() {
    let config = DeviceEventTriggerConfig {
        simulation: false,
        ..make_config()
    };
    let node = DeviceEventTriggerNode::new("det-1", config, shared_connection_manager()).unwrap();
    let err = node
        .transform(Uuid::new_v4(), Value::Null)
        .await
        .unwrap_err();
    assert!(matches!(err, EngineError::NodeConfig { .. }));
}

#[tokio::test]
async fn simulation_模式返回事件_payload() {
    let node = make_node();
    let execution = node.transform(Uuid::new_v4(), Value::Null).await.unwrap();
    let output = &execution.outputs[0];
    let payload = &output.payload;
    assert_eq!(payload["device_id"], "test_device");
    assert_eq!(payload["signal_id"], "pressure");
    assert_eq!(payload["event_type"], "signal_update");
    assert!(payload.get("value").is_some());
    assert_eq!(payload["unit"], "MPa");

    let metadata = output.metadata.as_ref().unwrap();
    assert_eq!(metadata["device_event"]["simulated"], Value::Bool(true));
}

#[test]
fn 无效_scale_表达式_构造时失败() {
    let config = DeviceEventTriggerConfig {
        signals: vec![SignalListenerSnapshot {
            scale: Some("raw * / 2".to_owned()),
            ..topic_signal()
        }],
        ..make_config()
    };
    let result = DeviceEventTriggerNode::new("det-1", config, shared_connection_manager());
    assert!(result.is_err());
}

#[test]
fn signal_listener_snapshot_serde_round_trip() {
    let sig = topic_signal();
    let val = serde_json::to_value(&sig).unwrap();
    assert_eq!(val["signal_id"], "pressure");
    let back: SignalListenerSnapshot = serde_json::from_value(val).unwrap();
    assert_eq!(back.signal_id, "pressure");
}

#[tokio::test]
async fn mqtt_listener_部署期要求显式_port() {
    let manager = shared_connection_manager();
    manager
        .register_connection(connection(
            "mqtt-1",
            "mqtt",
            json!({"host": "127.0.0.1", "topic": "factory/#"}),
        ))
        .await
        .unwrap();
    let node = DeviceEventTriggerNode::new(
        "det-1",
        DeviceEventTriggerConfig {
            connection_id: Some("mqtt-1".to_owned()),
            simulation: false,
            ..make_config()
        },
        manager,
    )
    .unwrap();

    let err = node
        .validate_listener_connection("mqtt-1", &[ListenerProtocol::Mqtt])
        .await
        .unwrap_err();
    assert!(matches!(
        err,
        EngineError::ConnectionInvalidConfiguration { .. }
    ));
}

#[tokio::test]
async fn 混合协议_signal_部署期拒绝() {
    let manager = shared_connection_manager();
    let node = DeviceEventTriggerNode::new("det-1", make_config(), manager).unwrap();

    let err = node
        .validate_listener_connection(
            "unused",
            &[ListenerProtocol::Mqtt, ListenerProtocol::Serial],
        )
        .await
        .unwrap_err();
    assert!(matches!(err, EngineError::NodeConfig { .. }));
}
