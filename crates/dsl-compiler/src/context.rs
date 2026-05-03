//! 编译上下文：持有已解析的设备与能力资产快照，提供引用校验。

use std::collections::HashMap;

use nazh_dsl_core::device::DeviceSpec;
use nazh_dsl_core::workflow::{ActionSpec, ActionTarget, WorkflowSpec};

use crate::error::CompileError;

/// 编译上下文：持有已解析的设备与能力资产快照。
///
/// 编译前需通过 [`CompilerContext::validate_references`] 校验 `WorkflowSpec`
/// 中所有设备/能力引用是否可解析。
pub struct CompilerContext {
    /// 设备资产快照（key = device id）。
    pub devices: HashMap<String, DeviceSpec>,
    /// 能力资产快照（key = capability id）。
    pub capabilities: HashMap<String, nazh_dsl_core::capability::CapabilitySpec>,
}

impl CompilerContext {
    /// 从设备和能力列表构建编译上下文。
    pub fn new(
        devices: Vec<DeviceSpec>,
        capabilities: Vec<nazh_dsl_core::capability::CapabilitySpec>,
    ) -> Self {
        Self {
            devices: devices.into_iter().map(|d| (d.id.clone(), d)).collect(),
            capabilities: capabilities
                .into_iter()
                .map(|c| (c.id.clone(), c))
                .collect(),
        }
    }

    /// 校验 `WorkflowSpec` 中所有引用是否可解析。
    ///
    /// 检查项：
    /// 1. `spec.devices` 中每个设备 ID 存在
    /// 2. 每个 `ActionTarget::Capability(id)` 存在
    /// 3. 每个能力的 `device_id` 存在
    ///
    /// 收集所有错误后一次性报告（不 fail-fast）。
    pub fn validate_references(&self, spec: &WorkflowSpec) -> Result<(), CompileError> {
        let mut missing = Vec::new();

        // 校验设备引用
        for device_id in &spec.devices {
            if !self.devices.contains_key(device_id) {
                missing.push(format!("设备 `{device_id}` 未在资产快照中找到"));
            }
        }

        // 收集所有 action 中的 capability 引用
        let mut capability_refs: Vec<&str> = Vec::new();
        for state in spec.states.values() {
            collect_capability_refs(&state.entry, &mut capability_refs);
            collect_capability_refs(&state.exit, &mut capability_refs);
        }
        for trans in &spec.transitions {
            if let Some(action) = &trans.action
                && let ActionTarget::Capability(id) = &action.target
            {
                capability_refs.push(id);
            }
        }

        for cap_id in &capability_refs {
            if let Some(cap) = self.capabilities.get(*cap_id) {
                // 校验能力的 device_id 存在
                if !self.devices.contains_key(&cap.device_id) {
                    missing.push(format!(
                        "能力 `{cap_id}` 引用的设备 `{}` 未在资产快照中找到",
                        cap.device_id
                    ));
                }
            } else {
                missing.push(format!("能力 `{cap_id}` 未在资产快照中找到"));
            }
        }

        if missing.is_empty() {
            Ok(())
        } else {
            Err(CompileError::Reference {
                detail: missing.join("；"),
            })
        }
    }

    /// 根据 device id 查找连接 id（用于生成 `capabilityCall` 节点的 `connection_id`）。
    pub fn connection_id_for_device(&self, device_id: &str) -> Option<&str> {
        self.devices
            .get(device_id)
            .map(|d| d.connection.id.as_str())
    }
}

/// 从 action 列表中提取所有 Capability 引用 ID。
fn collect_capability_refs<'a>(actions: &'a [ActionSpec], out: &mut Vec<&'a str>) {
    for action in actions {
        if let ActionTarget::Capability(id) = &action.target {
            out.push(id);
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use nazh_dsl_core::capability::{
        CapabilityImpl, CapabilitySpec, SafetyConstraints, SafetyLevel,
    };
    use nazh_dsl_core::device::{ConnectionRef, DeviceSpec};
    use nazh_dsl_core::workflow::WorkflowSpec;

    fn sample_device(id: &str) -> DeviceSpec {
        DeviceSpec {
            id: id.to_owned(),
            device_type: "test".to_owned(),
            manufacturer: None,
            model: None,
            connection: ConnectionRef {
                connection_type: "modbus-tcp".to_owned(),
                id: format!("{id}_conn"),
                unit: Some(1),
            },
            signals: vec![],
            alarms: vec![],
        }
    }

    fn sample_capability(id: &str, device_id: &str) -> CapabilitySpec {
        CapabilitySpec {
            id: id.to_owned(),
            device_id: device_id.to_owned(),
            description: String::new(),
            inputs: vec![],
            outputs: vec![],
            preconditions: vec![],
            effects: vec![],
            implementation: CapabilityImpl::Script {
                content: "pass".to_owned(),
            },
            fallback: vec![],
            safety: SafetyConstraints {
                level: SafetyLevel::Low,
                requires_approval: false,
                max_execution_time: None,
            },
        }
    }

    fn minimal_spec() -> WorkflowSpec {
        let yaml = r#"
id: test
version: "1.0.0"
devices:
  - dev1
states:
  idle:
    entry:
      - capability: cap1
transitions:
  - from: idle
    to: idle
    when: "true"
"#;
        serde_yaml::from_str(yaml).unwrap()
    }

    #[test]
    fn 合法引用通过校验() {
        let ctx = CompilerContext::new(
            vec![sample_device("dev1")],
            vec![sample_capability("cap1", "dev1")],
        );
        let spec = minimal_spec();
        assert!(ctx.validate_references(&spec).is_ok());
    }

    #[test]
    fn 缺失设备引用报错() {
        let ctx = CompilerContext::new(vec![], vec![]);
        let spec = minimal_spec();
        let err = ctx.validate_references(&spec).unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("设备 `dev1`"));
    }

    #[test]
    fn 缺失能力引用报错() {
        let ctx = CompilerContext::new(vec![sample_device("dev1")], vec![]);
        let spec = minimal_spec();
        let err = ctx.validate_references(&spec).unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("能力 `cap1`"));
    }

    #[test]
    fn 能力引用不存在的设备报错() {
        let ctx = CompilerContext::new(vec![], vec![sample_capability("cap1", "unknown_dev")]);
        let spec = minimal_spec();
        let err = ctx.validate_references(&spec).unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("unknown_dev"));
    }

    #[test]
    fn 一次报告所有缺失引用() {
        let ctx = CompilerContext::new(vec![], vec![]);
        let spec = minimal_spec();
        let err = ctx.validate_references(&spec).unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("设备 `dev1`"));
        assert!(msg.contains("能力 `cap1`"));
    }

    #[test]
    fn connection_id_for_device_查找成功() {
        let ctx = CompilerContext::new(vec![sample_device("dev1")], vec![]);
        assert_eq!(ctx.connection_id_for_device("dev1"), Some("dev1_conn"));
        assert_eq!(ctx.connection_id_for_device("unknown"), None);
    }
}
