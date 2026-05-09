use super::*;
use connections::shared_connection_manager;

fn make_node(mode: &str) -> MqttClientNode {
    let config: MqttClientNodeConfig = serde_json::from_value(json!({ "mode": mode })).unwrap();
    MqttClientNode::new("mqtt-1", config, shared_connection_manager())
}

#[test]
fn publish_模式_input_json_output_any() {
    let node = make_node("publish");

    let inputs = node.input_pins();
    assert_eq!(inputs.len(), 1);
    assert_eq!(inputs[0].pin_type, PinType::Json);
    assert!(inputs[0].required, "publish 模式必需 payload");

    let outputs = node.output_pins();
    assert_eq!(outputs.len(), 1);
    assert_eq!(
        outputs[0].pin_type,
        PinType::Any,
        "publish 仅 echo 上游，输出无具体 schema"
    );
}

#[test]
fn subscribe_模式_input_any_output_json() {
    let node = make_node("subscribe");

    let inputs = node.input_pins();
    assert_eq!(inputs.len(), 1);
    assert_eq!(
        inputs[0].pin_type,
        PinType::Any,
        "subscribe 模式实际由 on_deploy 触发，input 仅用于手动 dispatch"
    );

    let outputs = node.output_pins();
    assert_eq!(outputs.len(), 1);
    assert_eq!(
        outputs[0].pin_type,
        PinType::Json,
        "subscribe 输出是规范化后的 JSON 消息"
    );
}

#[test]
fn pin_声明随_mode_变化而切换() {
    // 守住"input_pins/output_pins 是 &self 实例方法（非 'static 表）"的
    // 核心理由：同 NodeTrait 实现，pin 形态完全由 self.config 决定。
    let publish = make_node("publish");
    let subscribe = make_node("subscribe");

    assert_ne!(
        publish.input_pins()[0].pin_type,
        subscribe.input_pins()[0].pin_type
    );
    assert_ne!(
        publish.output_pins()[0].pin_type,
        subscribe.output_pins()[0].pin_type
    );
}
