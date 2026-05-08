//! 编译器核心：`WorkflowSpec` + `CompilerContext` → `WorkflowGraph` JSON。

use nazh_dsl_core::capability::CapabilityImpl;
use nazh_dsl_core::workflow::{ActionSpec, ActionTarget, StateSpec, WorkflowSpec};
use serde_json::{Map, Value};

use crate::context::CompilerContext;
use crate::error::CompileError;
use crate::safety::{SafetyReport, run_safety_checks};
use crate::validate::{determine_initial_state, validate_workflow_spec};

/// 将 `WorkflowSpec` 编译为符合 `WorkflowGraph` serde 契约的 JSON。
///
/// 编译流程：
/// 1. 引用校验（设备/能力存在性）
/// 2. 语义校验（状态机约束）
/// 3. 收集所有唯一 action → 生成 capabilityCall 节点
/// 4. 生成 stateMachine 节点
/// 5. 生成边（stateMachine → capabilityCall）
/// 6. 生成变量（用户变量 + 内部状态跟踪变量）
///
/// # Errors
///
/// 引用缺失、语义校验失败或 JSON 构建错误时返回 [`CompileError`]。
pub fn compile(ctx: &CompilerContext, spec: &WorkflowSpec) -> Result<Value, CompileError> {
    ctx.validate_references(spec)?;
    validate_workflow_spec(spec)?;
    validate_supported_runtime_features(spec)?;
    let initial_state = determine_initial_state(spec)?;

    let mut builder = GraphBuilder::new(ctx, spec, &initial_state);
    builder.collect_actions();
    builder.build_state_machine_node();
    builder.build_capability_call_nodes()?;
    builder.build_edges();
    builder.build_variables();
    Ok(builder.build_output())
}

/// 编译 `WorkflowSpec` 并同时执行安全编译器校验（RFC-0004 Phase 5）。
///
/// 与 [`compile`] 相同的编译流程，额外在引用校验和语义校验成功后
/// 运行安全编译器 6 条规则。安全诊断通过 [`SafetyReport`] 暴露。
///
/// 安全错误（`DiagnosticLevel::Error`）阻止编译产出 `WorkflowGraph` JSON。
/// 安全警告（`DiagnosticLevel::Warning`）不阻止编译。
pub fn compile_with_safety(
    ctx: &CompilerContext,
    spec: &WorkflowSpec,
) -> Result<(Value, SafetyReport), CompileError> {
    ctx.validate_references(spec)?;
    validate_workflow_spec(spec)?;
    validate_supported_runtime_features(spec)?;
    let initial_state = determine_initial_state(spec)?;

    // 安全编译器校验
    let safety_report = run_safety_checks(ctx, spec, &initial_state);

    // 安全错误阻止编译
    if safety_report.has_errors() {
        let error_count = safety_report.errors().count();
        return Err(CompileError::CapabilityCall {
            detail: format!("安全编译器校验失败，共 {error_count} 个错误"),
        });
    }

    // 继续正常编译
    let mut builder = GraphBuilder::new(ctx, spec, &initial_state);
    builder.collect_actions();
    builder.build_state_machine_node();
    builder.build_capability_call_nodes()?;
    builder.build_edges();
    builder.build_variables();
    Ok((builder.build_output(), safety_report))
}

// ---- 内部构建器 ----

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

struct GraphBuilder<'a> {
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
    fn new(ctx: &'a CompilerContext, spec: &'a WorkflowSpec, initial_state: &'a str) -> Self {
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
    fn collect_actions(&mut self) {
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
    fn build_state_machine_node(&mut self) {
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
    fn build_capability_call_nodes(&mut self) -> Result<(), CompileError> {
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
    fn build_edges(&mut self) {
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
    fn build_variables(&mut self) {
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
    fn build_output(self) -> Value {
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

/// 将 `CapabilityImpl` 映射为编译器输出的 JSON 片段。
fn capability_impl_to_json(impl_: &CapabilityImpl) -> Value {
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
fn infer_pin_type_json(value: &Value) -> Value {
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
fn action_target_id(target: &ActionTarget) -> &str {
    match target {
        ActionTarget::Capability(id) | ActionTarget::Action(id) => id,
    }
}

/// 将 `HashMap<String, Value>` 转为 `serde_json::Map`。
fn map_to_json_map(map: &std::collections::HashMap<String, Value>) -> Map<String, Value> {
    let mut result = Map::new();
    for (k, v) in map {
        result.insert(k.clone(), v.clone());
    }
    result
}

/// 将任意字符串转换为合法的节点 ID（替换不安全字符为 `_`）。
fn sanitize_node_id(s: &str) -> String {
    s.replace(['.', '-', ' '], "_")
}

/// 将状态/端口名称中的特殊字符替换。
fn sanitize_id(s: &str) -> String {
    s.replace(['.', '-', ' '], "_")
}

/// 拒绝编译当前运行时尚未闭环的 Workflow DSL 特性。
fn validate_supported_runtime_features(spec: &WorkflowSpec) -> Result<(), CompileError> {
    if !spec.timeout.is_empty() || spec.on_timeout.is_some() {
        return Err(CompileError::StateMachine {
            detail: "timeout/on_timeout 已建模但 stateMachine 运行时尚未实现超时触发".to_owned(),
        });
    }

    for state in spec.states.values() {
        validate_actions_supported(&state.entry)?;
        validate_actions_supported(&state.exit)?;
    }
    for trans in &spec.transitions {
        validate_transition_condition_supported(trans)?;
        if let Some(action) = &trans.action {
            validate_action_supported(action)?;
        }
    }

    Ok(())
}

fn validate_actions_supported(actions: &[ActionSpec]) -> Result<(), CompileError> {
    for action in actions {
        validate_action_supported(action)?;
    }
    Ok(())
}

fn validate_action_supported(action: &ActionSpec) -> Result<(), CompileError> {
    if let ActionTarget::Action(id) = &action.target {
        return Err(CompileError::CapabilityCall {
            detail: format!("系统动作 `{id}` 尚未实现，不能生成可执行节点"),
        });
    }
    Ok(())
}

fn validate_transition_condition_supported(
    trans: &nazh_dsl_core::workflow::TransitionSpec,
) -> Result<(), CompileError> {
    let without_strings = strip_string_literals(&trans.when);
    let without_payload_paths = strip_payload_paths(&without_strings);
    let bare_identifiers = extract_bare_identifiers(&without_payload_paths);

    if bare_identifiers.is_empty() {
        return Ok(());
    }

    Err(CompileError::StateMachine {
        detail: format!(
            "transition `{} -> {}` 的条件 `{}` 引用了裸变量 `{}`；stateMachine 运行时只注入 `payload`，请改用 `payload.{}`",
            trans.from, trans.to, trans.when, bare_identifiers[0], bare_identifiers[0]
        ),
    })
}

fn strip_string_literals(expr: &str) -> String {
    let mut result = String::with_capacity(expr.len());
    let mut quote: Option<char> = None;
    let mut escaped = false;

    for ch in expr.chars() {
        match quote {
            Some(q) => {
                result.push(' ');
                if escaped {
                    escaped = false;
                } else if ch == '\\' {
                    escaped = true;
                } else if ch == q {
                    quote = None;
                }
            }
            None if ch == '"' || ch == '\'' => {
                quote = Some(ch);
                result.push(' ');
            }
            None => result.push(ch),
        }
    }

    result
}

fn strip_payload_paths(expr: &str) -> String {
    let chars: Vec<char> = expr.chars().collect();
    let mut result = String::with_capacity(expr.len());
    let mut i = 0;

    while i < chars.len() {
        if starts_payload_path(&chars, i) {
            let end = payload_path_end(&chars, i + "payload".chars().count());
            for _ in i..end {
                result.push(' ');
            }
            i = end;
        } else {
            result.push(chars[i]);
            i += 1;
        }
    }

    result
}

fn starts_payload_path(chars: &[char], start: usize) -> bool {
    let payload: Vec<char> = "payload".chars().collect();
    if start > 0 && is_identifier_continue(chars[start - 1]) {
        return false;
    }
    if chars.len() < start + payload.len() {
        return false;
    }
    if chars[start..start + payload.len()] != payload {
        return false;
    }

    let after = start + payload.len();
    after < chars.len() && chars[after] == '.'
}

fn payload_path_end(chars: &[char], mut i: usize) -> usize {
    while i < chars.len() && chars[i] == '.' {
        let next = i + 1;
        if next >= chars.len() || !is_identifier_start(chars[next]) {
            break;
        }
        i = next + 1;
        while i < chars.len() && is_identifier_continue(chars[i]) {
            i += 1;
        }
    }
    i
}

fn extract_bare_identifiers(expr: &str) -> Vec<String> {
    let chars: Vec<char> = expr.chars().collect();
    let mut identifiers = Vec::new();
    let mut i = 0;

    while i < chars.len() {
        if is_identifier_start(chars[i]) {
            let start = i;
            i += 1;
            while i < chars.len() && is_identifier_continue(chars[i]) {
                i += 1;
            }
            let ident: String = chars[start..i].iter().collect();
            if !is_condition_reserved_word(&ident) && ident.parse::<f64>().is_err() {
                identifiers.push(ident);
            }
        } else {
            i += 1;
        }
    }

    identifiers
}

fn is_identifier_start(ch: char) -> bool {
    ch == '_' || ch.is_ascii_alphabetic()
}

fn is_identifier_continue(ch: char) -> bool {
    ch == '_' || ch.is_ascii_alphanumeric()
}

fn is_condition_reserved_word(s: &str) -> bool {
    matches!(
        s,
        "true"
            | "false"
            | "payload"
            | "let"
            | "if"
            | "else"
            | "and"
            | "or"
            | "not"
            | "while"
            | "loop"
            | "for"
            | "in"
            | "return"
            | "fn"
            | "import"
            | "export"
    )
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::unwrap_used)]
mod tests {
    use super::*;
    use nazh_dsl_core::capability::{
        CapabilityImpl, CapabilitySpec, SafetyConstraints, SafetyLevel,
    };
    use nazh_dsl_core::device::{ConnectionRef, DeviceSpec};
    use nazh_dsl_core::workflow::WorkflowSpec;

    fn sample_device(id: &str, conn_id: &str) -> DeviceSpec {
        DeviceSpec {
            id: id.to_owned(),
            device_type: "test".to_owned(),
            manufacturer: None,
            model: None,
            connection: Some(ConnectionRef {
                connection_type: "modbus-tcp".to_owned(),
                id: conn_id.to_owned(),
                unit: Some(1),
            }),
            network_group: None,
            signals: vec![],
            alarms: vec![],
        }
    }

    fn sample_capability_modbus(id: &str, device_id: &str, register: u16) -> CapabilitySpec {
        CapabilitySpec {
            id: id.to_owned(),
            device_id: device_id.to_owned(),
            description: String::new(),
            inputs: vec![],
            outputs: vec![],
            preconditions: vec![],
            effects: vec![],
            implementation: CapabilityImpl::ModbusWrite {
                register,
                value: "${value}".to_owned(),
            },
            fallback: vec![],
            safety: SafetyConstraints {
                level: SafetyLevel::Low,
                requires_approval: false,
                max_execution_time: None,
            },
        }
    }

    fn sample_capability_script(id: &str, device_id: &str) -> CapabilitySpec {
        CapabilitySpec {
            id: id.to_owned(),
            device_id: device_id.to_owned(),
            description: String::new(),
            inputs: vec![],
            outputs: vec![],
            preconditions: vec![],
            effects: vec![],
            implementation: CapabilityImpl::Script {
                content: "pass".to_owned(),
            },
            fallback: vec![],
            safety: SafetyConstraints {
                level: SafetyLevel::Low,
                requires_approval: false,
                max_execution_time: None,
            },
        }
    }

    #[test]
    fn 最小工作流_编译成功() {
        let yaml = r#"
id: minimal
version: "1.0.0"
devices:
  - dev1
states:
  idle:
  running:
transitions:
  - from: idle
    to: running
    when: "payload.start == true"
"#;
        let spec: WorkflowSpec = serde_yaml::from_str(yaml).unwrap();
        let ctx = CompilerContext::new(vec![sample_device("dev1", "conn1")], vec![]);
        let output = compile(&ctx, &spec).unwrap();

        // 验证基本结构
        assert_eq!(output["name"], "minimal");
        assert!(output["connections"].as_array().is_some_and(Vec::is_empty));
        assert!(output["nodes"].is_object());
        assert!(output["edges"].is_array());
        assert!(output["variables"].is_object());

        // 只有 stateMachine 节点（无 action）
        assert!(
            output["nodes"]
                .as_object()
                .unwrap()
                .contains_key("sm_minimal")
        );
        // 无边（没有 action）
        assert!(output["edges"].as_array().unwrap().is_empty());
        // 有内部状态变量
        let vars = output["variables"].as_object().unwrap();
        assert!(vars.contains_key("_sm.sm_minimal.current_state"));
        assert_eq!(vars["_sm.sm_minimal.current_state"]["initial"], "idle");
    }

    #[test]
    fn 带capability调用的工作流_编译成功() {
        let yaml = r#"
id: test_wf
version: "1.0.0"
devices:
  - dev1
variables:
  target_pressure: 25.0
  mode: "auto"
states:
  idle:
  pressing:
    entry:
      - capability: cap.press
        args:
          target: "${target_pressure}"
transitions:
  - from: idle
    to: pressing
    when: "payload.start == true"
"#;
        let spec: WorkflowSpec = serde_yaml::from_str(yaml).unwrap();
        let ctx = CompilerContext::new(
            vec![sample_device("dev1", "conn1")],
            vec![sample_capability_modbus("cap.press", "dev1", 40010)],
        );
        let output = compile(&ctx, &spec).unwrap();

        // stateMachine 节点
        let nodes = output["nodes"].as_object().unwrap();
        assert!(nodes.contains_key("sm_test_wf"));

        // capabilityCall 节点
        let cap_node_key = nodes
            .keys()
            .find(|k| k.starts_with("cap_cap_press"))
            .expect("应有 capabilityCall 节点");
        let cap_node = &nodes[cap_node_key];
        assert_eq!(cap_node["type"], "capabilityCall");
        assert_eq!(cap_node["connection_id"], "conn1");
        assert_eq!(cap_node["config"]["capability_id"], "cap.press");
        assert_eq!(cap_node["config"]["implementation"]["type"], "modbus-write");

        // 边
        let edges = output["edges"].as_array().unwrap();
        assert_eq!(edges.len(), 1);
        assert_eq!(edges[0]["from"], "sm_test_wf");
        assert_eq!(edges[0]["source_port_id"], "entry_pressing_0");

        // 用户变量
        let vars = output["variables"].as_object().unwrap();
        assert_eq!(vars["target_pressure"]["type"]["kind"], "float");
        assert_eq!(vars["target_pressure"]["initial"], 25.0);
        assert_eq!(vars["mode"]["type"]["kind"], "string");
        assert_eq!(vars["mode"]["initial"], "auto");
    }

    #[test]
    fn 同一capability多次调用_保留各自动作参数() {
        let yaml = r#"
id: repeated_capability_args
version: "1.0.0"
devices:
  - dev1
variables:
  approach_position: 100.0
states:
  idle:
  approaching:
    entry:
      - capability: cap.move_to
        args:
          position: "${approach_position}"
  returning:
    entry:
      - capability: cap.move_to
        args:
          position: 0.0
transitions:
  - from: idle
    to: approaching
    when: "payload.start == true"
  - from: approaching
    to: returning
    when: "payload.done == true"
"#;
        let spec: WorkflowSpec = serde_yaml::from_str(yaml).unwrap();
        let ctx = CompilerContext::new(
            vec![sample_device("dev1", "conn1")],
            vec![sample_capability_modbus("cap.move_to", "dev1", 40010)],
        );
        let output = compile(&ctx, &spec).unwrap();
        let nodes = output["nodes"].as_object().unwrap();

        let approaching = nodes
            .get("cap_cap_move_to_entry_approaching_0")
            .expect("应生成 approaching entry 节点");
        let returning = nodes
            .get("cap_cap_move_to_entry_returning_0")
            .expect("应生成 returning entry 节点");

        assert_eq!(
            approaching["config"]["args"]["position"],
            "${approach_position}"
        );
        assert_eq!(returning["config"]["args"]["position"], 0.0);
    }

    #[test]
    fn timeout未实现时_编译期拒绝() {
        let yaml = r#"
id: timeout_not_supported
version: "1.0.0"
states:
  idle:
  pressing:
  fault:
transitions:
  - from: idle
    to: pressing
    when: "payload.start == true"
timeout:
  pressing: 60s
on_timeout: fault
"#;
        let spec: WorkflowSpec = serde_yaml::from_str(yaml).unwrap();
        let ctx = CompilerContext::new(vec![], vec![]);

        let err = compile(&ctx, &spec).expect_err("timeout 运行时未实现时应拒绝编译");

        assert!(err.to_string().contains("timeout"));
    }

    #[test]
    fn 裸变量条件_编译期拒绝() {
        let yaml = r#"
id: bare_condition
version: "1.0.0"
states:
  idle:
  running:
transitions:
  - from: idle
    to: running
    when: "start_button == true"
"#;
        let spec: WorkflowSpec = serde_yaml::from_str(yaml).unwrap();
        let ctx = CompilerContext::new(vec![], vec![]);

        let err = compile(&ctx, &spec).expect_err("裸变量条件应在编译期被拒绝");

        assert!(err.to_string().contains("payload"));
    }

    #[test]
    fn system_action未实现时_编译期拒绝() {
        let yaml = r#"
id: system_action_not_supported
version: "1.0.0"
states:
  idle:
  fault:
    entry:
      - action: alarm.raise
        args:
          msg: "error"
transitions:
  - from: idle
    to: fault
    when: "payload.fault == true"
"#;
        let spec: WorkflowSpec = serde_yaml::from_str(yaml).unwrap();
        let ctx = CompilerContext::new(vec![], vec![]);

        let err = compile(&ctx, &spec).expect_err("system action 未实现时应拒绝编译");

        assert!(err.to_string().contains("alarm.raise"));
    }

    #[test]
    fn 混合capability和system_action调用_拒绝未实现动作() {
        let yaml = r#"
id: mixed
version: "1.0.0"
devices:
  - dev1
states:
  idle:
  fault:
    entry:
      - capability: cap.stop
      - action: alarm.raise
        args:
          msg: "error"
transitions:
  - from: idle
    to: fault
    when: "true"
"#;
        let spec: WorkflowSpec = serde_yaml::from_str(yaml).unwrap();
        let ctx = CompilerContext::new(
            vec![sample_device("dev1", "conn1")],
            vec![sample_capability_script("cap.stop", "dev1")],
        );
        let err = compile(&ctx, &spec).expect_err("system action 未实现时应拒绝编译");

        assert!(err.to_string().contains("alarm.raise"));
    }

    #[test]
    fn 变量类型推断() {
        let yaml = r#"
id: types_test
version: "1.0.0"
variables:
  float_var: 3.14
  int_var: 42
  str_var: "hello"
  bool_var: true
states:
  idle:
"#;
        let spec: WorkflowSpec = serde_yaml::from_str(yaml).unwrap();
        let ctx = CompilerContext::new(vec![], vec![]);
        let output = compile(&ctx, &spec).unwrap();

        let vars = output["variables"].as_object().unwrap();
        assert_eq!(vars["float_var"]["type"]["kind"], "float");
        assert_eq!(vars["int_var"]["type"]["kind"], "integer");
        assert_eq!(vars["str_var"]["type"]["kind"], "string");
        assert_eq!(vars["bool_var"]["type"]["kind"], "bool");
    }

    #[test]
    fn sanitize_node_id_替换特殊字符() {
        assert_eq!(sanitize_node_id("a.b-c d"), "a_b_c_d");
    }
}
