//! RFC-0004 Phase 1 集成测试：`DeviceSpec` 解析 → Pin 声明映射 → `Store` 持久化。
//!
//! 验证完整的设备建模管道：YAML 文本解析为 `DeviceSpec`，信号映射为 `PinDefinition`，
//! 设备资产保存到 `SQLite` 并可检索。

#![allow(clippy::expect_used)]

use nazh_core::{PinDirection, PinType};
use nazh_dsl_core::{
    SignalType, parse_device_yaml, signal_to_direction, signal_to_pin_type,
    signals_to_pin_definitions,
};
use serde_json::json;
use store::Store;

const SAMPLE_DEVICE_YAML: &str = r#"
id: hydraulic_press_1
type: hydraulic_press
manufacturer: "测试液压"
model: YP-320T
connection:
  type: modbus-tcp
  id: press_modbus
  unit: 1
signals:
  - id: pressure
    signal_type: analog_input
    unit: MPa
    range: [0, 35]
    source:
      type: register
      register: 40001
      access: read
      data_type: float32
    scale: "raw * 35 / 65535"
  - id: servo_ready
    signal_type: digital_input
    source:
      type: register
      register: 40100
      access: read
      data_type: u16
      bit: 0
  - id: target_position
    signal_type: analog_output
    unit: mm
    range: [0, 150]
    source:
      type: register
      register: 40010
      access: write
      data_type: float32
alarms:
  - id: over_pressure
    condition: "pressure > 34"
    severity: critical
    action: emergency_stop
"#;

#[test]
fn 完整设备建模管道_yaml_解析到_pin_映射() {
    // 1. 解析 YAML
    let spec = parse_device_yaml(SAMPLE_DEVICE_YAML).expect("YAML 解析应成功");

    assert_eq!(spec.id, "hydraulic_press_1");
    assert_eq!(spec.device_type, "hydraulic_press");
    assert_eq!(spec.signals.len(), 3);
    assert_eq!(spec.alarms.len(), 1);

    // 2. 类型映射
    assert_eq!(signal_to_pin_type(SignalType::AnalogInput), PinType::Float);
    assert_eq!(signal_to_pin_type(SignalType::AnalogOutput), PinType::Float);
    assert_eq!(signal_to_pin_type(SignalType::DigitalInput), PinType::Bool);
    assert_eq!(signal_to_pin_type(SignalType::DigitalOutput), PinType::Bool);

    assert_eq!(
        signal_to_direction(SignalType::AnalogInput),
        PinDirection::Input
    );
    assert_eq!(
        signal_to_direction(SignalType::AnalogOutput),
        PinDirection::Output
    );

    // 3. 信号 → Pin 声明
    let pins = signals_to_pin_definitions(&spec.signals);
    assert_eq!(pins.len(), 3);

    // pressure pin
    let pressure_pin = &pins[0];
    assert_eq!(pressure_pin.id, "pressure");
    assert_eq!(pressure_pin.pin_type, PinType::Float);
    assert_eq!(pressure_pin.direction, PinDirection::Input);
    assert_eq!(pressure_pin.label, "Pressure (MPa)");
    assert!(pressure_pin.description.is_some());

    // servo_ready pin
    let servo_pin = &pins[1];
    assert_eq!(servo_pin.id, "servo_ready");
    assert_eq!(servo_pin.pin_type, PinType::Bool);
    assert_eq!(servo_pin.direction, PinDirection::Input);

    // target_position pin
    let target_pin = &pins[2];
    assert_eq!(target_pin.id, "target_position");
    assert_eq!(target_pin.pin_type, PinType::Float);
    assert_eq!(target_pin.direction, PinDirection::Output);
}

#[test]
fn 设备资产持久化_完整生命周期() {
    let store = Store::open_unpersisted();

    // 解析并序列化 DeviceSpec
    let spec = parse_device_yaml(SAMPLE_DEVICE_YAML).expect("YAML 解析应成功");
    let spec_json = serde_json::to_value(&spec).expect("序列化应成功");

    // 保存
    store
        .save_device_asset("press_1", "液压机 1", "hydraulic_press", &spec_json)
        .expect("保存应成功");

    // 加载
    let loaded = store
        .load_device_asset("press_1")
        .expect("加载应成功")
        .expect("资产应存在");
    assert_eq!(loaded.id, "press_1");
    assert_eq!(loaded.version, 1);
    assert_eq!(loaded.spec_json["id"], "hydraulic_press_1");

    // 更新（版本递增）— 保持合法 DeviceSpec 结构
    let updated_spec = json!({
        "id": "hydraulic_press_1",
        "type": "hydraulic_press_v2",
        "connection": {"type": "modbus-tcp", "id": "press_modbus"}
    });
    store
        .save_device_asset("press_1", "液压机 1 更新", "hydraulic_press", &updated_spec)
        .expect("更新保存应成功");

    let reloaded = store
        .load_device_asset("press_1")
        .expect("加载应成功")
        .expect("资产应存在");
    assert_eq!(reloaded.version, 2);
    assert_eq!(reloaded.name, "液压机 1 更新");

    // 版本历史
    let versions = store
        .list_asset_versions("press_1")
        .expect("版本列表应成功");
    assert_eq!(versions.len(), 2);
    assert_eq!(versions[0].version, 2);
    assert_eq!(versions[1].version, 1);

    // Pin schema 生成：用原始 spec 验证（v2 无 signals，pin 列表为空）
    let pins = signals_to_pin_definitions(&spec.signals);
    assert_eq!(pins.len(), 3);

    // 删除
    store.delete_device_asset("press_1").expect("删除应成功");
    assert!(
        store
            .load_device_asset("press_1")
            .expect("查询应成功")
            .is_none()
    );
}

#[test]
fn 多设备资产列表() {
    let store = Store::open_unpersisted();

    for i in 1..=3 {
        let spec = json!({"id": format!("dev_{i}"), "type": "sensor"});
        store
            .save_device_asset(&format!("dev_{i}"), &format!("设备 {i}"), "sensor", &spec)
            .expect("保存应成功");
    }

    let list = store.list_device_assets().expect("列表应成功");
    assert_eq!(list.len(), 3);
}
