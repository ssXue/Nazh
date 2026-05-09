use super::*;
use connections::shared_connection_manager;

fn make_node() -> HttpClientNode {
    HttpClientNode::new(
        "http-1",
        HttpClientNodeConfig::default(),
        shared_connection_manager(),
    )
    .unwrap()
}

#[test]
fn input_pin_是_json_必需() {
    let node = make_node();
    let pins = node.input_pins();
    assert_eq!(pins.len(), 1);
    assert_eq!(pins[0].id, "in");
    assert_eq!(pins[0].pin_type, PinType::Json);
    assert!(pins[0].required);
}

#[test]
fn output_pin_是_json() {
    let node = make_node();
    let pins = node.output_pins();
    assert_eq!(pins.len(), 1);
    assert_eq!(pins[0].id, "out");
    assert_eq!(pins[0].pin_type, PinType::Json);
    assert!(!pins[0].required, "HTTP 响应不强制下游消费");
}
