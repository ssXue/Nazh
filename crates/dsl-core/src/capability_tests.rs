use super::*;

#[test]
fn 完整的_capability_spec_从_yaml_解析成功() {
    let yaml = r#"
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
  - "pressure < 32"
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
    let spec: CapabilitySpec = serde_yaml::from_str(yaml).unwrap();
    assert_eq!(spec.id, "hydraulic_axis.move_to");
    assert_eq!(spec.device_id, "hydraulic_press_1");
    assert_eq!(spec.inputs.len(), 1);
    assert_eq!(spec.inputs[0].id, "position");
    assert_eq!(spec.inputs[0].range.map(|r| r.max), Some(150.0));
    assert!(spec.inputs[0].required);
    assert_eq!(spec.outputs.len(), 1);
    assert_eq!(spec.outputs[0].output_type, "bool");
    assert_eq!(spec.preconditions.len(), 3);
    assert_eq!(spec.fallback, vec!["hydraulic_axis.stop"]);
    assert_eq!(spec.safety.level, SafetyLevel::High);
    assert!(!spec.safety.requires_approval);
    assert_eq!(
        spec.safety.max_execution_time.map(|d| d.millis),
        Some(30_000)
    );
}

#[test]
fn capability_impl_modbus_write() {
    let yaml = r#"
type: modbus-write
register: 40010
value: "${position}"
"#;
    let imp: CapabilityImpl = serde_yaml::from_str(yaml).unwrap();
    assert_eq!(
        imp,
        CapabilityImpl::ModbusWrite {
            register: 40010,
            value: "${position}".to_owned(),
        }
    );
}

#[test]
fn capability_impl_mqtt_publish() {
    let yaml = r#"
type: mqtt-publish
topic: "factory/command"
payload: "${cmd}"
"#;
    let imp: CapabilityImpl = serde_yaml::from_str(yaml).unwrap();
    assert!(matches!(imp, CapabilityImpl::MqttPublish { .. }));
}

#[test]
fn capability_impl_serial_command() {
    let yaml = r#"
type: serial-command
command: "MOVE_TO"
"#;
    let imp: CapabilityImpl = serde_yaml::from_str(yaml).unwrap();
    assert!(matches!(imp, CapabilityImpl::SerialCommand { .. }));
}

#[test]
fn capability_impl_can_write() {
    let yaml = r#"
type: can-write
can_id: 291
data: "${value}"
is_extended: false
"#;
    let imp: CapabilityImpl = serde_yaml::from_str(yaml).unwrap();
    assert_eq!(
        imp,
        CapabilityImpl::CanWrite {
            can_id: 291,
            data: "${value}".to_owned(),
            is_extended: false,
        }
    );
}

#[test]
fn capability_impl_script() {
    let yaml = r#"
type: script
content: "let x = 1 + 2;"
"#;
    let imp: CapabilityImpl = serde_yaml::from_str(yaml).unwrap();
    assert!(matches!(imp, CapabilityImpl::Script { .. }));
}

#[test]
fn safety_level_三种变体() {
    for (yaml_str, expected) in [
        ("high", SafetyLevel::High),
        ("medium", SafetyLevel::Medium),
        ("low", SafetyLevel::Low),
    ] {
        let level: SafetyLevel = serde_yaml::from_str(yaml_str).unwrap();
        assert_eq!(level, expected);
    }
}

#[test]
fn capability_spec_yaml_round_trip() {
    let yaml = r#"
id: test.move
device_id: test_device
implementation:
  type: script
  content: "ok"
safety:
  level: low
"#;
    let spec: CapabilitySpec = serde_yaml::from_str(yaml).unwrap();
    let re_yaml = serde_yaml::to_string(&spec).unwrap();
    let back: CapabilitySpec = serde_yaml::from_str(&re_yaml).unwrap();
    assert_eq!(spec, back);
}

#[test]
fn 缺少_implementation_解析失败() {
    let yaml = r#"
id: test.move
device_id: test_device
safety:
  level: low
"#;
    assert!(serde_yaml::from_str::<CapabilitySpec>(yaml).is_err());
}

#[test]
fn 空_的_preconditions_and_fallback_默认空数组() {
    let yaml = r#"
id: test.stop
device_id: test_device
implementation:
  type: script
  content: "stop()"
safety:
  level: low
"#;
    let spec: CapabilitySpec = serde_yaml::from_str(yaml).unwrap();
    assert!(spec.preconditions.is_empty());
    assert!(spec.fallback.is_empty());
    assert!(spec.inputs.is_empty());
    assert!(spec.outputs.is_empty());
}

// ---- validate() 测试 ----

#[test]
fn validate_合法的_capability_spec() {
    let spec = CapabilitySpec {
        id: "axis.move".to_owned(),
        device_id: "press".to_owned(),
        description: String::new(),
        inputs: vec![CapabilityParam {
            id: "pos".to_owned(),
            unit: Some("mm".to_owned()),
            range: Some(Range {
                min: 0.0,
                max: 150.0,
            }),
            required: true,
        }],
        outputs: vec![],
        preconditions: vec!["ready == true".to_owned()],
        effects: vec!["state = moving".to_owned()],
        implementation: CapabilityImpl::ModbusWrite {
            register: 40010,
            value: "${pos}".to_owned(),
        },
        fallback: vec![],
        safety: SafetyConstraints {
            level: SafetyLevel::Low,
            requires_approval: false,
            max_execution_time: None,
        },
    };
    assert!(spec.validate().is_ok());
}

#[test]
fn validate_自引用_fallback_失败() {
    let spec = CapabilitySpec {
        id: "cap.a".to_owned(),
        device_id: "d".to_owned(),
        description: String::new(),
        inputs: vec![],
        outputs: vec![],
        preconditions: vec![],
        effects: vec![],
        implementation: CapabilityImpl::Script {
            content: "ok".to_owned(),
        },
        fallback: vec!["cap.a".to_owned()],
        safety: SafetyConstraints {
            level: SafetyLevel::Low,
            requires_approval: false,
            max_execution_time: None,
        },
    };
    assert!(spec.validate().is_err());
}

#[test]
fn validate_括号不匹配的_precondition_失败() {
    let spec = CapabilitySpec {
        id: "c".to_owned(),
        device_id: "d".to_owned(),
        description: String::new(),
        inputs: vec![],
        outputs: vec![],
        preconditions: vec!["(pressure > 34".to_owned()],
        effects: vec![],
        implementation: CapabilityImpl::Script {
            content: "ok".to_owned(),
        },
        fallback: vec![],
        safety: SafetyConstraints {
            level: SafetyLevel::Low,
            requires_approval: false,
            max_execution_time: None,
        },
    };
    assert!(spec.validate().is_err());
}

#[test]
fn validate_required_input_必须声明量程() {
    let spec = CapabilitySpec {
        id: "axis.move".to_owned(),
        device_id: "press".to_owned(),
        description: String::new(),
        inputs: vec![CapabilityParam {
            id: "position".to_owned(),
            unit: Some("mm".to_owned()),
            range: None,
            required: true,
        }],
        outputs: vec![],
        preconditions: vec![],
        effects: vec![],
        implementation: CapabilityImpl::ModbusWrite {
            register: 40010,
            value: "${position}".to_owned(),
        },
        fallback: vec![],
        safety: SafetyConstraints {
            level: SafetyLevel::Low,
            requires_approval: false,
            max_execution_time: None,
        },
    };

    let err = spec.validate().unwrap_err();
    assert!(err.to_string().contains("inputs.position.range"));
}

#[test]
fn validate_拒绝重复_input_output_id() {
    let spec = CapabilitySpec {
        id: "axis.move".to_owned(),
        device_id: "press".to_owned(),
        description: String::new(),
        inputs: vec![
            CapabilityParam {
                id: "value".to_owned(),
                unit: None,
                range: Some(Range { min: 0.0, max: 1.0 }),
                required: true,
            },
            CapabilityParam {
                id: "value".to_owned(),
                unit: None,
                range: Some(Range { min: 0.0, max: 1.0 }),
                required: true,
            },
        ],
        outputs: vec![
            CapabilityOutput {
                id: "done".to_owned(),
                output_type: "bool".to_owned(),
            },
            CapabilityOutput {
                id: "done".to_owned(),
                output_type: "bool".to_owned(),
            },
        ],
        preconditions: vec![],
        effects: vec![],
        implementation: CapabilityImpl::ModbusWrite {
            register: 40010,
            value: "${value}".to_owned(),
        },
        fallback: vec![],
        safety: SafetyConstraints {
            level: SafetyLevel::Low,
            requires_approval: false,
            max_execution_time: None,
        },
    };

    let err = spec.validate().unwrap_err();
    assert!(err.to_string().contains("重复"));
}

#[test]
fn validate_拒绝未声明模板变量() {
    let spec = CapabilitySpec {
        id: "axis.move".to_owned(),
        device_id: "press".to_owned(),
        description: String::new(),
        inputs: vec![CapabilityParam {
            id: "position".to_owned(),
            unit: Some("mm".to_owned()),
            range: Some(Range {
                min: 0.0,
                max: 150.0,
            }),
            required: true,
        }],
        outputs: vec![],
        preconditions: vec![],
        effects: vec![],
        implementation: CapabilityImpl::ModbusWrite {
            register: 40010,
            value: "${target_position}".to_owned(),
        },
        fallback: vec![],
        safety: SafetyConstraints {
            level: SafetyLevel::Low,
            requires_approval: false,
            max_execution_time: None,
        },
    };

    let err = spec.validate().unwrap_err();
    assert!(err.to_string().contains("target_position"));
}

// ---- generate_capabilities_from_device() 测试 ----

#[test]
fn 从设备生成能力_只取写信号() {
    use crate::device::{DataType, SignalSpec};
    let device = DeviceSpec {
        id: "press_1".to_owned(),
        device_type: "hydraulic_press".to_owned(),
        manufacturer: None,
        model: None,
        connection: None,
        network_group: None,
        signals: vec![
            SignalSpec {
                id: "pressure".to_owned(),
                signal_type: SignalType::AnalogInput,
                unit: Some("MPa".to_owned()),
                range: Some(Range {
                    min: 0.0,
                    max: 35.0,
                }),
                source: SignalSource::Register {
                    register: 40001,
                    access: AccessMode::Read,
                    data_type: DataType::Float32,
                    bit: None,
                },
                scale: None,
            },
            SignalSpec {
                id: "target_pos".to_owned(),
                signal_type: SignalType::AnalogOutput,
                unit: Some("mm".to_owned()),
                range: Some(Range {
                    min: 0.0,
                    max: 150.0,
                }),
                source: SignalSource::Topic {
                    topic: "press/target_pos".to_owned(),
                },
                scale: None,
            },
        ],
        alarms: vec![],
    };

    let caps = generate_capabilities_from_device(&device);
    assert_eq!(caps.len(), 1);
    assert_eq!(caps[0].id, "press_1.write_target_pos");
    assert_eq!(caps[0].inputs.len(), 1);
    assert_eq!(caps[0].inputs[0].id, "value");
    assert_eq!(caps[0].inputs[0].range.map(|r| r.max), Some(150.0));
    assert!(matches!(
        caps[0].implementation,
        CapabilityImpl::MqttPublish { .. }
    ));
    assert_eq!(caps[0].safety.level, SafetyLevel::Low);
}

#[test]
fn 从设备生成能力_无写信号返回空() {
    use crate::device::SignalSpec;
    let device = DeviceSpec {
        id: "sensor".to_owned(),
        device_type: "temp_sensor".to_owned(),
        manufacturer: None,
        model: None,
        connection: None,
        network_group: None,
        signals: vec![SignalSpec {
            id: "temp".to_owned(),
            signal_type: SignalType::AnalogInput,
            unit: Some("C".to_owned()),
            range: None,
            source: SignalSource::Topic {
                topic: "sensors/temp".to_owned(),
            },
            scale: None,
        }],
        alarms: vec![],
    };

    let caps = generate_capabilities_from_device(&device);
    assert!(caps.is_empty());
}

#[test]
fn 从_can_output_信号生成写能力_因编码语义无法无损表达而失败() {
    use crate::device::{ByteOrder, DataType, SignalSpec};
    let device = DeviceSpec {
        id: "drive_1".to_owned(),
        device_type: "servo_drive".to_owned(),
        manufacturer: None,
        model: None,
        connection: None,
        network_group: None,
        signals: vec![SignalSpec {
            id: "target_speed".to_owned(),
            signal_type: SignalType::AnalogOutput,
            unit: Some("rpm".to_owned()),
            range: None,
            source: SignalSource::CanFrame {
                can_id: 0x123,
                is_extended: false,
                byte_offset: 0,
                byte_length: 2,
                data_type: DataType::U16,
                byte_order: ByteOrder::BigEndian,
            },
            scale: None,
        }],
        alarms: vec![],
    };

    let err = try_generate_capabilities_from_device(&device).unwrap_err();
    let msg = err.to_string();
    assert!(msg.contains("target_speed"));
    assert!(msg.contains("CAN"));
    assert!(msg.contains("byte_offset"));
}
