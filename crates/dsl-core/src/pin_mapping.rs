//! SignalSpec → PinDefinition 映射（RFC-0004 Phase 1）。
//!
//! 将 Device DSL 的信号定义转换为引擎可消费的 Pin 声明，
//! 用于设备操作节点自动生成端口 schema。

use nazh_core::{EmptyPolicy, PinDefinition, PinDirection, PinKind, PinType};

use crate::device::SignalType;

/// 将 [`SignalType`] 映射为 [`PinType`]。
///
/// 映射规则：
/// - `AnalogInput` / `AnalogOutput` → `Float`（模拟量通常为浮点）
/// - `DigitalInput` / `DigitalOutput` → `Bool`（数字量通常为开关）
#[must_use]
pub fn signal_to_pin_type(signal_type: SignalType) -> PinType {
    match signal_type {
        SignalType::AnalogInput | SignalType::AnalogOutput => PinType::Float,
        SignalType::DigitalInput | SignalType::DigitalOutput => PinType::Bool,
    }
}

/// 将 [`SignalType`] 映射为 [`PinDirection`]。
///
/// 映射规则：
/// - `AnalogInput` / `DigitalInput` → `Input`（设备读入）
/// - `AnalogOutput` / `DigitalOutput` → `Output`（设备写出）
#[must_use]
pub fn signal_to_direction(signal_type: SignalType) -> PinDirection {
    match signal_type {
        SignalType::AnalogInput | SignalType::DigitalInput => PinDirection::Input,
        SignalType::AnalogOutput | SignalType::DigitalOutput => PinDirection::Output,
    }
}

/// 将信号 ID 转换为人类可读的标签。
///
/// 例如 `"hydraulic_pressure"` → `"Hydraulic Pressure"`。
#[must_use]
pub fn signal_id_to_label(id: &str) -> String {
    id.split('_')
        .map(|word| {
            let mut chars = word.chars();
            match chars.next() {
                None => String::new(),
                Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// 从信号规格列表生成 Pin 声明列表。
///
/// 每个信号生成一个 `PinDefinition`：
/// - `id` = 信号 ID
/// - `label` = 从信号 ID 派生的可读标签 + 可选单位
/// - `pin_type` = 由 `signal_to_pin_type` 映射
/// - `direction` = 由 `signal_to_direction` 映射
/// - `kind` = `Data`（设备信号是数据端口，不是控制流）
/// - `required` = false（信号为可选端口）
pub fn signals_to_pin_definitions(
    signals: &[crate::device::SignalSpec],
) -> Vec<PinDefinition> {
    signals
        .iter()
        .map(|signal| {
            let mut label = signal_id_to_label(&signal.id);
            if let Some(unit) = &signal.unit {
                label.push_str(&format!(" ({unit})"));
            }

            let mut description = None;
            if signal.range.is_some() || signal.scale.is_some() {
                let mut desc = String::new();
                if let Some(range) = &signal.range {
                    desc.push_str(&format!("量程: [{}, {}]", range.min, range.max));
                }
                if let Some(scale) = &signal.scale {
                    if !desc.is_empty() {
                        desc.push_str("; ");
                    }
                    desc.push_str(&format!("缩放: {scale}"));
                }
                description = Some(desc);
            }

            PinDefinition {
                id: signal.id.clone(),
                label,
                pin_type: signal_to_pin_type(signal.signal_type),
                direction: signal_to_direction(signal.signal_type),
                required: false,
                kind: PinKind::Data,
                description,
                empty_policy: EmptyPolicy::default(),
                block_timeout_ms: None,
                ttl_ms: None,
            }
        })
        .collect()
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use crate::device::{AccessMode, DataType, SignalSource, SignalSpec};

    fn register_source(address: u16) -> SignalSource {
        SignalSource::Register {
            register: address,
            access: AccessMode::Read,
            data_type: DataType::Float32,
            bit: None,
        }
    }

    #[test]
    fn signal_type_映射_pin_type() {
        assert_eq!(signal_to_pin_type(SignalType::AnalogInput), PinType::Float);
        assert_eq!(signal_to_pin_type(SignalType::AnalogOutput), PinType::Float);
        assert_eq!(signal_to_pin_type(SignalType::DigitalInput), PinType::Bool);
        assert_eq!(signal_to_pin_type(SignalType::DigitalOutput), PinType::Bool);
    }

    #[test]
    fn signal_type_映射_direction() {
        assert_eq!(signal_to_direction(SignalType::AnalogInput), PinDirection::Input);
        assert_eq!(signal_to_direction(SignalType::DigitalInput), PinDirection::Input);
        assert_eq!(signal_to_direction(SignalType::AnalogOutput), PinDirection::Output);
        assert_eq!(signal_to_direction(SignalType::DigitalOutput), PinDirection::Output);
    }

    #[test]
    fn signal_id_to_label_转换() {
        assert_eq!(signal_id_to_label("hydraulic_pressure"), "Hydraulic Pressure");
        assert_eq!(signal_id_to_label("servo_ready"), "Servo Ready");
        assert_eq!(signal_id_to_label("pressure"), "Pressure");
    }

    #[test]
    fn signals_to_pin_definitions_完整转换() {
        let signals = vec![
            SignalSpec {
                id: "pressure".to_owned(),
                signal_type: SignalType::AnalogInput,
                unit: Some("MPa".to_owned()),
                range: Some(crate::workflow::Range {
                    min: 0.0,
                    max: 35.0,
                }),
                source: register_source(40001),
                scale: None,
            },
            SignalSpec {
                id: "servo_ready".to_owned(),
                signal_type: SignalType::DigitalInput,
                unit: None,
                range: None,
                source: register_source(40100),
                scale: None,
            },
        ];

        let pins = signals_to_pin_definitions(&signals);
        assert_eq!(pins.len(), 2);

        // pressure pin
        assert_eq!(pins[0].id, "pressure");
        assert_eq!(pins[0].pin_type, PinType::Float);
        assert_eq!(pins[0].direction, PinDirection::Input);
        assert_eq!(pins[0].kind, PinKind::Data);
        assert_eq!(pins[0].label, "Pressure (MPa)");
        assert!(pins[0].description.is_some());

        // servo_ready pin
        assert_eq!(pins[1].id, "servo_ready");
        assert_eq!(pins[1].pin_type, PinType::Bool);
        assert_eq!(pins[1].direction, PinDirection::Input);
        assert!(!pins[1].required);
    }

    #[test]
    fn 空信号列表返回空() {
        let pins = signals_to_pin_definitions(&[]);
        assert!(pins.is_empty());
    }
}
