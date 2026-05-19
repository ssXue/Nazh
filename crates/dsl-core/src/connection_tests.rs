use super::*;
use crate::parser::parse_connection_yaml_validated;

fn governance_yaml() -> &'static str {
    r#"
governance:
  connect_timeout_ms: 3000
  operation_timeout_ms: 5000
  heartbeat_interval_ms: 3000
  heartbeat_timeout_ms: 12000
  rate_limit_max_attempts: 8
  rate_limit_window_ms: 10000
  rate_limit_cooldown_ms: 4000
  circuit_failure_threshold: 3
  circuit_open_ms: 15000
  reconnect_base_ms: 800
  reconnect_max_ms: 8000
"#
}

#[test]
fn modbus_connection_spec_从_yaml_解析成功() {
    let yaml = format!(
        r#"
id: plc-line-a
protocol:
  type: modbus-tcp
  host: 192.168.10.11
  port: 502
  unit_id: 1
{}
labels:
  - production
description: 一号线 PLC
"#,
        governance_yaml()
    );

    let spec = parse_connection_yaml_validated(&yaml).unwrap();
    assert_eq!(spec.id, "plc-line-a");
    assert!(matches!(
        spec.protocol,
        ConnectionProtocol::ModbusTcp { port: 502, .. }
    ));
}

#[test]
fn http_sensitive_header_拒绝明文_literal() {
    let yaml = format!(
        r#"
id: webhook-main
protocol:
  type: http
  url: https://example.com/ingest
  method: POST
  headers:
    - name: Authorization
      value:
        type: literal
        value: Bearer plain-token
{}
"#,
        governance_yaml()
    );

    let err = parse_connection_yaml_validated(&yaml).unwrap_err();
    assert!(err.to_string().contains("敏感 HTTP Header"));
}

#[test]
fn secret_ref_必须使用_secret_scheme() {
    let yaml = format!(
        r#"
id: mqtt-main
protocol:
  type: mqtt
  host: broker.local
  port: 1883
  topic: factory/events
{}
secrets:
  password: plain-password
"#,
        governance_yaml()
    );

    let err = parse_connection_yaml_validated(&yaml).unwrap_err();
    assert!(err.to_string().contains("secret://"));
}

#[test]
fn 明文_password_字段被_schema_拒绝() {
    let yaml = format!(
        r#"
id: mqtt-main
protocol:
  type: mqtt
  host: broker.local
  port: 1883
  topic: factory/events
  password: plain-password
{}
"#,
        governance_yaml()
    );

    let err = parse_connection_yaml_validated(&yaml).unwrap_err();
    assert!(err.to_string().contains("YAML 解析失败"));
}

#[test]
fn governance_缺失时解析失败() {
    let yaml = r#"
id: plc-line-a
protocol:
  type: modbus-tcp
  host: 192.168.10.11
  port: 502
"#;

    let err = parse_connection_yaml_validated(yaml).unwrap_err();
    assert!(err.to_string().contains("governance"));
}
