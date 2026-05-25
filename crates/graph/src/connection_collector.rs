//! 部署期连接引用收集器（ADR-0026 Phase 1）。
//!
//! 从 [`WorkflowGraph`](nazh_core::WorkflowNodeDefinition) 的节点配置中收集实际引用的连接 ID 集合。
//! 低层协议节点通过 [`WorkflowNodeDefinition::connection_id()`] 显式声明连接；
//! 高级设备节点（`deviceSignalRead` / `deviceEventTrigger` / `capabilityCall`）
//! 通过 `config.device_id` 引用设备，设备快照中包含绑定的 `connection.id`。
//!
//! 调用方将收集到的 ID 集合传给连接解析器，实现"部署只加载引用连接"。

use std::collections::{BTreeSet, HashMap, HashSet};
use std::hash::BuildHasher;

use nazh_core::{EngineError, WorkflowNodeDefinition};

/// 高级设备节点类型——从设备继承连接，不在 `config.connection_id` 中声明。
const DEVICE_BACKED_NODE_TYPES: &[&str] =
    &["deviceSignalRead", "deviceEventTrigger", "capabilityCall"];

/// 设备绑定信息（从设备快照提取）。
#[derive(Debug, Clone)]
pub struct DeviceBinding {
    pub device_id: String,
    pub connection_id: String,
}

/// 连接引用收集结果。
#[derive(Debug, Clone)]
pub struct ConnectionReferenceReport {
    /// 所有引用到的连接 ID（去重、有序）。
    pub referenced_ids: Vec<String>,
    /// 从低层协议节点的显式 `connection_id` 收集到的 ID。
    pub explicit_ids: Vec<String>,
    /// 从高级设备节点经设备快照继承得到的 ID。
    pub inherited_ids: Vec<String>,
}

/// 从工作流图中收集所有被引用的连接 ID。
///
/// `device_bindings` 是从设备快照提取的 `{ device_id → connection_id }` 映射，
/// 由调用方在工作区加载设备资产后构建。
///
/// # Errors
///
/// 高级设备节点引用了不存在的 `device_id`，或设备未绑定连接时返回可定位错误。
pub fn collect_referenced_connection_ids<S1: BuildHasher, S2: BuildHasher>(
    nodes: &HashMap<String, WorkflowNodeDefinition, S1>,
    device_bindings: &HashMap<String, DeviceBinding, S2>,
) -> Result<ConnectionReferenceReport, EngineError> {
    let mut explicit_set: BTreeSet<String> = BTreeSet::new();
    let mut inherited_set: BTreeSet<String> = BTreeSet::new();
    let device_backed_types: HashSet<&str> = DEVICE_BACKED_NODE_TYPES.iter().copied().collect();

    for (node_id, node_def) in nodes {
        let node_type = node_def.node_type();

        if device_backed_types.contains(node_type) {
            // 高级设备节点：从 config.device_id 查设备快照取连接
            let device_id = extract_device_id(node_def.config()).ok_or_else(|| {
                EngineError::node_config(node_id.clone(), format!("{node_type} 必须配置 device_id"))
            })?;

            let binding = device_bindings.get(&device_id).ok_or_else(|| {
                EngineError::node_config(
                    node_id.clone(),
                    format!(
                        "{node_type} 引用设备 `{device_id}`，但该设备无连接绑定；\
                         请在设备资产中绑定连接，或检查设备 ID 是否正确"
                    ),
                )
            })?;
            inherited_set.insert(binding.connection_id.clone());
        } else if let Some(conn_id) = node_def.connection_id() {
            // 低层协议节点：显式 connection_id
            explicit_set.insert(conn_id.to_owned());
        }
    }

    let referenced_ids: BTreeSet<String> = explicit_set.union(&inherited_set).cloned().collect();

    Ok(ConnectionReferenceReport {
        referenced_ids: referenced_ids.into_iter().collect(),
        explicit_ids: explicit_set.into_iter().collect(),
        inherited_ids: inherited_set.into_iter().collect(),
    })
}

/// 从节点 config JSON 中提取 `device_id` 字段。
fn extract_device_id(config: &serde_json::Value) -> Option<String> {
    config.get("device_id")?.as_str().map(String::from)
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use serde_json::json;

    fn node_with_conn(node_type: &str, connection_id: &str) -> (String, WorkflowNodeDefinition) {
        let raw = json!({
            "id": format!("node_{node_type}"),
            "type": node_type,
            "connection_id": connection_id,
        });
        let def: WorkflowNodeDefinition = serde_json::from_value(raw).unwrap();
        (def.id().to_owned(), def)
    }

    fn node_with_config(
        node_type: &str,
        config: &serde_json::Value,
    ) -> (String, WorkflowNodeDefinition) {
        let raw = json!({
            "id": format!("node_{node_type}"),
            "type": node_type,
            "config": config,
        });
        let def: WorkflowNodeDefinition = serde_json::from_value(raw).unwrap();
        (def.id().to_owned(), def)
    }

    fn binding(device_id: &str, connection_id: &str) -> DeviceBinding {
        DeviceBinding {
            device_id: device_id.to_owned(),
            connection_id: connection_id.to_owned(),
        }
    }

    #[test]
    fn 空图_返回空集合() {
        let nodes = HashMap::new();
        let bindings = HashMap::new();
        let report = collect_referenced_connection_ids(&nodes, &bindings).unwrap();
        assert!(report.referenced_ids.is_empty());
    }

    #[test]
    fn 纯显式连接_低层节点() {
        let nodes: HashMap<String, WorkflowNodeDefinition> = [
            node_with_conn("modbusRead", "modbus_tcp_1"),
            node_with_conn("serialTrigger", "serial_plc"),
        ]
        .into();

        let bindings = HashMap::new();
        let report = collect_referenced_connection_ids(&nodes, &bindings).unwrap();
        assert_eq!(report.referenced_ids, vec!["modbus_tcp_1", "serial_plc"]);
        assert!(report.inherited_ids.is_empty());
    }

    #[test]
    fn 纯设备继承_高级节点() {
        let nodes: HashMap<String, WorkflowNodeDefinition> = [
            node_with_config("deviceSignalRead", &json!({"device_id": "press_1"})),
            node_with_config("capabilityCall", &json!({"device_id": "servo_1"})),
        ]
        .into();

        let mut bindings = HashMap::new();
        bindings.insert("press_1".to_owned(), binding("press_1", "modbus_tcp_1"));
        bindings.insert("servo_1".to_owned(), binding("servo_1", "ethercat_bus"));

        let report = collect_referenced_connection_ids(&nodes, &bindings).unwrap();
        assert_eq!(report.referenced_ids, vec!["ethercat_bus", "modbus_tcp_1"]);
        assert_eq!(report.inherited_ids, vec!["ethercat_bus", "modbus_tcp_1"]);
        assert!(report.explicit_ids.is_empty());
    }

    #[test]
    fn 混合引用_去重() {
        let nodes: HashMap<String, WorkflowNodeDefinition> = [
            node_with_conn("modbusRead", "modbus_tcp_1"),
            node_with_config("deviceSignalRead", &json!({"device_id": "press_1"})),
        ]
        .into();

        let mut bindings = HashMap::new();
        // 同一个连接既被低层节点显式引用，又被设备绑定
        bindings.insert("press_1".to_owned(), binding("press_1", "modbus_tcp_1"));

        let report = collect_referenced_connection_ids(&nodes, &bindings).unwrap();
        assert_eq!(report.referenced_ids, vec!["modbus_tcp_1"]);
        assert_eq!(report.explicit_ids, vec!["modbus_tcp_1"]);
        assert_eq!(report.inherited_ids, vec!["modbus_tcp_1"]);
    }

    #[test]
    fn 高级节点缺_device_id_返回可定位错误() {
        let nodes: HashMap<String, WorkflowNodeDefinition> =
            [node_with_config("deviceSignalRead", &json!({}))].into();

        let bindings = HashMap::new();
        let err = collect_referenced_connection_ids(&nodes, &bindings).unwrap_err();
        assert!(err.to_string().contains("device_id"));
    }

    #[test]
    fn 设备未绑定连接_返回可定位错误() {
        let nodes: HashMap<String, WorkflowNodeDefinition> = [node_with_config(
            "capabilityCall",
            &json!({"device_id": "orphan_device"}),
        )]
        .into();

        let bindings = HashMap::new();
        let err = collect_referenced_connection_ids(&nodes, &bindings).unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("orphan_device"));
        assert!(msg.contains("连接绑定"));
    }

    #[test]
    fn 非设备节点_无连接_id_不报错() {
        // if/switch/code 等控制流节点无 connection_id，正常通过
        let raw = json!({"id": "if_1", "type": "if", "config": {"condition": "true"}});
        let def: WorkflowNodeDefinition = serde_json::from_value(raw).unwrap();
        let nodes: HashMap<String, WorkflowNodeDefinition> = [("if_1".to_owned(), def)].into();

        let bindings = HashMap::new();
        let report = collect_referenced_connection_ids(&nodes, &bindings).unwrap();
        assert!(report.referenced_ids.is_empty());
    }
}
