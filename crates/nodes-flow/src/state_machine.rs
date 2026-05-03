//! 状态机节点：管理内部状态转移，通过动态 output pin 触发下游 action DAG。
//!
//! 由 DSL 编译器生成 config，运行时评估 transition 条件（Rhai 表达式），
//! 匹配时触发 exit/entry/transition action 并通过 [`NodeDispatch::Route`] 路由。

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

use std::collections::HashSet;
use std::sync::Arc;

use nazh_core::{
    EmptyPolicy, EngineError, NodeExecution, NodeOutput, NodeTrait, PinDefinition, PinDirection,
    PinKind, PinType, WorkflowVariables,
};
use rhai::serde::to_dynamic;
use scripting::default_max_operations;

/// 状态机节点配置——由 DSL 编译器生成。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StateMachineConfig {
    pub initial_state: String,
    pub states: Vec<StateConfig>,
    pub transitions: Vec<TransitionConfig>,
    #[serde(default)]
    pub timeout_rules: Vec<TimeoutRule>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub on_timeout_target: Option<String>,
    #[serde(default = "default_max_operations")]
    pub max_operations: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StateConfig {
    pub name: String,
    #[serde(default)]
    pub entry_actions: Vec<String>,
    #[serde(default)]
    pub exit_actions: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransitionConfig {
    pub from: String,
    pub to: String,
    pub when: String,
    #[serde(default)]
    pub priority: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub action_port: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimeoutRule {
    pub state: String,
    pub timeout_ms: u64,
}

/// 状态机节点：管理内部状态转移。
pub struct StateMachineNode {
    id: String,
    config: StateMachineConfig,
    variables: Option<Arc<WorkflowVariables>>,
    /// 所有唯一 action port ID（用于动态 output pins）。
    action_ports: Vec<String>,
    /// 状态查找表：name → `StateConfig` index。
    state_map: Vec<(String, usize)>,
}

impl StateMachineNode {
    /// 创建状态机节点实例。
    ///
    /// # Errors
    ///
    /// Rhai 脚本编译失败时返回错误。
    #[allow(clippy::needless_pass_by_value)]
    pub fn new(
        id: impl Into<String>,
        config: StateMachineConfig,
        variables: Option<Arc<WorkflowVariables>>,
    ) -> Result<Self, EngineError> {
        let id_str = id.into();
        // 收集所有唯一 action port
        let mut seen = HashSet::new();
        let mut action_ports = Vec::new();
        for state in &config.states {
            for port in &state.entry_actions {
                if seen.insert(port.clone()) {
                    action_ports.push(port.clone());
                }
            }
            for port in &state.exit_actions {
                if seen.insert(port.clone()) {
                    action_ports.push(port.clone());
                }
            }
        }
        for trans in &config.transitions {
            if let Some(port) = &trans.action_port
                && seen.insert(port.clone())
            {
                action_ports.push(port.clone());
            }
        }

        let state_map: Vec<(String, usize)> = config
            .states
            .iter()
            .enumerate()
            .map(|(i, s)| (s.name.clone(), i))
            .collect();

        // 预编译 transition 条件表达式（验证语法）
        let engine = rhai::Engine::new();
        for trans in &config.transitions {
            engine.compile_expression(&trans.when).map_err(|e| {
                EngineError::script_compile(
                    &id_str,
                    format!(
                        "transition `{} → {}` 条件编译失败: {e}",
                        trans.from, trans.to
                    ),
                )
            })?;
        }

        Ok(Self {
            id: id_str,
            config,
            variables,
            action_ports,
            state_map,
        })
    }

    fn state_variable_key(&self) -> String {
        format!("_sm.{}.current_state", self.id)
    }

    fn read_current_state(&self) -> Result<String, EngineError> {
        let vars = self.variables.as_ref().ok_or_else(|| {
            EngineError::invalid_graph(format!(
                "状态机节点 `{}` 需要 WorkflowVariables 来读取当前状态",
                self.id
            ))
        })?;
        let val = vars.get_value(&self.state_variable_key()).ok_or_else(|| {
            EngineError::invalid_graph(format!(
                "状态机节点 `{}` 的状态变量 `{}` 未初始化",
                self.id,
                self.state_variable_key()
            ))
        })?;
        val.as_str().map(String::from).ok_or_else(|| {
            EngineError::payload_conversion(
                self.id.clone(),
                format!("状态变量值不是字符串: {val:?}"),
            )
        })
    }

    fn write_current_state(&self, new_state: &str) -> Result<(), EngineError> {
        let vars = self.variables.as_ref().ok_or_else(|| {
            EngineError::invalid_graph(format!(
                "状态机节点 `{}` 需要 WorkflowVariables 来写入状态",
                self.id
            ))
        })?;
        vars.set(
            &self.state_variable_key(),
            Value::String(new_state.to_owned()),
            None,
        )
        .map_err(|e| {
            EngineError::payload_conversion(self.id.clone(), format!("写入状态变量失败: {e}"))
        })
    }

    fn find_state(&self, name: &str) -> Option<&StateConfig> {
        self.state_map
            .iter()
            .find(|(n, _)| n == name)
            .and_then(|(_, idx)| self.config.states.get(*idx))
    }

    /// 评估 transition 条件，返回匹配的 transition（按优先级降序）。
    fn evaluate_transitions(
        &self,
        current_state: &str,
        payload: &Value,
    ) -> Result<Option<&TransitionConfig>, EngineError> {
        let engine = rhai::Engine::new();
        let mut scope = rhai::Scope::new();
        // 将 payload 转为 Rhai Dynamic 以支持属性访问（payload.start 等）
        let payload_dynamic = to_dynamic(payload.clone()).map_err(|e| {
            EngineError::payload_conversion(
                self.id.clone(),
                format!("payload 转 Rhai Dynamic 失败: {e}"),
            )
        })?;
        scope.push_dynamic("payload", payload_dynamic);

        // 按优先级降序排序后评估
        let mut sorted: Vec<&TransitionConfig> = self.config.transitions.iter().collect();
        sorted.sort_by_key(|b| std::cmp::Reverse(b.priority));

        for trans in sorted {
            // 检查 from 是否匹配当前状态或通配符
            if trans.from != "*" && trans.from != current_state {
                continue;
            }

            let result = engine
                .eval_with_scope::<bool>(&mut scope, &trans.when)
                .map_err(|e| {
                    EngineError::script_runtime(&self.id, format!("transition 条件求值失败: {e}"))
                })?;

            if result {
                return Ok(Some(trans));
            }
        }
        Ok(None)
    }
}

#[async_trait]
impl NodeTrait for StateMachineNode {
    fn id(&self) -> &str {
        &self.id
    }

    fn kind(&self) -> &'static str {
        "stateMachine"
    }

    /// 动态 output pins：每个唯一 action port 对应一个 Exec pin。
    fn output_pins(&self) -> Vec<PinDefinition> {
        self.action_ports
            .iter()
            .map(|port_id| PinDefinition {
                id: port_id.clone(),
                label: port_id.clone(),
                pin_type: PinType::Any,
                direction: PinDirection::Output,
                required: false,
                kind: PinKind::Exec,
                description: None,
                empty_policy: EmptyPolicy::default(),
                block_timeout_ms: None,
                ttl_ms: None,
            })
            .collect()
    }

    async fn transform(
        &self,
        _trace_id: Uuid,
        payload: Value,
    ) -> Result<NodeExecution, EngineError> {
        let current_state = self.read_current_state()?;

        // 评估 transition
        let matched = self.evaluate_transitions(&current_state, &payload)?;

        let Some(trans) = matched else {
            // 无匹配 transition → 广播 payload（保持 DAG 流动）
            return Ok(NodeExecution::broadcast(payload));
        };

        let target_state = &trans.to;
        let from_state = current_state.clone();

        // 收集需要触发的 action ports
        let mut ports = Vec::new();

        // 1. 当前状态的 exit actions
        if let Some(from) = self.find_state(&from_state) {
            ports.extend(from.exit_actions.iter().cloned());
        }

        // 2. transition action（如果有）
        if let Some(action_port) = &trans.action_port {
            ports.push(action_port.clone());
        }

        // 3. 更新状态
        self.write_current_state(target_state)?;

        // 4. 目标状态的 entry actions
        if let Some(to) = self.find_state(target_state) {
            ports.extend(to.entry_actions.iter().cloned());
        }

        if ports.is_empty() {
            // 状态转移但无 action → 广播
            let mut metadata = serde_json::Map::new();
            metadata.insert(
                "state_machine".to_owned(),
                serde_json::json!({
                    "from_state": from_state,
                    "to_state": target_state,
                }),
            );
            return Ok(NodeExecution::from_outputs(vec![NodeOutput {
                payload,
                metadata: Some(metadata),
                dispatch: nazh_core::NodeDispatch::Broadcast,
            }]));
        }

        // 路由到 action ports
        let mut metadata = serde_json::Map::new();
        metadata.insert(
            "state_machine".to_owned(),
            serde_json::json!({
                "from_state": from_state,
                "to_state": target_state,
                "matched_transition": trans.when,
            }),
        );

        Ok(NodeExecution::from_outputs(vec![NodeOutput {
            payload,
            metadata: Some(metadata),
            dispatch: nazh_core::NodeDispatch::Route(ports),
        }]))
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn build_config() -> StateMachineConfig {
        StateMachineConfig {
            initial_state: "idle".to_owned(),
            states: vec![
                StateConfig {
                    name: "idle".to_owned(),
                    entry_actions: vec![],
                    exit_actions: vec![],
                },
                StateConfig {
                    name: "running".to_owned(),
                    entry_actions: vec!["entry_running_0".to_owned()],
                    exit_actions: vec!["exit_running_0".to_owned()],
                },
                StateConfig {
                    name: "fault".to_owned(),
                    entry_actions: vec!["entry_fault_0".to_owned()],
                    exit_actions: vec![],
                },
            ],
            transitions: vec![
                TransitionConfig {
                    from: "idle".to_owned(),
                    to: "running".to_owned(),
                    when: "payload.start == true".to_owned(),
                    priority: 0,
                    action_port: None,
                },
                TransitionConfig {
                    from: "running".to_owned(),
                    to: "idle".to_owned(),
                    when: "payload.done == true".to_owned(),
                    priority: 0,
                    action_port: None,
                },
                TransitionConfig {
                    from: "*".to_owned(),
                    to: "fault".to_owned(),
                    when: "payload.error == true".to_owned(),
                    priority: 100,
                    action_port: Some("trans_any_fault".to_owned()),
                },
            ],
            timeout_rules: vec![],
            on_timeout_target: None,
            max_operations: 50_000,
        }
    }

    fn build_vars(initial_state: &str) -> Arc<WorkflowVariables> {
        use nazh_core::VariableDeclaration;
        let decls: HashMap<String, nazh_core::VariableDeclaration> = [(
            "_sm.sm_test.current_state".to_owned(),
            VariableDeclaration {
                variable_type: PinType::String,
                initial: Value::String(initial_state.to_owned()),
            },
        )]
        .into_iter()
        .collect();
        Arc::new(WorkflowVariables::from_declarations(&decls).unwrap())
    }

    #[tokio::test]
    async fn idle_to_running_触发_entry_action() {
        let config = build_config();
        let vars = build_vars("idle");
        let node = StateMachineNode::new("sm_test", config, Some(vars.clone())).unwrap();

        let payload = serde_json::json!({ "start": true });
        let result = node.transform(Uuid::new_v4(), payload).await.unwrap();

        // 验证路由到 running 的 entry action
        let output = &result.outputs[0];
        assert!(
            matches!(output.dispatch, nazh_core::NodeDispatch::Route(ref ports) if ports.contains(&"entry_running_0".to_owned()))
        );

        // 验证状态更新
        assert_eq!(
            vars.get_value("_sm.sm_test.current_state").unwrap(),
            "running"
        );

        // 验证 metadata
        let sm_meta = output
            .metadata
            .as_ref()
            .unwrap()
            .get("state_machine")
            .unwrap();
        assert_eq!(sm_meta["from_state"], "idle");
        assert_eq!(sm_meta["to_state"], "running");
    }

    #[tokio::test]
    async fn running_to_idle_触发_exit_action() {
        let config = build_config();
        let vars = build_vars("running");
        let node = StateMachineNode::new("sm_test", config, Some(vars.clone())).unwrap();

        let payload = serde_json::json!({ "done": true });
        let result = node.transform(Uuid::new_v4(), payload).await.unwrap();

        let output = &result.outputs[0];
        assert!(
            matches!(output.dispatch, nazh_core::NodeDispatch::Route(ref ports) if ports.contains(&"exit_running_0".to_owned()))
        );
        assert_eq!(vars.get_value("_sm.sm_test.current_state").unwrap(), "idle");
    }

    #[tokio::test]
    async fn 通配符fault_transition_高优先级匹配() {
        let config = build_config();
        let vars = build_vars("running");
        let node = StateMachineNode::new("sm_test", config, Some(vars.clone())).unwrap();

        let payload = serde_json::json!({ "error": true });
        let result = node.transform(Uuid::new_v4(), payload).await.unwrap();

        let output = &result.outputs[0];
        assert!(
            matches!(output.dispatch, nazh_core::NodeDispatch::Route(ref ports) if ports.contains(&"exit_running_0".to_owned()) && ports.contains(&"trans_any_fault".to_owned()) && ports.contains(&"entry_fault_0".to_owned()))
        );
        assert_eq!(
            vars.get_value("_sm.sm_test.current_state").unwrap(),
            "fault"
        );
    }

    #[tokio::test]
    async fn 无匹配transition_广播payload() {
        let config = build_config();
        let vars = build_vars("idle");
        let node = StateMachineNode::new("sm_test", config, Some(vars.clone())).unwrap();

        let payload = serde_json::json!({ "unknown": true });
        let result = node.transform(Uuid::new_v4(), payload).await.unwrap();

        let output = &result.outputs[0];
        assert!(matches!(
            output.dispatch,
            nazh_core::NodeDispatch::Broadcast
        ));
        // 状态不变
        assert_eq!(vars.get_value("_sm.sm_test.current_state").unwrap(), "idle");
    }

    #[test]
    fn output_pins_包含所有唯一action_port() {
        let config = build_config();
        let node = StateMachineNode::new("sm_test", config, None).unwrap();
        let pins = node.output_pins();
        let ids: Vec<&str> = pins.iter().map(|p| p.id.as_str()).collect();

        assert!(ids.contains(&"entry_running_0"));
        assert!(ids.contains(&"exit_running_0"));
        assert!(ids.contains(&"entry_fault_0"));
        assert!(ids.contains(&"trans_any_fault"));
        assert_eq!(ids.len(), 4); // 无重复
    }

    #[test]
    fn config_从_json_解析成功() {
        let json = serde_json::json!({
            "initial_state": "idle",
            "states": [
                { "name": "idle", "entry_actions": [], "exit_actions": [] },
                { "name": "running", "entry_actions": ["act1"], "exit_actions": [] }
            ],
            "transitions": [
                { "from": "idle", "to": "running", "when": "true", "priority": 0 }
            ],
            "timeout_rules": [],
            "max_operations": 50000
        });
        let config: StateMachineConfig = serde_json::from_value(json).unwrap();
        assert_eq!(config.initial_state, "idle");
        assert_eq!(config.states.len(), 2);
        assert_eq!(config.transitions.len(), 1);
    }
}
