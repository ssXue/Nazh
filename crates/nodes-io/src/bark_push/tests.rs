use super::*;

use serde_json::json;

#[test]
fn resolve_bark_endpoint_从完整_url_提取设备_key() {
    let endpoint = resolve_bark_endpoint(
        "https://fallback.example",
        "https://api.day.app/device-key/path?ignored=true",
    )
    .unwrap();

    assert_eq!(endpoint, "https://api.day.app/device-key");
}

#[test]
fn build_bark_request_body_渲染模板与可选字段() {
    let config = BarkPushNodeConfig {
        title_template: "告警 {{payload.tag}}".to_owned(),
        body_template: "{{payload.message}}".to_owned(),
        badge: "3".to_owned(),
        sound: "alarm".to_owned(),
        group: "line-1".to_owned(),
        auto_copy: true,
        archive_mode: "archive".to_owned(),
        ..BarkPushNodeConfig::default()
    };
    let payload = json!({
        "tag": "E01",
        "message": "压力过高",
    });
    let trace_id = Uuid::nil();
    let vars = TemplateVars {
        payload: &payload,
        trace_id: &trace_id,
        node_id: "bark",
        timestamp: "2026-05-09T00:00:00Z",
        extras: &[],
    };

    let body = build_bark_request_body(
        &config, &vars, "body", "critical", "archive", "bark", trace_id,
    )
    .unwrap();

    assert_eq!(body["title"], "告警 E01");
    assert_eq!(body["body"], "压力过高");
    assert_eq!(body["badge"], 3);
    assert_eq!(body["sound"], "alarm");
    assert_eq!(body["group"], "line-1");
    assert_eq!(body["autoCopy"], "1");
    assert_eq!(body["isArchive"], 1);
    assert_eq!(body["level"], "critical");
}

#[test]
fn validate_bark_response_识别业务错误码() {
    let err = validate_bark_response(200, &json!({"code": 400, "message": "bad key"})).unwrap_err();

    assert!(err.contains("bad key"));
}
