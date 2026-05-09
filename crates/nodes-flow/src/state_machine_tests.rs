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

#[tokio::test]
async fn transition_条件运行时受_max_operations_限制() {
    let mut config = build_config();
    config.max_operations = 10;
    config.transitions = vec![TransitionConfig {
        from: "idle".to_owned(),
        to: "running".to_owned(),
        when: "let n = 0; while n < 1000 { n += 1; } true".to_owned(),
        priority: 0,
        action_port: None,
    }];
    let vars = build_vars("idle");
    let node = StateMachineNode::new("sm_test", config, Some(vars)).unwrap();

    let err = node
        .transform(Uuid::new_v4(), serde_json::json!({}))
        .await
        .unwrap_err();

    assert!(
        err.to_string().contains("操作数")
            || err.to_string().contains("operation")
            || err.to_string().contains("too many"),
        "超步数错误应从 stateMachine transition 传播，实际：{err}"
    );
}

#[tokio::test]
async fn 无_action_transition_不广播到已有_action_port() {
    let config = StateMachineConfig {
        initial_state: "idle".to_owned(),
        states: vec![
            StateConfig {
                name: "idle".to_owned(),
                entry_actions: vec!["idle_entry".to_owned()],
                exit_actions: vec![],
            },
            StateConfig {
                name: "running".to_owned(),
                entry_actions: vec![],
                exit_actions: vec![],
            },
        ],
        transitions: vec![TransitionConfig {
            from: "idle".to_owned(),
            to: "running".to_owned(),
            when: "payload.start == true".to_owned(),
            priority: 0,
            action_port: None,
        }],
        timeout_rules: vec![],
        on_timeout_target: None,
        max_operations: 50_000,
    };
    let vars = build_vars("idle");
    let node = StateMachineNode::new("sm_test", config, Some(vars)).unwrap();

    let result = node
        .transform(Uuid::new_v4(), serde_json::json!({ "start": true }))
        .await
        .unwrap();

    assert!(
        matches!(
            result.outputs[0].dispatch,
            nazh_core::NodeDispatch::Route(ref ports) if ports.is_empty()
        ),
        "无 action transition 不能 Broadcast，否则会触发既有 action-port 下游"
    );
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
