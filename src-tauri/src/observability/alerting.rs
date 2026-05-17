//! 告警构建与匹配。

use chrono::{DateTime, Utc};
use serde_json::{Value, json};
use tauri_bindings::{AlertDeliveryRecord, ObservabilityEntry};

use super::types::{ObservabilitySession, build_record_id};

pub(crate) fn alert_to_entry(alert: &AlertDeliveryRecord) -> ObservabilityEntry {
    ObservabilityEntry {
        id: alert.id.clone(),
        timestamp: alert.timestamp.clone(),
        level: if alert.success {
            "success".to_owned()
        } else {
            "error".to_owned()
        },
        category: "alert".to_owned(),
        source: alert.node_id.clone(),
        message: if alert.success {
            "告警投递成功".to_owned()
        } else {
            "告警投递失败".to_owned()
        },
        detail: Some(format!(
            "{} {} -> {} ({})",
            alert.method, alert.url, alert.status, alert.environment_name
        )),
        trace_id: Some(alert.trace_id.clone()),
        node_id: Some(alert.node_id.clone()),
        duration_ms: None,
        project_id: Some(alert.project_id.clone()),
        project_name: Some(alert.project_name.clone()),
        environment_id: Some(alert.environment_id.clone()),
        environment_name: Some(alert.environment_name.clone()),
        data: Some(json!({
            "url": alert.url,
            "method": alert.method,
            "status": alert.status,
            "success": alert.success,
            "webhook_kind": alert.webhook_kind,
            "body_mode": alert.body_mode,
            "request_timeout_ms": alert.request_timeout_ms,
            "requested_at": alert.requested_at,
            "request_body_preview": alert.request_body_preview,
        })),
        event_kind: None,
    }
}

pub(crate) fn alert_search_text(alert: &AlertDeliveryRecord) -> String {
    format!(
        "{} {} {} {} {} {}",
        alert.node_id,
        alert.url,
        alert.method,
        alert.status,
        alert.project_name,
        alert.environment_name
    )
}

pub(crate) fn build_alert_delivery(
    session: &ObservabilitySession,
    _stage: &str,
    trace_id: uuid::Uuid,
    http_meta: &serde_json::Map<String, Value>,
    timestamp: DateTime<Utc>,
) -> Option<AlertDeliveryRecord> {
    let node_id = http_meta.get("node_id")?.as_str()?.to_owned();
    let url = http_meta.get("url")?.as_str()?.to_owned();
    let method = http_meta
        .get("method")
        .and_then(Value::as_str)
        .unwrap_or("POST")
        .to_owned();
    let status = http_meta
        .get("status")
        .and_then(Value::as_u64)
        .and_then(|value| u16::try_from(value).ok())?;

    Some(AlertDeliveryRecord {
        id: build_record_id("alert", &timestamp),
        timestamp: timestamp.to_rfc3339(),
        trace_id: trace_id.to_string(),
        node_id,
        project_id: session.project_id.clone(),
        project_name: session.project_name.clone(),
        environment_id: session.environment_id.clone(),
        environment_name: session.environment_name.clone(),
        url,
        method,
        status,
        success: status < 400,
        webhook_kind: http_meta
            .get("webhook_kind")
            .and_then(Value::as_str)
            .map(str::to_owned),
        body_mode: http_meta
            .get("body_mode")
            .and_then(Value::as_str)
            .map(str::to_owned),
        request_timeout_ms: http_meta.get("request_timeout_ms").and_then(Value::as_u64),
        requested_at: http_meta
            .get("requested_at")
            .and_then(Value::as_str)
            .map(str::to_owned),
        request_body_preview: http_meta
            .get("request_body_preview")
            .and_then(Value::as_str)
            .map(str::to_owned),
    })
}
