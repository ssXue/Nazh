//! YAML 文本 → Spec 类型的解析入口。
//!
//! 每种 DSL 有独立的解析函数，提供统一的错误类型 [`DslError`]
//! 和人类可读的错误上下文。

use crate::capability::CapabilitySpec;
use crate::device::DeviceSpec;
use crate::error::DslError;
use crate::workflow::WorkflowSpec;

/// 从 YAML 文本解析 [`DeviceSpec`]。
///
/// # Errors
///
/// YAML 语法错误、必填字段缺失或字段类型不匹配时返回 [`DslError`]。
pub fn parse_device_yaml(yaml: &str) -> Result<DeviceSpec, DslError> {
    serde_yaml::from_str(yaml).map_err(Into::into)
}

/// 从 YAML 文本解析 [`CapabilitySpec`]。
///
/// # Errors
///
/// 同 [`parse_device_yaml`]。
pub fn parse_capability_yaml(yaml: &str) -> Result<CapabilitySpec, DslError> {
    serde_yaml::from_str(yaml).map_err(Into::into)
}

/// 从 YAML 文本解析 [`WorkflowSpec`]。
///
/// # Errors
///
/// 同 [`parse_device_yaml`]。
pub fn parse_workflow_yaml(yaml: &str) -> Result<WorkflowSpec, DslError> {
    serde_yaml::from_str(yaml).map_err(Into::into)
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::needless_raw_string_hashes)]
mod tests {
    use super::*;

    #[test]
    fn parse_device_yaml_空字符串_失败() {
        assert!(parse_device_yaml("").is_err());
    }

    #[test]
    fn parse_device_yaml_非法_yaml_失败() {
        assert!(parse_device_yaml(":\n  :").is_err());
    }

    #[test]
    fn parse_capability_yaml_空字符串_失败() {
        assert!(parse_capability_yaml("").is_err());
    }

    #[test]
    fn parse_workflow_yaml_空字符串_失败() {
        assert!(parse_workflow_yaml("").is_err());
    }

    #[test]
    fn parse_device_yaml_json_round_trip() {
        let yaml = r#"
id: test_dev
type: sensor
connection:
  type: mqtt
  id: broker
"#;
        let spec = parse_device_yaml(yaml).unwrap();
        let json = serde_json::to_string(&spec).unwrap();
        let from_json: DeviceSpec = serde_json::from_str(&json).unwrap();
        assert_eq!(spec, from_json);
    }

    #[test]
    fn parse_capability_yaml_json_round_trip() {
        let yaml = r#"
id: test.cap
device_id: dev1
implementation:
  type: script
  content: "ok"
safety:
  level: low
"#;
        let spec = parse_capability_yaml(yaml).unwrap();
        let json = serde_json::to_string(&spec).unwrap();
        let from_json: CapabilitySpec = serde_json::from_str(&json).unwrap();
        assert_eq!(spec, from_json);
    }

    #[test]
    fn parse_workflow_yaml_json_round_trip() {
        let yaml = r#"
id: wf1
version: "1.0"
states:
  idle:
"#;
        let spec = parse_workflow_yaml(yaml).unwrap();
        let json = serde_json::to_string(&spec).unwrap();
        let from_json: WorkflowSpec = serde_json::from_str(&json).unwrap();
        assert_eq!(spec, from_json);
    }

    #[test]
    fn dsl_error_from_serde_yaml_error_包含上下文() {
        let result = parse_device_yaml("invalid: {{{");
        let err = result.unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("YAML 解析失败"), "错误消息应为中文: {msg}");
    }
}
