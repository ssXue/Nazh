//! `WorkflowSpec` 语义校验：状态存在性、transition 有效性、timeout 校验。

use std::collections::HashSet;

use nazh_dsl_core::workflow::WorkflowSpec;

use crate::error::CompileError;

/// 对 `WorkflowSpec` 执行状态机语义校验。
///
/// 校验规则：
/// 1. 至少一个状态定义
/// 2. 所有 transition 的 `from`/`to` 引用存在的状态（`"*"` 通配符例外）
/// 3. 通配符 transition 必须声明 `priority`
/// 4. timeout 键引用存在的状态
/// 5. `on_timeout` 引用存在的状态
/// 6. 无重复 transition（`from`/`to`/`when` 三元组）
pub fn validate_workflow_spec(spec: &WorkflowSpec) -> Result<(), CompileError> {
    let mut errors = Vec::new();

    // 规则 1：至少一个状态
    if spec.states.is_empty() {
        errors.push("工作流必须定义至少一个状态".to_owned());
    }

    let state_names: HashSet<&str> = spec.states.keys().map(String::as_str).collect();

    // 规则 2：transition 的 from/to 引用存在的状态
    let mut seen_transitions: HashSet<(String, String, String)> = HashSet::new();
    for (i, trans) in spec.transitions.iter().enumerate() {
        if trans.from != "*" && !state_names.contains(trans.from.as_str()) {
            errors.push(format!(
                "transition[{i}] 的 from 状态 `{}` 不存在",
                trans.from
            ));
        }
        if !state_names.contains(trans.to.as_str()) {
            errors.push(format!("transition[{i}] 的 to 状态 `{}` 不存在", trans.to));
        }

        // 规则 3：通配符 transition 必须声明 priority
        if trans.from == "*" && trans.priority.is_none() {
            errors.push(format!(
                "transition[{i}] 使用通配符 from=\"*\"，必须声明 priority"
            ));
        }

        // 规则 6：无重复 transition
        let key = (trans.from.clone(), trans.to.clone(), trans.when.clone());
        if !seen_transitions.insert(key) {
            errors.push(format!(
                "transition[{i}] 与已有 transition 重复 (from={}, to={}, when={})",
                trans.from, trans.to, trans.when
            ));
        }
    }

    // 规则 4：timeout 键引用存在的状态
    for state_name in spec.timeout.keys() {
        if !state_names.contains(state_name.as_str()) {
            errors.push(format!("timeout 中引用的状态 `{state_name}` 不存在"));
        }
    }

    // 规则 5：on_timeout 引用存在的状态
    if let Some(on_timeout) = &spec.on_timeout
        && !state_names.contains(on_timeout.as_str())
    {
        errors.push(format!("on_timeout 引用的状态 `{on_timeout}` 不存在"));
    }

    if errors.is_empty() {
        Ok(())
    } else {
        Err(CompileError::StateMachine {
            detail: errors.join("；"),
        })
    }
}

/// 确定初始状态：优先选择名为 `"idle"` 的状态，否则选择字典序首个状态。
///
/// # Errors
///
/// 当 `spec.states` 为空时返回错误。
pub fn determine_initial_state(spec: &WorkflowSpec) -> Result<String, CompileError> {
    if spec.states.is_empty() {
        return Err(CompileError::StateMachine {
            detail: "工作流没有定义任何状态".to_owned(),
        });
    }
    if spec.states.contains_key("idle") {
        return Ok("idle".to_owned());
    }
    // states 已检查非空，keys().min() 一定返回 Some
    let first = spec
        .states
        .keys()
        .min()
        .unwrap_or_else(|| unreachable!("states 非空")); // states 已检查非空
    Ok(first.clone())
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    fn parse_spec(yaml: &str) -> WorkflowSpec {
        serde_yaml::from_str(yaml).unwrap()
    }

    #[test]
    fn 合法的最小_spec_通过校验() {
        let spec = parse_spec(
            r#"
id: test
version: "1.0.0"
states:
  idle:
  running:
transitions:
  - from: idle
    to: running
    when: "true"
"#,
        );
        assert!(validate_workflow_spec(&spec).is_ok());
    }

    #[test]
    fn 无状态定义报错() {
        let spec = parse_spec(
            r#"
id: test
version: "1.0.0"
states: {}
"#,
        );
        let err = validate_workflow_spec(&spec).unwrap_err();
        assert!(err.to_string().contains("至少一个状态"));
    }

    #[test]
    fn transition_from_不存在的状态报错() {
        let spec = parse_spec(
            r#"
id: test
version: "1.0.0"
states:
  idle:
transitions:
  - from: nonexistent
    to: idle
    when: "true"
"#,
        );
        let err = validate_workflow_spec(&spec).unwrap_err();
        assert!(err.to_string().contains("nonexistent"));
    }

    #[test]
    fn transition_to_不存在的状态报错() {
        let spec = parse_spec(
            r#"
id: test
version: "1.0.0"
states:
  idle:
transitions:
  - from: idle
    to: nonexistent
    when: "true"
"#,
        );
        let err = validate_workflow_spec(&spec).unwrap_err();
        assert!(err.to_string().contains("nonexistent"));
    }

    #[test]
    fn 通配符transition_无_priority_报错() {
        let spec = parse_spec(
            r#"
id: test
version: "1.0.0"
states:
  idle:
  fault:
transitions:
  - from: "*"
    to: fault
    when: "error"
"#,
        );
        let err = validate_workflow_spec(&spec).unwrap_err();
        assert!(err.to_string().contains("priority"));
    }

    #[test]
    fn 通配符transition_有_priority_通过() {
        let spec = parse_spec(
            r#"
id: test
version: "1.0.0"
states:
  idle:
  fault:
transitions:
  - from: "*"
    to: fault
    when: "error"
    priority: 100
"#,
        );
        assert!(validate_workflow_spec(&spec).is_ok());
    }

    #[test]
    fn timeout_引用不存在的状态报错() {
        let spec = parse_spec(
            r#"
id: test
version: "1.0.0"
states:
  idle:
timeout:
  nonexistent: 30s
"#,
        );
        let err = validate_workflow_spec(&spec).unwrap_err();
        assert!(err.to_string().contains("nonexistent"));
    }

    #[test]
    fn on_timeout_引用不存在的状态报错() {
        let spec = parse_spec(
            r#"
id: test
version: "1.0.0"
states:
  idle:
timeout:
  idle: 30s
on_timeout: nonexistent
"#,
        );
        let err = validate_workflow_spec(&spec).unwrap_err();
        assert!(err.to_string().contains("nonexistent"));
    }

    #[test]
    fn 重复transition报错() {
        let spec = parse_spec(
            r#"
id: test
version: "1.0.0"
states:
  idle:
  running:
transitions:
  - from: idle
    to: running
    when: "true"
  - from: idle
    to: running
    when: "true"
"#,
        );
        let err = validate_workflow_spec(&spec).unwrap_err();
        assert!(err.to_string().contains("重复"));
    }

    #[test]
    fn 初始状态优先选idle() {
        let spec = parse_spec(
            r#"
id: test
version: "1.0.0"
states:
  active:
  idle:
"#,
        );
        assert_eq!(determine_initial_state(&spec).unwrap(), "idle");
    }

    #[test]
    fn 无idle时选字典序首个() {
        let spec = parse_spec(
            r#"
id: test
version: "1.0.0"
states:
  beta:
  alpha:
"#,
        );
        assert_eq!(determine_initial_state(&spec).unwrap(), "alpha");
    }

    #[test]
    fn 空状态时初始状态报错() {
        let spec = parse_spec(
            r#"
id: test
version: "1.0.0"
states: {}
"#,
        );
        assert!(determine_initial_state(&spec).is_err());
    }
}
