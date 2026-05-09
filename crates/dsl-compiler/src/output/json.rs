use nazh_dsl_core::capability::CapabilityImpl;
use nazh_dsl_core::workflow::ActionTarget;
use serde_json::{Map, Value};

/// 将 `CapabilityImpl` 映射为编译器输出的 JSON 片段。
pub(super) fn capability_impl_to_json(impl_: &CapabilityImpl) -> Value {
    match impl_ {
        CapabilityImpl::ModbusWrite { register, value } => serde_json::json!({
            "type": "modbus-write",
            "register": register,
            "value_template": value,
        }),
        CapabilityImpl::MqttPublish { topic, payload } => serde_json::json!({
            "type": "mqtt-publish",
            "topic": topic,
            "payload_template": payload,
        }),
        CapabilityImpl::SerialCommand { command } => serde_json::json!({
            "type": "serial-command",
            "command_template": command,
        }),
        CapabilityImpl::CanWrite {
            can_id,
            data,
            is_extended,
        } => serde_json::json!({
            "type": "can-write",
            "can_id": can_id,
            "data_template": data,
            "is_extended": is_extended,
        }),
        CapabilityImpl::Script { content } => serde_json::json!({
            "type": "script",
            "content": content,
        }),
    }
}

/// 从 `serde_json::Value` 推断 `PinType` 的 JSON 表示。
///
/// 推断规则：整数→Integer，浮点→Float，字符串→String，布尔→Bool，其余→Any。
pub(super) fn infer_pin_type_json(value: &Value) -> Value {
    match value {
        Value::Bool(_) => serde_json::json!({ "kind": "bool" }),
        Value::Number(n) => {
            if n.is_i64() || n.is_u64() {
                serde_json::json!({ "kind": "integer" })
            } else {
                serde_json::json!({ "kind": "float" })
            }
        }
        Value::String(_) => serde_json::json!({ "kind": "string" }),
        _ => serde_json::json!({ "kind": "any" }),
    }
}

/// 提取 action 目标 ID。
pub(super) fn action_target_id(target: &ActionTarget) -> &str {
    match target {
        ActionTarget::Capability(id) | ActionTarget::Action(id) => id,
    }
}

/// 将 `HashMap<String, Value>` 转为 `serde_json::Map`。
pub(super) fn map_to_json_map(
    map: &std::collections::HashMap<String, Value>,
) -> Map<String, Value> {
    let mut result = Map::new();
    for (k, v) in map {
        result.insert(k.clone(), v.clone());
    }
    result
}
