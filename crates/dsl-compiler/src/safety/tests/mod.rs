use nazh_dsl_core::capability::{
    CapabilityImpl, CapabilityParam, CapabilitySpec, SafetyConstraints, SafetyLevel,
};
use nazh_dsl_core::device::{ConnectionRef, DeviceSpec, SignalSource, SignalSpec, SignalType};
use nazh_dsl_core::workflow::{Range, WorkflowSpec};

use super::*;
use crate::CompilerContext;

// ---- 测试辅助 ----

fn sample_device_with_signals(id: &str, signals: Vec<SignalSpec>) -> DeviceSpec {
    DeviceSpec {
        id: id.to_owned(),
        device_type: "test".to_owned(),
        manufacturer: None,
        model: None,
        connection: Some(ConnectionRef {
            connection_type: "modbus-tcp".to_owned(),
            id: format!("{id}_conn"),
            unit: Some(1),
        }),
        network_group: None,
        signals,
        alarms: vec![],
    }
}

fn sample_device(id: &str) -> DeviceSpec {
    sample_device_with_signals(id, vec![])
}

fn readable_signal(id: &str) -> SignalSpec {
    SignalSpec {
        id: id.to_owned(),
        signal_type: SignalType::AnalogInput,
        unit: Some("MPa".to_owned()),
        range: Some(Range {
            min: 0.0,
            max: 35.0,
        }),
        source: SignalSource::Register {
            register: 40001,
            access: nazh_dsl_core::device::AccessMode::Read,
            data_type: nazh_dsl_core::device::DataType::Float32,
            bit: None,
        },
        scale: None,
    }
}

fn writable_signal(id: &str) -> SignalSpec {
    SignalSpec {
        id: id.to_owned(),
        signal_type: SignalType::AnalogOutput,
        unit: Some("mm".to_owned()),
        range: Some(Range {
            min: 0.0,
            max: 150.0,
        }),
        source: SignalSource::Register {
            register: 40010,
            access: nazh_dsl_core::device::AccessMode::Write,
            data_type: nazh_dsl_core::device::DataType::Float32,
            bit: None,
        },
        scale: None,
    }
}

fn cap_with_input(
    id: &str,
    device_id: &str,
    input_id: &str,
    unit: Option<&str>,
    range: Option<Range>,
    register: u16,
) -> CapabilitySpec {
    CapabilitySpec {
        id: id.to_owned(),
        device_id: device_id.to_owned(),
        description: String::new(),
        inputs: vec![CapabilityParam {
            id: input_id.to_owned(),
            unit: unit.map(String::from),
            range,
            required: true,
        }],
        outputs: vec![],
        preconditions: vec![],
        effects: vec![],
        implementation: CapabilityImpl::ModbusWrite {
            register,
            value: format!("${{{input_id}}}"),
        },
        fallback: vec![],
        safety: SafetyConstraints {
            level: SafetyLevel::Low,
            requires_approval: false,
            max_execution_time: None,
        },
    }
}

fn parse_spec(yaml: &str) -> WorkflowSpec {
    serde_yaml::from_str(yaml).unwrap()
}

// ---- 规则 1: 单位一致性 ----

mod action_rules;
mod interlock;
mod preconditions;
mod state_graph;
