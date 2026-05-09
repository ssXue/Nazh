use serde_json::{Value, json};

pub(super) struct BarkMetadataParams<'a> {
    pub(super) node_id: &'a str,
    pub(super) endpoint: &'a str,
    pub(super) content_mode: &'a str,
    pub(super) level: &'a str,
    pub(super) status_code: u16,
    pub(super) request_timeout_ms: u64,
    pub(super) requested_at: &'a str,
    pub(super) request_body_preview: &'a str,
}

pub(super) fn build_bark_metadata(
    params: &BarkMetadataParams<'_>,
) -> serde_json::Map<String, Value> {
    let mut metadata = serde_json::Map::new();
    metadata.insert(
        "http".to_owned(),
        json!({
            "node_id": params.node_id,
            "url": params.endpoint,
            "method": "POST",
            "webhook_kind": "bark",
            "body_mode": params.content_mode,
            "content_type": "application/json",
            "request_timeout_ms": params.request_timeout_ms,
            "status": params.status_code,
            "requested_at": params.requested_at,
            "request_body_preview": params.request_body_preview,
        }),
    );
    metadata.insert(
        "bark".to_owned(),
        json!({
            "node_id": params.node_id,
            "endpoint": params.endpoint,
            "content_mode": params.content_mode,
            "level": params.level,
            "request_timeout_ms": params.request_timeout_ms,
            "requested_at": params.requested_at,
        }),
    );
    metadata
}
