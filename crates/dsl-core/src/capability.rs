//! Capability DSL 类型定义（RFC-0004 §7.2）。
//!
//! 将底层寄存器/信号操作封装为受约束的设备能力。

use std::collections::HashSet;

use serde::{Deserialize, Serialize};

use crate::device::{AccessMode, ByteOrder, DataType, DeviceSpec, SignalSource, SignalType};
use crate::error::DslError;
use crate::workflow::{HumanDuration, Range};

/// 单个信号的能力推断结果（RFC-0006 Phase 2）。
#[derive(Debug, Clone)]
pub enum CapabilityInference {
    /// 成功自动生成。
    Generated {
        capability: Box<CapabilitySpec>,
        source_signal_id: String,
    },
    /// 无法自动生成，返回编码上下文供 AI 接手。
    Unsupported {
        signal_id: String,
        reason: String,
        encoding_info: serde_json::Value,
    },
}

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
        #[serde(default)]
        data_type: DataType,
        #[serde(default)]
        #[serde(skip_serializing_if = "Option::is_none")]
        bit: Option<u8>,
        #[serde(default)]
        byte_order: ByteOrder,
        #[serde(default)]
        #[serde(skip_serializing_if = "Option::is_none")]
        scale: Option<String>,
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
        is_extended: bool,
        #[serde(default)]
        byte_offset: u8,
        #[serde(default)]
        byte_length: u8,
        #[serde(default)]
        data_type: DataType,
        #[serde(default)]
        byte_order: ByteOrder,
        #[serde(default)]
        #[serde(skip_serializing_if = "Option::is_none")]
        scale: Option<String>,
        data: String,
    },
    EthercatPdoWrite {
        #[serde(default)]
        #[serde(skip_serializing_if = "Option::is_none")]
        slave_address: Option<u16>,
        pdo_index: u16,
        entry_index: u16,
        sub_index: u8,
        bit_len: u16,
        #[serde(default)]
        #[serde(skip_serializing_if = "Option::is_none")]
        data_type: Option<String>,
        #[serde(default)]
        byte_order: ByteOrder,
        #[serde(default)]
        #[serde(skip_serializing_if = "Option::is_none")]
        scale: Option<String>,
        value: String,
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
        let input_ids = validate_unique_inputs(self)?;
        validate_unique_outputs(self)?;
        validate_required_input_ranges(self)?;

        // implementation 完整性
        match &self.implementation {
            CapabilityImpl::ModbusWrite { value, .. } => {
                validate_template_expr(self, "implementation.value", value, &input_ids)?;
            }
            CapabilityImpl::MqttPublish { payload, .. } => {
                validate_template_expr(self, "implementation.payload", payload, &input_ids)?;
            }
            CapabilityImpl::SerialCommand { command } => {
                validate_template_expr(self, "implementation.command", command, &input_ids)?;
            }
            CapabilityImpl::CanWrite { data, .. } => {
                validate_template_expr(self, "implementation.data", data, &input_ids)?;
            }
            CapabilityImpl::Script { content } => {
                validate_template_expr(self, "implementation.content", content, &input_ids)?;
            }
            CapabilityImpl::EthercatPdoWrite { .. } => {
                // EthercatPdoWrite does not use template expressions
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
        validate_fallback_constraints(self)?;

        Ok(())
    }
}

/// 从设备的写信号自动生成 `CapabilitySpec` 列表。
///
/// 每个写信号（`AnalogOutput` / `DigitalOutput`，或 `AccessMode::Write` / `ReadWrite`）
/// 映射为一个能力，信号元数据（量程、单位、寄存器地址）映射到能力输入和实现。
pub fn generate_capabilities_from_device(device: &DeviceSpec) -> Vec<CapabilitySpec> {
    try_generate_capabilities_from_device(device)
}

/// ADR-0028 后所有 `SignalSource` 都可自动生成能力，此函数不再失败。
pub fn try_generate_capabilities_from_device(device: &DeviceSpec) -> Vec<CapabilitySpec> {
    device
        .signals
        .iter()
        .filter(|s| is_writable_signal(s.signal_type, &s.source))
        .map(|signal| build_capability_from_signal(device, signal))
        .collect()
}

/// 从设备信号推断能力列表（RFC-0006 Phase 2）。
///
/// 对每个写信号尝试生成能力。自动生成成功的返回 `Generated`，
/// 无法生成的（CAN / Modbus / `EtherCAT` 写入）返回 `Unsupported` + 编码上下文，
///
/// `signal_ids` 为 `None` 时处理所有写信号；为 `Some(ids)` 时只处理指定信号。
pub fn infer_capabilities_from_signals(
    device: &DeviceSpec,
    signal_ids: Option<&[String]>,
) -> Vec<CapabilityInference> {
    device
        .signals
        .iter()
        .filter(|s| is_writable_signal(s.signal_type, &s.source))
        .filter(|s| signal_ids.is_none_or(|ids| ids.contains(&s.id)))
        .map(|signal| infer_single_capability(device, signal))
        .collect()
}

/// 对单个信号推断能力。
fn infer_single_capability(
    device: &DeviceSpec,
    signal: &crate::device::SignalSpec,
) -> CapabilityInference {
    // ADR-0028: build_capability_from_signal 对所有 SignalSource 均成功
    let capability = build_capability_from_signal(device, signal);
    CapabilityInference::Generated {
        capability: Box::new(capability),
        source_signal_id: signal.id.clone(),
    }
}

fn build_capability_from_signal(
    device: &DeviceSpec,
    signal: &crate::device::SignalSpec,
) -> CapabilitySpec {
    let cap_id = format!("{}.write_{}", device.id, signal.id);
    let cap_name = format!("写入 {}", signal.id);

    let input = CapabilityParam {
        id: "value".to_owned(),
        unit: signal.unit.clone(),
        range: signal.range,
        required: true,
    };

    let implementation = match &signal.source {
        SignalSource::Register {
            data_type,
            bit,
            access: _,
            register,
        } => CapabilityImpl::ModbusWrite {
            register: *register,
            data_type: *data_type,
            bit: *bit,
            byte_order: ByteOrder::BigEndian,
            scale: signal.scale.clone(),
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
            byte_offset,
            byte_length,
            data_type,
            byte_order,
        } => CapabilityImpl::CanWrite {
            can_id: *can_id,
            is_extended: *is_extended,
            byte_offset: *byte_offset,
            byte_length: *byte_length,
            data_type: *data_type,
            byte_order: *byte_order,
            scale: signal.scale.clone(),
            data: "${value}".to_owned(),
        },
        SignalSource::EthercatPdo {
            slave_address,
            pdo_index,
            entry_index,
            sub_index,
            bit_len,
            data_type,
            pdo_name: _,
            entry_name: _,
        } => CapabilityImpl::EthercatPdoWrite {
            slave_address: *slave_address,
            pdo_index: *pdo_index,
            entry_index: *entry_index,
            sub_index: *sub_index,
            bit_len: *bit_len,
            data_type: data_type.clone(),
            byte_order: ByteOrder::LittleEndian,
            scale: signal.scale.clone(),
            value: "${value}".to_owned(),
        },
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
fn validate_unique_inputs(spec: &CapabilitySpec) -> Result<HashSet<String>, DslError> {
    let mut ids = HashSet::new();
    for input in &spec.inputs {
        if input.id.trim().is_empty() {
            return Err(DslError::Validation {
                context: format!("capability `{}` inputs", spec.id),
                detail: "input id 不能为空".to_owned(),
            });
        }
        if !ids.insert(input.id.clone()) {
            return Err(DslError::Validation {
                context: format!("capability `{}` inputs", spec.id),
                detail: format!("重复 input id `{}`", input.id),
            });
        }
    }
    Ok(ids)
}

fn validate_unique_outputs(spec: &CapabilitySpec) -> Result<(), DslError> {
    let mut ids = HashSet::new();
    for output in &spec.outputs {
        if output.id.trim().is_empty() {
            return Err(DslError::Validation {
                context: format!("capability `{}` outputs", spec.id),
                detail: "output id 不能为空".to_owned(),
            });
        }
        if !ids.insert(output.id.clone()) {
            return Err(DslError::Validation {
                context: format!("capability `{}` outputs", spec.id),
                detail: format!("重复 output id `{}`", output.id),
            });
        }
    }
    Ok(())
}

fn validate_required_input_ranges(spec: &CapabilitySpec) -> Result<(), DslError> {
    for input in &spec.inputs {
        if input.required && input.range.is_none() {
            return Err(DslError::Validation {
                context: format!("capability `{}` inputs.{}.range", spec.id, input.id),
                detail: "required input 必须声明 range，避免运行时无法做量程保护".to_owned(),
            });
        }
    }
    Ok(())
}

fn validate_fallback_constraints(spec: &CapabilitySpec) -> Result<(), DslError> {
    let mut ids = HashSet::new();
    for fallback in &spec.fallback {
        if fallback.trim().is_empty() {
            return Err(DslError::Validation {
                context: format!("capability `{}` fallback", spec.id),
                detail: "fallback id 不能为空".to_owned(),
            });
        }
        if !ids.insert(fallback.clone()) {
            return Err(DslError::Validation {
                context: format!("capability `{}` fallback", spec.id),
                detail: format!("重复 fallback id `{fallback}`"),
            });
        }
    }
    Ok(())
}

fn validate_template_expr(
    spec: &CapabilitySpec,
    field_path: &str,
    expr: &str,
    input_ids: &HashSet<String>,
) -> Result<(), DslError> {
    let mut depth = 0i32;
    for ch in expr.chars() {
        match ch {
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth < 0 {
                    return Err(DslError::Validation {
                        context: format!("capability `{}` {field_path}", spec.id),
                        detail: format!("模板表达式括号不匹配: `{expr}`"),
                    });
                }
            }
            _ => {}
        }
    }
    if depth != 0 {
        return Err(DslError::Validation {
            context: format!("capability `{}` {field_path}", spec.id),
            detail: format!("模板表达式括号不匹配: `{expr}`"),
        });
    }
    for var_name in extract_template_variables(expr) {
        if !input_ids.contains(&var_name) {
            return Err(DslError::Validation {
                context: format!("capability `{}` {field_path}", spec.id),
                detail: format!(
                    "模板变量 `{var_name}` 未在 inputs 中声明；请新增 input 或修正模板引用"
                ),
            });
        }
    }
    Ok(())
}

fn extract_template_variables(expr: &str) -> Vec<String> {
    let mut vars = Vec::new();
    let chars: Vec<char> = expr.chars().collect();
    let mut i = 0;
    while i + 2 <= chars.len() {
        if chars[i] == '$' && i + 1 < chars.len() && chars[i + 1] == '{' {
            let start = i + 2;
            let mut end = start;
            while end < chars.len() && chars[end] != '}' {
                end += 1;
            }
            if end < chars.len() {
                let name: String = chars[start..end].iter().collect();
                let trimmed = name.trim();
                if !trimmed.is_empty() {
                    vars.push(trimmed.to_owned());
                }
                i = end + 1;
                continue;
            }
        }
        i += 1;
    }
    vars
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
#[path = "capability_tests.rs"]
mod tests;
