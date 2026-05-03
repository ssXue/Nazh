//! 三段式 DSL 类型定义与 YAML 解析（RFC-0004 Phase 0）。
//!
//! 本 crate 定义设备（Device）、能力（Capability）、工作流（Workflow）
//! 三种 DSL 的结构化类型，并提供从 YAML 文本解析这些类型的 API。

pub mod capability;
pub mod device;
pub mod error;
pub mod parser;
pub mod pin_mapping;
pub mod workflow;

pub use capability::{
    CapabilityImpl, CapabilityOutput, CapabilityParam, CapabilitySpec, SafetyConstraints,
    SafetyLevel, generate_capabilities_from_device,
};
pub use device::{
    AccessMode, AlarmSeverity, AlarmSpec, ConnectionRef, DataType, DeviceSpec, SignalSource,
    SignalSpec, SignalType,
};
pub use error::DslError;
pub use parser::{parse_capability_yaml, parse_device_yaml, parse_workflow_yaml};
pub use pin_mapping::{signal_to_direction, signal_to_pin_type, signals_to_pin_definitions};
pub use workflow::{
    ActionSpec, ActionTarget, HumanDuration, Range, StateSpec, TransitionSpec, WorkflowSpec,
};
