use nazh_dsl_core::workflow::{ActionSpec, ActionTarget, WorkflowSpec};
use serde_json::{Map, Value};

use crate::error::CompileError;

use super::json::action_target_id;

/// 将任意字符串转换为合法的节点 ID（替换不安全字符为 `_`）。
pub(super) fn sanitize_node_id(s: &str) -> String {
    s.replace(['.', '-', ' '], "_")
}

/// 将状态/端口名称中的特殊字符替换。
pub(super) fn sanitize_id(s: &str) -> String {
    s.replace(['.', '-', ' '], "_")
}

/// 拒绝编译当前运行时尚未闭环的 Workflow DSL 特性。
pub(super) fn validate_supported_runtime_features(spec: &WorkflowSpec) -> Result<(), CompileError> {
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

pub(super) fn validate_sanitized_ids(spec: &WorkflowSpec) -> Result<(), CompileError> {
    let mut port_origins = Map::new();
    let mut node_origins = Map::new();

    for (state_name, state) in &spec.states {
        for (i, action) in state.entry.iter().enumerate() {
            let port_id = format!("entry_{}_{i}", sanitize_id(state_name));
            let origin = format!("state `{state_name}` entry action {i}");
            insert_unique_sanitized_id(&mut port_origins, &port_id, origin.clone())?;
            insert_action_node_origin(&mut node_origins, action, &port_id, &origin)?;
        }
        for (i, action) in state.exit.iter().enumerate() {
            let port_id = format!("exit_{}_{i}", sanitize_id(state_name));
            let origin = format!("state `{state_name}` exit action {i}");
            insert_unique_sanitized_id(&mut port_origins, &port_id, origin.clone())?;
            insert_action_node_origin(&mut node_origins, action, &port_id, &origin)?;
        }
    }

    for (i, trans) in spec.transitions.iter().enumerate() {
        if let Some(action) = &trans.action {
            let port_id = format!(
                "trans_{}_{}_{i}",
                sanitize_id(&trans.from),
                sanitize_id(&trans.to)
            );
            let origin = format!("transition `{}` -> `{}` action {i}", trans.from, trans.to);
            insert_unique_sanitized_id(&mut port_origins, &port_id, origin.clone())?;
            insert_action_node_origin(&mut node_origins, action, &port_id, &origin)?;
        }
    }

    Ok(())
}

fn insert_action_node_origin(
    node_origins: &mut Map<String, Value>,
    action: &ActionSpec,
    port_id: &str,
    origin: &str,
) -> Result<(), CompileError> {
    let target_id = action_target_id(&action.target);
    let node_id = sanitize_node_id(&format!("cap_{target_id}_{port_id}"));
    insert_unique_sanitized_id(
        node_origins,
        &node_id,
        format!("{origin}, target `{target_id}`"),
    )
}

fn insert_unique_sanitized_id(
    origins: &mut Map<String, Value>,
    sanitized: &str,
    origin: String,
) -> Result<(), CompileError> {
    if let Some(existing) = origins.get(sanitized).and_then(Value::as_str) {
        return Err(CompileError::OutputBuild {
            detail: format!("sanitize 后 ID `{sanitized}` 发生碰撞：{existing} 与 {origin}"),
        });
    }
    origins.insert(sanitized.to_owned(), Value::String(origin));
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
