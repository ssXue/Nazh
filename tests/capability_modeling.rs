//! RFC-0004 Phase 2 集成测试：Capability DSL 解析 → 验证 → 自动生成 → Store 持久化。
//!
//! 验证完整的能力建模管道：YAML 解析 `CapabilitySpec`，静态校验，
//! 从设备信号自动生成能力，保存到 `SQLite` 并可检索。

#![allow(clippy::expect_used)]

use nazh_dsl_core::{
    CapabilityImpl, CapabilitySpec, SafetyLevel, generate_capabilities_from_device,
    parse_capability_yaml, parse_device_yaml,
};
use serde_json::json;
use store::Store;

const SAMPLE_CAPABILITY_YAML: &str = r#"
id: hydraulic_axis.move_to
device_id: hydraulic_press_1
description: "控制液压轴移动到指定位置"
inputs:
  - id: position
    unit: mm
    range: [0, 150]
    required: true
outputs:
  - id: position_reached
    type: bool
preconditions:
  - "servo_ready == true"
  - "emergency_stop == false"
effects:
  - "axis_state = moving"
implementation:
  type: modbus-write
  register: 40010
  value: "${position}"
fallback:
  - hydraulic_axis.stop
safety:
  level: high
  requires_approval: false
  max_execution_time: 30s
"#;

const SAMPLE_DEVICE_YAML: &str = r#"
id: hydraulic_press_1
type: hydraulic_press
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
  - id: target_position
    signal_type: analog_output
    unit: mm
    range: [0, 150]
    source:
      type: register
      register: 40010
      access: write
      data_type: float32
  - id: servo_enable
    signal_type: digital_output
    source:
      type: register
      register: 40200
      access: write
      data_type: u16
      bit: 0
alarms:
  - id: over_pressure
    condition: "pressure > 34"
    severity: critical
"#;

#[test]
fn capability_yaml_解析_验证_和_持久化_完整管道() {
    // 1. 解析 YAML
    let spec = parse_capability_yaml(SAMPLE_CAPABILITY_YAML).expect("Capability YAML 解析应成功");
    assert_eq!(spec.id, "hydraulic_axis.move_to");
    assert_eq!(spec.device_id, "hydraulic_press_1");
    assert_eq!(spec.inputs.len(), 1);
    assert_eq!(spec.inputs[0].range.map(|r| r.max), Some(150.0));
    assert_eq!(spec.safety.level, SafetyLevel::High);

    // 2. 验证
    spec.validate().expect("合法 CapabilitySpec 校验应通过");

    // 3. 持久化
    let store = Store::open_unpersisted();
    let spec_json = serde_json::to_value(&spec).expect("序列化应成功");
    store
        .save_capability(
            "axis.move_to",
            "hydraulic_press_1",
            "移动轴",
            Some("控制轴移动"),
            &spec_json,
        )
        .expect("保存应成功");

    // 4. 加载验证
    let loaded = store
        .load_capability("axis.move_to")
        .expect("加载应成功")
        .expect("能力应存在");
    assert_eq!(loaded.id, "axis.move_to");
    assert_eq!(loaded.device_id, "hydraulic_press_1");
    assert_eq!(loaded.version, 1);
    assert_eq!(loaded.spec_json["id"], "hydraulic_axis.move_to");
    assert_eq!(loaded.spec_json["device_id"], "hydraulic_press_1");

    // 5. 版本递增
    store
        .save_capability(
            "axis.move_to",
            "hydraulic_press_1",
            "移动轴 v2",
            None,
            &spec_json,
        )
        .expect("更新保存应成功");
    let reloaded = store
        .load_capability("axis.move_to")
        .expect("加载应成功")
        .expect("能力应存在");
    assert_eq!(reloaded.version, 2);

    // 6. 删除
    store.delete_capability("axis.move_to").expect("删除应成功");
    assert!(
        store
            .load_capability("axis.move_to")
            .expect("查询应成功")
            .is_none()
    );
}

#[test]
fn 从设备信号自动生成能力_并持久化() {
    // 1. 解析设备
    let device = parse_device_yaml(SAMPLE_DEVICE_YAML).expect("Device YAML 解析应成功");
    assert_eq!(device.signals.len(), 3);

    // 2. 自动生成能力
    let caps = generate_capabilities_from_device(&device);
    assert_eq!(caps.len(), 2); // target_position (AnalogOutput) + servo_enable (DigitalOutput)

    // 验证第一个生成的能力
    let write_target = caps
        .iter()
        .find(|c| c.id.contains("target_position"))
        .expect("应有 target_position 能力");
    assert_eq!(write_target.inputs.len(), 1);
    assert_eq!(write_target.inputs[0].id, "value");
    assert_eq!(write_target.inputs[0].range.map(|r| r.max), Some(150.0));
    assert!(matches!(
        &write_target.implementation,
        CapabilityImpl::ModbusWrite { register, .. } if *register == 40010
    ));
    assert_eq!(write_target.safety.level, SafetyLevel::Low);

    // 验证第二个生成的能力
    let write_servo = caps
        .iter()
        .find(|c| c.id.contains("servo_enable"))
        .expect("应有 servo_enable 能力");
    assert!(matches!(
        &write_servo.implementation,
        CapabilityImpl::ModbusWrite { register, .. } if *register == 40200
    ));

    // 3. 验证生成的能力通过校验
    for cap in &caps {
        cap.validate()
            .unwrap_or_else(|e| panic!("生成的能力 {} 应通过校验: {e}", cap.id));
    }

    // 4. 持久化生成的能力
    let store = Store::open_unpersisted();
    for cap in &caps {
        let spec_json = serde_json::to_value(cap).expect("序列化应成功");
        store
            .save_capability(
                &cap.id,
                &cap.device_id,
                &format!("自动生成: {}", cap.id),
                Some("从设备信号自动生成"),
                &spec_json,
            )
            .expect("保存应成功");
    }

    // 5. 按设备过滤查询
    let list = store
        .list_capabilities(Some("hydraulic_press_1"))
        .expect("查询应成功");
    assert_eq!(list.len(), 2);

    let list_other = store
        .list_capabilities(Some("other_device"))
        .expect("查询应成功");
    assert!(list_other.is_empty());
}

#[test]
fn capability_验证_拒绝非法定义() {
    // 自引用 fallback
    let spec = CapabilitySpec {
        id: "cap.a".to_owned(),
        device_id: "dev".to_owned(),
        description: String::new(),
        inputs: vec![],
        outputs: vec![],
        preconditions: vec![],
        effects: vec![],
        implementation: CapabilityImpl::Script {
            content: "ok".to_owned(),
        },
        fallback: vec!["cap.a".to_owned()],
        safety: nazh_dsl_core::SafetyConstraints {
            level: SafetyLevel::Low,
            requires_approval: false,
            max_execution_time: None,
        },
    };
    assert!(spec.validate().is_err());

    // 括号不匹配的 precondition
    let spec2 = CapabilitySpec {
        id: "cap.b".to_owned(),
        device_id: "dev".to_owned(),
        description: String::new(),
        inputs: vec![],
        outputs: vec![],
        preconditions: vec!["(pressure > 34".to_owned()],
        effects: vec![],
        implementation: CapabilityImpl::Script {
            content: "ok".to_owned(),
        },
        fallback: vec![],
        safety: nazh_dsl_core::SafetyConstraints {
            level: SafetyLevel::Low,
            requires_approval: false,
            max_execution_time: None,
        },
    };
    assert!(spec2.validate().is_err());
}

#[test]
fn capability_版本和来源追溯() {
    let store = Store::open_unpersisted();

    // 保存多个版本
    store
        .save_capability("cap.v", "dev", "V", None, &json!({"step": 1}))
        .expect("v1 应成功");
    store
        .save_capability("cap.v", "dev", "V 更新", None, &json!({"step": 2}))
        .expect("v2 应成功");

    // 版本列表
    let versions = store
        .list_capability_versions("cap.v")
        .expect("版本列表应成功");
    assert_eq!(versions.len(), 2);
    assert_eq!(versions[0].version, 2);

    // 加载特定版本
    let v1 = store
        .load_capability_version("cap.v", 1)
        .expect("加载版本应成功")
        .expect("版本 1 应存在");
    assert_eq!(v1.spec_json["step"], 1);

    // 来源追溯
    let sources = vec![store::CapabilitySource {
        field_path: "inputs[0].range".to_owned(),
        source_text: "量程 0-150 mm".to_owned(),
        confidence: 0.95,
    }];
    store
        .save_capability_sources("cap.v", &sources)
        .expect("保存来源应成功");
    let loaded_sources = store
        .load_capability_sources("cap.v")
        .expect("加载来源应成功");
    assert_eq!(loaded_sources.len(), 1);
    assert_eq!(loaded_sources[0].field_path, "inputs[0].range");
}
