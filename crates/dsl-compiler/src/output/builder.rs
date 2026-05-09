use nazh_dsl_core::workflow::{ActionTarget, StateSpec, WorkflowSpec};
use serde_json::{Map, Value};

use crate::context::CompilerContext;
use crate::error::CompileError;

use super::guards::{sanitize_id, sanitize_node_id};
use super::json::{
    action_target_id, capability_impl_to_json, infer_pin_type_json, map_to_json_map,
};

/// 单个 action 实例的描述，用于生成 action port 对应的节点。
#[derive(Debug, Clone)]
struct ActionInstance {
    /// 动作端口 ID（如 `entry_approaching_0`、`trans_idle_approaching`）。
    port_id: String,
    /// Capability ID 或 system action 名称。
    target_id: String,
    /// true = Capability，false = Action。
    is_capability: bool,
    /// 当前 action 实例自己的参数，不能按 target 回查。
    args: Map<String, Value>,
}

pub(super) struct GraphBuilder<'a> {
    ctx: &'a CompilerContext,
    spec: &'a WorkflowSpec,
    initial_state: &'a str,
    /// stateMachine 节点的 ID。
    sm_node_id: String,
    /// 已生成的节点 JSON。
    nodes: Map<String, Value>,
    /// 所有 action 实例。
    actions: Vec<ActionInstance>,
    /// 已生成的边。
    edges: Vec<Value>,
    /// 已生成的变量。
    variables: Map<String, Value>,
}

impl<'a> GraphBuilder<'a> {
    pub(super) fn new(
        ctx: &'a CompilerContext,
        spec: &'a WorkflowSpec,
        initial_state: &'a str,
    ) -> Self {
        let sm_node_id = sanitize_node_id(&format!("sm_{}", spec.id));
        Self {
            ctx,
            spec,
            initial_state,
            sm_node_id,
            nodes: Map::new(),
            actions: Vec::new(),
            edges: Vec::new(),
            variables: Map::new(),
        }
    }

    /// 收集所有 action 实例，为每个分配端口 ID。
    pub(super) fn collect_actions(&mut self) {
        // Entry / exit actions
        for (state_name, state) in &self.spec.states {
            self.collect_state_actions(state_name, state);
        }

        // Transition actions
        for (i, trans) in self.spec.transitions.iter().enumerate() {
            if let Some(action) = &trans.action {
                let port_id = format!(
                    "trans_{}_{}_{i}",
                    sanitize_id(&trans.from),
                    sanitize_id(&trans.to)
                );
                let target_id = action_target_id(&action.target);
                let instance = ActionInstance {
                    port_id,
                    target_id: target_id.to_owned(),
                    is_capability: matches!(action.target, ActionTarget::Capability(_)),
                    args: map_to_json_map(&action.args),
                };
                self.actions.push(instance);
            }
        }
    }

    fn collect_state_actions(&mut self, state_name: &str, state: &StateSpec) {
        for (i, action) in state.entry.iter().enumerate() {
            let port_id = format!("entry_{}_{i}", sanitize_id(state_name));
            let target_id = action_target_id(&action.target);
            let instance = ActionInstance {
                port_id,
                target_id: target_id.to_owned(),
                is_capability: matches!(action.target, ActionTarget::Capability(_)),
                args: map_to_json_map(&action.args),
            };
            self.actions.push(instance);
        }
        for (i, action) in state.exit.iter().enumerate() {
            let port_id = format!("exit_{}_{i}", sanitize_id(state_name));
            let target_id = action_target_id(&action.target);
            let instance = ActionInstance {
                port_id,
                target_id: target_id.to_owned(),
                is_capability: matches!(action.target, ActionTarget::Capability(_)),
                args: map_to_json_map(&action.args),
            };
            self.actions.push(instance);
        }
    }

    /// 生成 stateMachine 节点。
    pub(super) fn build_state_machine_node(&mut self) {
        let mut states_config = Vec::new();
        for (state_name, state) in &self.spec.states {
            let entry_actions: Vec<String> = state
                .entry
                .iter()
                .enumerate()
                .map(|(i, _)| format!("entry_{}_{i}", sanitize_id(state_name)))
                .collect();
            let exit_actions: Vec<String> = state
                .exit
                .iter()
                .enumerate()
                .map(|(i, _)| format!("exit_{}_{i}", sanitize_id(state_name)))
                .collect();
            states_config.push(serde_json::json!({
                "name": state_name,
                "entry_actions": entry_actions,
                "exit_actions": exit_actions,
            }));
        }

        let mut transitions_config = Vec::new();
        for (i, trans) in self.spec.transitions.iter().enumerate() {
            let action_port = trans.action.as_ref().map(|_| {
                format!(
                    "trans_{}_{}_{i}",
                    sanitize_id(&trans.from),
                    sanitize_id(&trans.to)
                )
            });
            transitions_config.push(serde_json::json!({
                "from": trans.from,
                "to": trans.to,
                "when": trans.when,
                "priority": trans.priority.unwrap_or(0),
                "action_port": action_port,
            }));
        }

        let timeout_rules: Vec<Value> = self
            .spec
            .timeout
            .iter()
            .map(|(state, duration)| {
                serde_json::json!({
                    "state": state,
                    "timeout_ms": duration.millis,
                })
            })
            .collect();

        let config = serde_json::json!({
            "initial_state": self.initial_state,
            "states": states_config,
            "transitions": transitions_config,
            "timeout_rules": timeout_rules,
            "on_timeout_target": self.spec.on_timeout,
        });

        let node = serde_json::json!({
            "id": self.sm_node_id,
            "type": "stateMachine",
            "config": config,
            "buffer": 32,
        });

        self.nodes.insert(self.sm_node_id.clone(), node);
    }

    /// 为每个唯一 action 生成 capabilityCall 节点。
    pub(super) fn build_capability_call_nodes(&mut self) -> Result<(), CompileError> {
        for action_key in &self.actions {
            let node_id = sanitize_node_id(&format!(
                "cap_{}_{}",
                action_key.target_id, action_key.port_id
            ));

            let mut config = Map::new();

            if action_key.is_capability {
                let cap = self
                    .ctx
                    .capabilities
                    .get(&action_key.target_id)
                    .ok_or_else(|| CompileError::CapabilityCall {
                        detail: format!(
                            "能力 `{}` 在编译上下文中未找到（引用校验应已捕获）",
                            action_key.target_id
                        ),
                    })?;

                config.insert(
                    "capability_id".to_owned(),
                    Value::String(action_key.target_id.clone()),
                );
                config.insert("device_id".to_owned(), Value::String(cap.device_id.clone()));
                config.insert(
                    "implementation".to_owned(),
                    capability_impl_to_json(&cap.implementation),
                );
                config.insert("args".to_owned(), Value::Object(action_key.args.clone()));

                // 设置 connection_id：设备对应的连接 ID（设备未指定时留空，由运行时解析）
                let connection_id = self.ctx.connection_id_for_device(&cap.device_id);

                let mut node = serde_json::json!({
                    "id": node_id,
                    "type": "capabilityCall",
                    "config": Value::Object(config),
                    "buffer": 32,
                });
                if let Some(conn_id) = connection_id {
                    node["connection_id"] = Value::String(conn_id.to_owned());
                }
                self.nodes.insert(node_id.clone(), node);
            } else {
                return Err(CompileError::CapabilityCall {
                    detail: format!(
                        "系统动作 `{}` 尚未实现，不能生成可执行节点",
                        action_key.target_id
                    ),
                });
            }
        }
        Ok(())
    }

    /// 生成边：stateMachine 的 action port → capabilityCall 节点。
    pub(super) fn build_edges(&mut self) {
        for action_key in &self.actions {
            let target_node_id = sanitize_node_id(&format!(
                "cap_{}_{}",
                action_key.target_id, action_key.port_id
            ));
            let edge = serde_json::json!({
                "from": self.sm_node_id,
                "to": target_node_id,
                "source_port_id": action_key.port_id,
            });
            self.edges.push(edge);
        }
    }

    /// 生成变量：用户变量 + 内部状态跟踪变量。
    pub(super) fn build_variables(&mut self) {
        // 用户变量
        for (name, value) in &self.spec.variables {
            let var_type = infer_pin_type_json(value);
            self.variables.insert(
                name.clone(),
                serde_json::json!({
                    "type": var_type,
                    "initial": value,
                }),
            );
        }

        // 内部状态跟踪变量
        let state_var = format!("_sm.{}.current_state", self.sm_node_id);
        self.variables.insert(
            state_var,
            serde_json::json!({
                "type": { "kind": "string" },
                "initial": self.initial_state,
            }),
        );
    }

    /// 构建最终输出 JSON。
    pub(super) fn build_output(self) -> Value {
        serde_json::json!({
            "name": self.spec.id,
            "connections": [],
            "nodes": Value::Object(self.nodes),
            "edges": self.edges,
            "variables": Value::Object(self.variables),
        })
    }
}

// ---- 辅助函数 ----
