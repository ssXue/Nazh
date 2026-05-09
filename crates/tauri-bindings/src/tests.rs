use super::*;
use nazh_core::{
    EngineError, NodeCapabilities, NodeRegistry, NodeTrait, SharedResources, WorkflowNodeDefinition,
};
use std::sync::Arc;

fn stub_factory(
    _def: &WorkflowNodeDefinition,
    _res: SharedResources,
) -> Result<Arc<dyn NodeTrait>, EngineError> {
    Err(EngineError::unsupported_node_type("test-stub"))
}

#[test]
fn list_node_types_response_排序后输出全部类型() {
    let mut registry = NodeRegistry::new();
    registry.register_with_capabilities("timer", NodeCapabilities::empty(), stub_factory);
    registry.register_with_capabilities("code", NodeCapabilities::empty(), stub_factory);
    registry.register_with_capabilities("native", NodeCapabilities::empty(), stub_factory);

    let response = list_node_types_response(&registry);
    assert_eq!(response.types.len(), 3);
    assert_eq!(response.types[0].name, "code");
    assert_eq!(response.types[1].name, "native");
    assert_eq!(response.types[2].name, "timer");
}

#[test]
fn list_node_types_response_空注册表返回空列表() {
    let registry = NodeRegistry::new();
    let response = list_node_types_response(&registry);
    assert!(response.types.is_empty());
}

#[test]
fn list_node_types_response_透传能力标签位图() {
    let mut registry = NodeRegistry::new();
    registry.register_with_capabilities("timer", NodeCapabilities::TRIGGER, stub_factory);
    registry.register_with_capabilities("modbusRead", NodeCapabilities::DEVICE_IO, stub_factory);
    registry.register_with_capabilities("plain", NodeCapabilities::empty(), stub_factory);

    let response = list_node_types_response(&registry);
    let by_name: std::collections::HashMap<&str, u32> = response
        .types
        .iter()
        .map(|entry| (entry.name.as_str(), entry.capabilities))
        .collect();

    assert_eq!(by_name["timer"], NodeCapabilities::TRIGGER.bits());
    assert_eq!(by_name["modbusRead"], NodeCapabilities::DEVICE_IO.bits());
    assert_eq!(by_name["plain"], 0);
}

#[cfg(feature = "ts-export")]
#[test]
fn export_bindings() {
    super::export_all().unwrap();
}
