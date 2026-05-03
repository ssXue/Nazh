//! Capability DSL 类型定义（RFC-0004 §7.2）。
//!
//! 将底层寄存器/信号操作封装为受约束的设备能力。

use serde::{Deserialize, Serialize};

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
    ModbusWrite { register: u16, value: String },
    MqttPublish { topic: String, payload: String },
    SerialCommand { command: String },
    Script { content: String },
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
}
