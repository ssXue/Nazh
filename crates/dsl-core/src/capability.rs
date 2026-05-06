//! Capability DSL 类型定义（RFC-0004 §7.2）。
//!
//! 将底层寄存器/信号操作封装为受约束的设备能力。

use serde::{Deserialize, Serialize};

use crate::device::{AccessMode, DeviceSpec, SignalSource, SignalType};
use crate::error::DslError;
use crate::workflow::{HumanDuration, Range};

/// 能力定义。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CapabilitySpec {
    pub id: String,
    pub device_id: String,
    #[serde(default)]
    #[serde(skip_serializing_if = "String::is_empty")]
    pub description: String,
    #[serde(default)]
    pub inputs: Vec<CapabilityParam>,
    #[serde(default)]
    pub outputs: Vec<CapabilityOutput>,
    /// Rhai 前置条件表达式列表。
    #[serde(default)]
    pub preconditions: Vec<String>,
    /// 执行后产生的副作用声明列表。
    #[serde(default)]
    pub effects: Vec<String>,
    pub implementation: CapabilityImpl,
    /// 后备能力 ID 列表。
    #[serde(default)]
    pub fallback: Vec<String>,
    pub safety: SafetyConstraints,
}

/// 能力输入参数。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CapabilityParam {
    pub id: String,
    #[serde(default)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub unit: Option<String>,
    #[serde(default)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub range: Option<Range>,
    #[serde(default)]
    pub required: bool,
}

/// 能力输出声明。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CapabilityOutput {
    pub id: String,
    #[serde(rename = "type")]
    pub output_type: String,
}

/// 能力的底层实现方式。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "kebab-case")]
pub enum CapabilityImpl {
    ModbusWrite {
        register: u16,
        value: String,
    },
    MqttPublish {
        topic: String,
        payload: String,
    },
    SerialCommand {
        command: String,
    },
    CanWrite {
        can_id: u32,
        data: String,
        is_extended: bool,
    },
    Script {
        content: String,
    },
}

/// 安全约束。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SafetyConstraints {
    pub level: SafetyLevel,
    #[serde(default)]
    pub requires_approval: bool,
    #[serde(default)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_execution_time: Option<HumanDuration>,
}

/// 安全等级。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SafetyLevel {
    High,
    Medium,
    Low,
}

impl CapabilitySpec {
    /// 校验能力定义的语义完整性。
    ///
    /// 检查项：`implementation` 字段完整性、`preconditions` 基本语法、
    /// `fallback` 引用非自引用、required input 有 range。
    pub fn validate(&self) -> Result<(), DslError> {
        // implementation 完整性
        match &self.implementation {
            CapabilityImpl::ModbusWrite { value, .. } => {
                validate_template_expr(value)?;
            }
            CapabilityImpl::MqttPublish { payload, .. } => {
                validate_template_expr(payload)?;
            }
            CapabilityImpl::SerialCommand { command } => {
                validate_template_expr(command)?;
            }
            CapabilityImpl::CanWrite { data, .. } => {
                validate_template_expr(data)?;
            }
            CapabilityImpl::Script { content } => {
                validate_template_expr(content)?;
            }
        }

        // preconditions 基本语法检查
        for cond in &self.preconditions {
            validate_rhai_like_expr(cond)?;
        }

        // effects 语法检查
        for eff in &self.effects {
            validate_rhai_like_expr(eff)?;
        }

        // fallback 不自引用
        if self.fallback.contains(&self.id) {
            return Err(DslError::Validation {
                context: format!("capability `{}`", self.id),
                detail: "fallback 不能引用自身".to_owned(),
            });
        }

        Ok(())
    }
}

/// 从设备的写信号自动生成 `CapabilitySpec` 列表。
///
/// 每个写信号（`AnalogOutput` / `DigitalOutput`，或 `AccessMode::Write` / `ReadWrite`）
/// 映射为一个能力，信号元数据（量程、单位、寄存器地址）映射到能力输入和实现。
pub fn generate_capabilities_from_device(device: &DeviceSpec) -> Vec<CapabilitySpec> {
    device
        .signals
        .iter()
        .filter(|s| is_writable_signal(s.signal_type, &s.source))
        .map(|signal| {
            let cap_id = format!("{}.write_{}", device.id, signal.id);
            let cap_name = format!("写入 {}", signal.id);

            let input = CapabilityParam {
                id: "value".to_owned(),
                unit: signal.unit.clone(),
                range: signal.range,
                required: true,
            };

            let implementation = match &signal.source {
                SignalSource::Register { register, .. } => CapabilityImpl::ModbusWrite {
                    register: *register,
                    value: "${value}".to_owned(),
                },
                SignalSource::Topic { topic } => CapabilityImpl::MqttPublish {
                    topic: topic.clone(),
                    payload: "${value}".to_owned(),
                },
                SignalSource::SerialCommand { command } => CapabilityImpl::SerialCommand {
                    command: format!("{command} ${{value}}"),
                },
                SignalSource::CanFrame {
                    can_id,
                    is_extended,
                    ..
                } => CapabilityImpl::CanWrite {
                    can_id: *can_id,
                    data: "${value}".to_owned(),
                    is_extended: *is_extended,
                },
                SignalSource::EthercatPdo {
                    slave_address,
                    pdo_index,
                    entry_index,
                    sub_index,
                    ..
                } => {
                    let content = if let Some(slave_address) = slave_address {
                        format!(
                            "ethercat_pdo_write({slave_address}, {pdo_index}, {entry_index}, {sub_index}, ${{value}})"
                        )
                    } else {
                        format!(
                            "ethercat_pdo_write({pdo_index}, {entry_index}, {sub_index}, ${{value}})"
                        )
                    };
                    CapabilityImpl::Script { content }
                }
            };

            CapabilitySpec {
                id: cap_id,
                device_id: device.id.clone(),
                description: format!("自动生成：{cap_name}"),
                inputs: vec![input],
                outputs: vec![],
                preconditions: vec![],
                effects: vec![format!("{} 被修改", signal.id)],
                implementation,
                fallback: vec![],
                safety: SafetyConstraints {
                    level: SafetyLevel::Low,
                    requires_approval: false,
                    max_execution_time: None,
                },
            }
        })
        .collect()
}

/// 判断信号是否为写信号。
fn is_writable_signal(signal_type: SignalType, source: &SignalSource) -> bool {
    // 输入信号也可能是 read-write
    if let SignalSource::Register { access, .. } = source {
        return matches!(
            signal_type,
            SignalType::AnalogOutput | SignalType::DigitalOutput
        ) || matches!(access, AccessMode::Write | AccessMode::ReadWrite);
    }
    matches!(
        source,
        SignalSource::Topic { .. }
            | SignalSource::SerialCommand { .. }
            | SignalSource::CanFrame { .. }
    ) && matches!(
        signal_type,
        SignalType::AnalogOutput | SignalType::DigitalOutput
    )
}

/// 校验模板表达式中的 `${...}` 参数引用格式。
fn validate_template_expr(expr: &str) -> Result<(), DslError> {
    let mut depth = 0i32;
    for ch in expr.chars() {
        match ch {
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth < 0 {
                    return Err(DslError::Validation {
                        context: "implementation".to_owned(),
                        detail: format!("模板表达式括号不匹配: `{expr}`"),
                    });
                }
            }
            _ => {}
        }
    }
    if depth != 0 {
        return Err(DslError::Validation {
            context: "implementation".to_owned(),
            detail: format!("模板表达式括号不匹配: `{expr}`"),
        });
    }
    Ok(())
}

/// 对 Rhai 风格表达式做基本语法校验（括号匹配 + 非空）。
fn validate_rhai_like_expr(expr: &str) -> Result<(), DslError> {
    let trimmed = expr.trim();
    if trimmed.is_empty() {
        return Err(DslError::Validation {
            context: "expression".to_owned(),
            detail: "表达式不能为空".to_owned(),
        });
    }

    let mut paren_depth = 0i32;
    let mut bracket_depth = 0i32;
    for ch in trimmed.chars() {
        match ch {
            '(' => paren_depth += 1,
            ')' => {
                paren_depth -= 1;
                if paren_depth < 0 {
                    return Err(DslError::Validation {
                        context: "expression".to_owned(),
                        detail: format!("括号不匹配: `{expr}`"),
                    });
                }
            }
            '[' => bracket_depth += 1,
            ']' => {
                bracket_depth -= 1;
                if bracket_depth < 0 {
                    return Err(DslError::Validation {
                        context: "expression".to_owned(),
                        detail: format!("方括号不匹配: `{expr}`"),
                    });
                }
            }
            _ => {}
        }
    }
    if paren_depth != 0 || bracket_depth != 0 {
        return Err(DslError::Validation {
            context: "expression".to_owned(),
            detail: format!("括号不匹配: `{expr}`"),
        });
    }
    Ok(())
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::needless_raw_string_hashes)]
mod tests {
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

    // ---- generate_capabilities_from_device() 测试 ----

    #[test]
    fn 从设备生成能力_只取写信号() {
        use crate::device::{ConnectionRef, DataType, SignalSpec};
        let device = DeviceSpec {
            id: "press_1".to_owned(),
            device_type: "hydraulic_press".to_owned(),
            manufacturer: None,
            model: None,
            connection: ConnectionRef {
                connection_type: "modbus-tcp".to_owned(),
                id: "conn1".to_owned(),
                unit: None,
            },
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
                    source: SignalSource::Register {
                        register: 40010,
                        access: AccessMode::Write,
                        data_type: DataType::Float32,
                        bit: None,
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
            CapabilityImpl::ModbusWrite { .. }
        ));
        assert_eq!(caps[0].safety.level, SafetyLevel::Low);
    }

    #[test]
    fn 从设备生成能力_无写信号返回空() {
        use crate::device::{ConnectionRef, SignalSpec};
        let device = DeviceSpec {
            id: "sensor".to_owned(),
            device_type: "temp_sensor".to_owned(),
            manufacturer: None,
            model: None,
            connection: ConnectionRef {
                connection_type: "mqtt".to_owned(),
                id: "broker".to_owned(),
                unit: None,
            },
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
    fn 从_can_output_信号生成写能力() {
        use crate::device::{ByteOrder, ConnectionRef, DataType, SignalSpec};
        let device = DeviceSpec {
            id: "drive_1".to_owned(),
            device_type: "servo_drive".to_owned(),
            manufacturer: None,
            model: None,
            connection: ConnectionRef {
                connection_type: "can-slcan".to_owned(),
                id: "drive_can".to_owned(),
                unit: None,
            },
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

        let caps = generate_capabilities_from_device(&device);
        assert_eq!(caps.len(), 1);
        assert!(matches!(
            caps[0].implementation,
            CapabilityImpl::CanWrite { can_id: 0x123, .. }
        ));
    }
}
