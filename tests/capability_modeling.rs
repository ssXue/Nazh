//! RFC-0004 Phase 2 集成测试：Capability DSL 解析 → 验证 → 自动生成。
//!
//! 验证完整的能力建模管道：YAML 解析 `CapabilitySpec`，静态校验，
//! 从设备信号自动生成能力。能力资产文件持久化由 Tauri 壳层工作路径 YAML 命令负责。

#![allow(clippy::expect_used)]

use nazh_dsl_core::{
    CapabilityImpl, CapabilitySpec, SafetyLevel, generate_capabilities_from_device,
    parse_capability_yaml, parse_device_yaml,
};

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
fn capability_yaml_解析_验证_和_round_trip() {
    // 1. 解析 YAML
    let spec = parse_capability_yaml(SAMPLE_CAPABILITY_YAML).expect("Capability YAML 解析应成功");
    assert_eq!(spec.id, "hydraulic_axis.move_to");
    assert_eq!(spec.device_id, "hydraulic_press_1");
    assert_eq!(spec.inputs.len(), 1);
    assert_eq!(spec.inputs[0].range.map(|r| r.max), Some(150.0));
    assert_eq!(spec.safety.level, SafetyLevel::High);

    // 2. 验证
    spec.validate().expect("合法 CapabilitySpec 校验应通过");

    // 3. YAML round-trip
    let yaml = serde_yaml::to_string(&spec).expect("YAML 序列化应成功");
    let reparsed = parse_capability_yaml(&yaml).expect("重新解析应成功");
    assert_eq!(reparsed.id, spec.id);
    assert_eq!(reparsed.device_id, spec.device_id);
    assert_eq!(reparsed.inputs.len(), spec.inputs.len());
}

#[test]
fn 从设备信号自动生成能力() {
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

    // 4. 生成结果可序列化为 YAML 文件内容
    for cap in &caps {
        let yaml = serde_yaml::to_string(cap).expect("YAML 序列化应成功");
        let reparsed = parse_capability_yaml(&yaml).expect("重新解析应成功");
        assert_eq!(reparsed.device_id, "hydraulic_press_1");
    }
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
