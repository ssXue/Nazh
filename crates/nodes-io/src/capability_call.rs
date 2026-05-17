//! 通用能力调用节点：由 DSL 编译器生成，运行时解析 capability 实现并执行协议操作。
//!
//! 编译期将 capability 实现细节烘焙到 config（"snapshot" 模型），
//! 运行时无需查注册表，只需模板替换 + `ConnectionManager` 借用连接执行。

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use uuid::Uuid;

use std::collections::HashMap;
use std::sync::Arc;

use connections::{ConnectionGuard, SharedConnectionManager};
use nazh_core::{EngineError, NodeExecution, NodeTrait, WorkflowVariables};

mod executor;
mod protocol;

use self::protocol::connection_kind_matches;

/// 能力调用节点配置——编译期烘焙。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CapabilityCallConfig {
    /// 连接 ID（来自节点顶层 `connection_id` 或 config）。
    #[serde(default)]
    pub connection_id: Option<String>,
    /// 能力 ID（如 `hydraulic_axis.move_to`）。
    pub capability_id: String,
    /// 设备 ID。
    pub device_id: String,
    /// 能力实现快照（编译期从 CapabilitySpec.implementation 复制）。
    pub implementation: CapabilityImplSnapshot,
    /// 参数模板（值中可含 `${var_name}` 占位符）。
    #[serde(default)]
    pub args: HashMap<String, Value>,
}

/// 能力实现快照——与 `dsl-core::CapabilityImpl` 对应但独立定义。
///
/// 编译器直接输出匹配此 serde 格式的 JSON，conformance test 守护一致性。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "kebab-case")]
pub enum CapabilityImplSnapshot {
    ModbusWrite {
        register: u16,
        value_template: String,
    },
    MqttPublish {
        topic: String,
        payload_template: String,
    },
    SerialCommand {
        command_template: String,
    },
    CanWrite {
        can_id: u32,
        data_template: String,
        is_extended: bool,
    },
    Script {
        content: String,
    },
}

/// 通用能力调用节点。
pub struct CapabilityCallNode {
    id: String,
    config: CapabilityCallConfig,
    variables: Option<Arc<WorkflowVariables>>,
    // 保留供后续协议节点执行时借出连接（Modbus/MQTT/Serial/CAN）
    #[allow(dead_code)]
    connection_manager: SharedConnectionManager,
}

impl CapabilityCallNode {
    pub fn new(
        id: impl Into<String>,
        config: CapabilityCallConfig,
        variables: Option<Arc<WorkflowVariables>>,
        connection_manager: SharedConnectionManager,
    ) -> Self {
        Self {
            id: id.into(),
            config,
            variables,
            connection_manager,
        }
    }

    /// 解析模板字符串中的 `${var_name}` 占位符。
    ///
    /// 查找顺序：payload 同名键 → `WorkflowVariables` → config.args。
    fn resolve_template(&self, template: &str, payload: &Value) -> String {
        let mut result = template.to_owned();
        // 简单的 ${...} 替换——不支持嵌套
        let mut start = 0;
        while let Some(open) = result[start..].find("${") {
            let abs_open = start + open;
            let Some(close) = result[abs_open..].find('}') else {
                break;
            };
            let abs_close = abs_open + close;
            let var_name = &result[abs_open + 2..abs_close];

            let resolved = self.resolve_variable(var_name, payload);
            result.replace_range(abs_open..=abs_close, &resolved);
            // 跳过已替换部分
            start = abs_open + resolved.len();
        }
        result
    }

    fn resolve_variable(&self, var_name: &str, payload: &Value) -> String {
        // 1. payload 中的字段
        if let Some(val) = payload.get(var_name) {
            return value_to_string(val);
        }
        // 2. WorkflowVariables
        if let Some(vars) = &self.variables
            && let Some(val) = vars.get_value(var_name)
        {
            return value_to_string(&val);
        }
        // 3. config.args
        if let Some(val) = self.config.args.get(var_name) {
            return value_to_string(val);
        }
        // 未解析 → 保留原始占位
        format!("${{{var_name}}}")
    }

    fn connection_id(&self) -> Result<&str, EngineError> {
        self.config
            .connection_id
            .as_deref()
            .filter(|value| !value.trim().is_empty())
            .ok_or_else(|| {
                EngineError::node_config(
                    self.id.clone(),
                    "capabilityCall 执行协议动作必须配置 connection_id".to_owned(),
                )
            })
    }

    async fn acquire_for_protocol(
        &self,
        protocol: &str,
        allowed_kinds: &[&str],
    ) -> Result<ConnectionGuard, EngineError> {
        let connection_id = self.connection_id()?;
        let mut guard = self.connection_manager.acquire(connection_id).await?;
        if !connection_kind_matches(&guard.lease().kind, allowed_kinds) {
            let reason = format!(
                "capabilityCall `{}` 需要 {protocol} 连接，实际连接 `{connection_id}` 类型为 `{}`",
                self.config.capability_id,
                guard.lease().kind
            );
            guard.mark_failure(&reason);
            return Err(EngineError::node_config(self.id.clone(), reason));
        }
        Ok(guard)
    }

    fn capability_metadata(&self) -> Map<String, Value> {
        Map::from_iter([(
            "capability_call".to_owned(),
            serde_json::json!({
                "capability_id": self.config.capability_id,
                "device_id": self.config.device_id,
                "connection_id": self.config.connection_id,
            }),
        )])
    }

    fn output(&self, payload: Value, protocol_metadata: Option<(&str, Value)>) -> NodeExecution {
        let mut metadata = self.capability_metadata();
        if let Some((key, value)) = protocol_metadata {
            metadata.insert(key.to_owned(), value);
        }
        NodeExecution::from_outputs(vec![nazh_core::NodeOutput {
            payload,
            metadata: Some(metadata),
            dispatch: nazh_core::NodeDispatch::Broadcast,
        }])
    }
}

fn value_to_string(val: &Value) -> String {
    match val {
        Value::String(s) => s.clone(),
        Value::Number(n) => n.to_string(),
        Value::Bool(b) => b.to_string(),
        other => other.to_string(),
    }
}

#[async_trait]
impl NodeTrait for CapabilityCallNode {
    fn id(&self) -> &str {
        &self.id
    }

    fn kind(&self) -> &'static str {
        "capabilityCall"
    }

    async fn transform(
        &self,
        _trace_id: Uuid,
        payload: Value,
    ) -> Result<NodeExecution, EngineError> {
        // 解析 args 中的模板
        let mut resolved_args = HashMap::new();
        for (key, val) in &self.config.args {
            if let Some(s) = val.as_str() {
                resolved_args.insert(
                    key.clone(),
                    Value::String(self.resolve_template(s, &payload)),
                );
            } else {
                resolved_args.insert(key.clone(), val.clone());
            }
        }

        // 根据 implementation 类型执行对应协议操作。
        match &self.config.implementation {
            CapabilityImplSnapshot::ModbusWrite {
                register,
                value_template,
            } => {
                let resolved_value = self.resolve_template(value_template, &payload);
                self.execute_modbus_write(_trace_id, *register, resolved_value, resolved_args)
                    .await
            }
            CapabilityImplSnapshot::MqttPublish {
                topic,
                payload_template,
            } => {
                let resolved_topic = self.resolve_template(topic, &payload);
                let resolved_payload = self.resolve_template(payload_template, &payload);
                self.execute_mqtt_publish(
                    _trace_id,
                    resolved_topic,
                    resolved_payload,
                    resolved_args,
                )
                .await
            }
            CapabilityImplSnapshot::SerialCommand { command_template } => {
                let resolved_cmd = self.resolve_template(command_template, &payload);
                self.execute_serial_command(_trace_id, resolved_cmd, resolved_args)
                    .await
            }
            CapabilityImplSnapshot::CanWrite {
                can_id,
                data_template,
                is_extended,
            } => {
                let resolved_data = self.resolve_template(data_template, &payload);
                self.execute_can_write(
                    _trace_id,
                    *can_id,
                    resolved_data,
                    *is_extended,
                    resolved_args,
                )
                .await
            }
            CapabilityImplSnapshot::Script { content } => {
                let _ = (content, resolved_args);
                Err(EngineError::node_config(
                    self.id.clone(),
                    "capabilityCall 的 script implementation 尚未接入实际执行器，不能返回意图成功",
                ))
            }
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    fn sample_config() -> CapabilityCallConfig {
        CapabilityCallConfig {
            connection_id: None,
            capability_id: "axis.move_to".to_owned(),
            device_id: "press_1".to_owned(),
            implementation: CapabilityImplSnapshot::ModbusWrite {
                register: 40010,
                value_template: "${position}".to_owned(),
            },
            args: {
                let mut m = HashMap::new();
                m.insert(
                    "position".to_owned(),
                    Value::String("${target_pos}".to_owned()),
                );
                m
            },
        }
    }

    #[test]
    fn config_从_json_解析成功() {
        let json = serde_json::json!({
            "capability_id": "axis.move_to",
            "device_id": "press_1",
            "implementation": {
                "type": "modbus-write",
                "register": 40010,
                "value_template": "${position}"
            },
            "args": {
                "position": "${target_pos}"
            }
        });
        let config: CapabilityCallConfig = serde_json::from_value(json).unwrap();
        assert_eq!(config.capability_id, "axis.move_to");
        assert!(matches!(
            config.implementation,
            CapabilityImplSnapshot::ModbusWrite {
                register: 40010,
                ..
            }
        ));
    }

    #[test]
    fn 模板解析_替换_payload_字段() {
        let config = sample_config();
        let cm = connections::shared_connection_manager();
        let node = CapabilityCallNode::new("test_node", config, None, cm);

        let payload = serde_json::json!({ "position": 100.5 });
        let resolved = node.resolve_template("${position}", &payload);
        assert_eq!(resolved, "100.5");
    }

    #[test]
    fn 模板解析_未找到变量保留占位() {
        let config = sample_config();
        let cm = connections::shared_connection_manager();
        let node = CapabilityCallNode::new("test_node", config, None, cm);

        let payload = serde_json::json!({});
        let resolved = node.resolve_template("${unknown}", &payload);
        assert_eq!(resolved, "${unknown}");
    }

    #[tokio::test]
    async fn script_implementation_尚未接入执行器时失败() {
        let config = CapabilityCallConfig {
            connection_id: None,
            capability_id: "axis.calculate".to_owned(),
            device_id: "press_1".to_owned(),
            implementation: CapabilityImplSnapshot::Script {
                content: "let target = ${target_pos};".to_owned(),
            },
            args: HashMap::new(),
        };
        let cm = connections::shared_connection_manager();
        let node = CapabilityCallNode::new("test_node", config, None, cm);

        let err = node
            .transform(Uuid::new_v4(), serde_json::json!({ "target_pos": 50.0 }))
            .await
            .unwrap_err();

        assert!(
            err.to_string()
                .contains("script implementation 尚未接入实际执行器"),
            "错误应明确说明脚本 implementation 不会伪成功，实际为: {err}"
        );
    }

    #[tokio::test]
    async fn 缺少连接_id_时拒绝协议动作() {
        let config = sample_config();
        let cm = connections::shared_connection_manager();
        let node = CapabilityCallNode::new("test_node", config, None, cm);

        let err = node
            .transform(Uuid::new_v4(), serde_json::json!({ "target_pos": 50 }))
            .await
            .unwrap_err();

        assert!(
            err.to_string().contains("connection_id"),
            "错误应明确指出 capabilityCall 缺少 connection_id，实际为: {err}"
        );
    }

    #[tokio::test]
    async fn can_write_绑定连接时发送并返回底层元数据() {
        let config: CapabilityCallConfig = serde_json::from_value(serde_json::json!({
            "connection_id": "can-fast",
            "capability_id": "axis.send_frame",
            "device_id": "press_1",
            "implementation": {
                "type": "can-write",
                "can_id": 0x123,
                "data_template": "01 02 03",
                "is_extended": false
            }
        }))
        .unwrap();
        let cm = connections::shared_connection_manager();
        cm.register_connection(connections::ConnectionDefinition {
            id: "can-fast".to_owned(),
            kind: "can".to_owned(),
            metadata: serde_json::json!({
                "interface": "mock",
                "channel": "mock-can-fast",
                "baud_rate": 115_200,
                "bitrate": 500_000
            }),
        })
        .await
        .unwrap();
        let node = CapabilityCallNode::new("test_node", config, None, cm);

        let result = node
            .transform(Uuid::new_v4(), serde_json::json!({}))
            .await
            .unwrap();

        let output = &result.outputs[0];
        assert_eq!(output.payload["operation"], "can-write");
        assert_eq!(output.payload["sent"]["id"], 0x123);
        assert_eq!(output.payload["sent"]["data_hex"], "010203");

        let meta = output.metadata.as_ref().unwrap();
        assert_eq!(meta["capability_call"]["capability_id"], "axis.send_frame");
        assert_eq!(meta["can"]["simulated"], false);
        assert_eq!(meta["can"]["connection"]["id"], "can-fast");
    }

    #[test]
    fn 所有_implementation_类型_序列化_反序列化() {
        let variants = vec![
            CapabilityImplSnapshot::ModbusWrite {
                register: 1,
                value_template: "v".to_owned(),
            },
            CapabilityImplSnapshot::MqttPublish {
                topic: "t".to_owned(),
                payload_template: "p".to_owned(),
            },
            CapabilityImplSnapshot::SerialCommand {
                command_template: "c".to_owned(),
            },
            CapabilityImplSnapshot::CanWrite {
                can_id: 0x123,
                data_template: "0102".to_owned(),
                is_extended: false,
            },
            CapabilityImplSnapshot::Script {
                content: "s".to_owned(),
            },
        ];
        for v in &variants {
            let json = serde_json::to_string(v).unwrap();
            let back: CapabilityImplSnapshot = serde_json::from_str(&json).unwrap();
            let json2 = serde_json::to_string(&back).unwrap();
            assert_eq!(json, json2);
        }
    }
}
