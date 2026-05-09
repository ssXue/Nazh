use super::*;

/// 位分配由 ADR-0011 锁死；任何改动都会破坏 IPC 契约与前端常量表，必须同步。
#[test]
fn node_capabilities_位分配与_adr_0011_一致() {
    assert_eq!(NodeCapabilities::PURE.bits(), 0b0000_0001);
    assert_eq!(NodeCapabilities::NETWORK_IO.bits(), 0b0000_0010);
    assert_eq!(NodeCapabilities::FILE_IO.bits(), 0b0000_0100);
    assert_eq!(NodeCapabilities::DEVICE_IO.bits(), 0b0000_1000);
    assert_eq!(NodeCapabilities::TRIGGER.bits(), 0b0001_0000);
    assert_eq!(NodeCapabilities::BRANCHING.bits(), 0b0010_0000);
    assert_eq!(NodeCapabilities::MULTI_OUTPUT.bits(), 0b0100_0000);
    assert_eq!(NodeCapabilities::BLOCKING.bits(), 0b1000_0000);
}

#[test]
fn node_capabilities_可按位组合() {
    let caps = NodeCapabilities::PURE | NodeCapabilities::BRANCHING;
    assert!(caps.contains(NodeCapabilities::PURE));
    assert!(caps.contains(NodeCapabilities::BRANCHING));
    assert!(!caps.contains(NodeCapabilities::NETWORK_IO));
}

#[test]
fn node_capabilities_default_是空集合() {
    let caps = NodeCapabilities::default();
    assert!(caps.is_empty());
    assert_eq!(caps.bits(), 0);
}

mod is_pure_form_tests {
    use super::*;
    use crate::{EmptyPolicy, PinDefinition, PinDirection, PinKind, PinType};
    use async_trait::async_trait;
    use serde_json::Value;

    struct StubNode {
        inputs: Vec<PinDefinition>,
        outputs: Vec<PinDefinition>,
    }

    #[async_trait]
    impl NodeTrait for StubNode {
        fn id(&self) -> &'static str {
            "stub"
        }
        fn kind(&self) -> &'static str {
            "stub"
        }
        fn input_pins(&self) -> Vec<PinDefinition> {
            self.inputs.clone()
        }
        fn output_pins(&self) -> Vec<PinDefinition> {
            self.outputs.clone()
        }
        async fn transform(&self, _: Uuid, payload: Value) -> Result<NodeExecution, EngineError> {
            Ok(NodeExecution::broadcast(payload))
        }
    }

    fn data_pin(id: &str, dir: PinDirection) -> PinDefinition {
        PinDefinition {
            id: id.to_owned(),
            label: id.to_owned(),
            pin_type: PinType::Float,
            direction: dir,
            required: false,
            kind: PinKind::Data,
            description: None,
            empty_policy: EmptyPolicy::default(),
            block_timeout_ms: None,
            ttl_ms: None,
        }
    }

    fn exec_pin(id: &str, dir: PinDirection) -> PinDefinition {
        PinDefinition {
            id: id.to_owned(),
            label: id.to_owned(),
            pin_type: PinType::Any,
            direction: dir,
            required: matches!(dir, PinDirection::Input),
            kind: PinKind::Exec,
            description: None,
            empty_policy: EmptyPolicy::default(),
            block_timeout_ms: None,
            ttl_ms: None,
        }
    }

    #[test]
    fn 全_data_引脚是_pure_form() {
        let n = StubNode {
            inputs: vec![data_pin("in", PinDirection::Input)],
            outputs: vec![data_pin("out", PinDirection::Output)],
        };
        assert!(is_pure_form(&n));
    }

    #[test]
    fn 输入混_exec_不是_pure_form() {
        let n = StubNode {
            inputs: vec![exec_pin("in", PinDirection::Input)],
            outputs: vec![data_pin("out", PinDirection::Output)],
        };
        assert!(!is_pure_form(&n));
    }

    #[test]
    fn 输出混_exec_不是_pure_form() {
        let n = StubNode {
            inputs: vec![data_pin("in", PinDirection::Input)],
            outputs: vec![exec_pin("out", PinDirection::Output)],
        };
        assert!(!is_pure_form(&n));
    }

    #[test]
    fn 仅有输出且全_data_仍是_pure_form() {
        let n = StubNode {
            inputs: vec![],
            outputs: vec![data_pin("out", PinDirection::Output)],
        };
        assert!(is_pure_form(&n));
    }
}
