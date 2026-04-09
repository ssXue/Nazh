//! HTTP 请求节点，将 payload 发送到指定端点并将响应写入上下文。
//!
//! 支持三种 body 模式：`json`（默认）、`template`（占位符渲染）和
//! `dingtalk_markdown`（钉钉机器人 Markdown 格式）。GET/HEAD 不发送请求体，
//! 其余方法根据 body_mode 渲染请求体。响应状态码 >= 400 视为错误。

use async_trait::async_trait;
use chrono::Utc;
use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};

use super::helpers::into_payload_map;
use super::{NodeExecution, NodeTrait};
use crate::{EngineError, WorkflowContext};

fn default_http_method() -> String {
    "POST".to_owned()
}

fn default_http_webhook_kind() -> String {
    "generic".to_owned()
}

fn default_http_body_mode() -> String {
    "json".to_owned()
}

fn default_http_content_type() -> String {
    "application/json".to_owned()
}

fn default_http_request_timeout_ms() -> u64 {
    4_000
}

fn value_to_header_string(value: &Value) -> String {
    match value {
        Value::String(text) => text.clone(),
        other => other.to_string(),
    }
}

fn parse_json_or_string(raw: &str) -> Value {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        Value::Null
    } else {
        serde_json::from_str(trimmed).unwrap_or_else(|_| Value::String(trimmed.to_owned()))
    }
}

fn resolve_json_path<'a>(root: &'a Value, path: &str) -> Option<&'a Value> {
    path.split('.')
        .filter(|segment| !segment.is_empty())
        .try_fold(root, |current, segment| match current {
            Value::Object(map) => map.get(segment),
            Value::Array(items) => segment
                .parse::<usize>()
                .ok()
                .and_then(|index| items.get(index)),
            _ => None,
        })
}

fn value_to_template_string(value: &Value) -> String {
    match value {
        Value::Null => String::new(),
        Value::String(text) => text.clone(),
        other => other.to_string(),
    }
}

fn resolve_http_template_key(
    key: &str,
    payload: &Value,
    trace_id: &uuid::Uuid,
    node_id: &str,
    event_timestamp: &str,
    requested_at: &str,
) -> String {
    match key {
        "trace_id" => trace_id.to_string(),
        "node_id" => node_id.to_owned(),
        "timestamp" | "event_at" => event_timestamp.to_owned(),
        "requested_at" => requested_at.to_owned(),
        "payload" => payload.to_string(),
        _ => {
            if let Some(path) = key.strip_prefix("payload.") {
                resolve_json_path(payload, path)
                    .map(value_to_template_string)
                    .unwrap_or_default()
            } else {
                String::new()
            }
        }
    }
}

fn render_http_template(
    template: &str,
    payload: &Value,
    trace_id: &uuid::Uuid,
    node_id: &str,
    event_timestamp: &str,
    requested_at: &str,
) -> String {
    let mut result = String::with_capacity(template.len() + 48);
    let mut remaining = template;

    while let Some(start_index) = remaining.find("{{") {
        result.push_str(&remaining[..start_index]);
        let placeholder_region = &remaining[start_index + 2..];

        if let Some(end_index) = placeholder_region.find("}}") {
            let key = placeholder_region[..end_index].trim();
            result.push_str(&resolve_http_template_key(
                key,
                payload,
                trace_id,
                node_id,
                event_timestamp,
                requested_at,
            ));
            remaining = &placeholder_region[end_index + 2..];
        } else {
            result.push_str(&remaining[start_index..]);
            return result;
        }
    }

    result.push_str(remaining);
    result
}

fn default_http_alarm_title_template() -> &'static str {
    "Nazh 工业告警 · {{payload.tag}} · {{payload.severity}}"
}

fn default_http_alarm_body_template() -> &'static str {
    "### Nazh 工业告警\n- 设备：{{payload.tag}}\n- 温度：{{payload.temperature_c}} °C\n- 严重级别：{{payload.severity}}\n- Trace：{{trace_id}}\n- 事件时间：{{timestamp}}"
}

fn normalize_http_webhook_kind(value: &str) -> &'static str {
    match value.trim().to_ascii_lowercase().as_str() {
        "dingtalk" | "ding_talk" | "ding-talk" => "dingtalk",
        _ => "generic",
    }
}

fn normalize_http_body_mode(value: &str, webhook_kind: &str) -> &'static str {
    match value.trim().to_ascii_lowercase().as_str() {
        "template" | "raw-template" => "template",
        "dingtalk_markdown" | "dingtalk-markdown" | "alarm-template" => "dingtalk_markdown",
        "json" | "payload-json" | "payload_json" => "json",
        _ => {
            if webhook_kind == "dingtalk" {
                "dingtalk_markdown"
            } else {
                "json"
            }
        }
    }
}

fn prepare_http_request_body(
    node_id: &str,
    config: &HttpClientNodeConfig,
    ctx: &WorkflowContext,
    requested_at: &str,
) -> Result<(String, String, String, String), EngineError> {
    let webhook_kind = normalize_http_webhook_kind(&config.webhook_kind).to_owned();
    let body_mode = normalize_http_body_mode(&config.body_mode, &webhook_kind).to_owned();
    let event_timestamp = ctx.timestamp.to_rfc3339();

    let body = match body_mode.as_str() {
        "template" => {
            let template = if config.body_template.trim().is_empty() {
                "{{payload}}"
            } else {
                config.body_template.as_str()
            };
            render_http_template(
                template,
                &ctx.payload,
                &ctx.trace_id,
                node_id,
                &event_timestamp,
                requested_at,
            )
        }
        "dingtalk_markdown" => {
            let title_template = if config.title_template.trim().is_empty() {
                default_http_alarm_title_template()
            } else {
                config.title_template.as_str()
            };
            let body_template = if config.body_template.trim().is_empty() {
                default_http_alarm_body_template()
            } else {
                config.body_template.as_str()
            };
            let rendered_title = render_http_template(
                title_template,
                &ctx.payload,
                &ctx.trace_id,
                node_id,
                &event_timestamp,
                requested_at,
            );
            let rendered_body = render_http_template(
                body_template,
                &ctx.payload,
                &ctx.trace_id,
                node_id,
                &event_timestamp,
                requested_at,
            );

            serde_json::to_string(&json!({
                "msgtype": "markdown",
                "markdown": {
                    "title": rendered_title,
                    "text": rendered_body,
                },
                "at": {
                    "atMobiles": config.at_mobiles,
                    "isAtAll": config.at_all,
                }
            }))
            .map_err(|error| {
                EngineError::payload_conversion(node_id.to_owned(), error.to_string())
            })?
        }
        _ => serde_json::to_string(&ctx.payload).map_err(|error| {
            EngineError::payload_conversion(node_id.to_owned(), error.to_string())
        })?,
    };

    Ok((
        body,
        config.content_type.trim().to_owned(),
        webhook_kind,
        body_mode,
    ))
}

fn truncate_for_meta(text: &str, limit: usize) -> String {
    let mut truncated = text.chars().take(limit).collect::<String>();
    if text.chars().count() > limit {
        truncated.push_str("…");
    }
    truncated
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HttpClientNodeConfig {
    pub url: String,
    #[serde(default = "default_http_method")]
    pub method: String,
    #[serde(default)]
    pub headers: Map<String, Value>,
    #[serde(default = "default_http_webhook_kind")]
    pub webhook_kind: String,
    #[serde(default = "default_http_body_mode")]
    pub body_mode: String,
    #[serde(default = "default_http_content_type")]
    pub content_type: String,
    #[serde(default = "default_http_request_timeout_ms")]
    pub request_timeout_ms: u64,
    #[serde(default)]
    pub body_template: String,
    #[serde(default)]
    pub title_template: String,
    #[serde(default)]
    pub at_mobiles: Vec<String>,
    #[serde(default)]
    pub at_all: bool,
}

impl Default for HttpClientNodeConfig {
    fn default() -> Self {
        Self {
            url: String::new(),
            method: default_http_method(),
            headers: Map::new(),
            webhook_kind: default_http_webhook_kind(),
            body_mode: default_http_body_mode(),
            content_type: default_http_content_type(),
            request_timeout_ms: default_http_request_timeout_ms(),
            body_template: String::new(),
            title_template: String::new(),
            at_mobiles: Vec::new(),
            at_all: false,
        }
    }
}

/// HTTP 请求节点，内置 [`reqwest::Client`] 连接池。
///
/// 构造时创建 reqwest 客户端，支持 webhook 模板渲染和请求超时。
pub struct HttpClientNode {
    id: String,
    ai_description: String,
    config: HttpClientNodeConfig,
    client: reqwest::Client,
}

impl HttpClientNode {
    pub fn new(
        id: impl Into<String>,
        config: HttpClientNodeConfig,
        ai_description: impl Into<String>,
    ) -> Self {
        let client = reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::limited(10))
            .build()
            .unwrap_or_default();
        Self {
            id: id.into(),
            ai_description: ai_description.into(),
            config,
            client,
        }
    }
}

#[async_trait]
impl NodeTrait for HttpClientNode {
    fn id(&self) -> &str {
        &self.id
    }

    fn kind(&self) -> &'static str {
        "httpClient"
    }

    fn ai_description(&self) -> &str {
        &self.ai_description
    }

    async fn execute(&self, ctx: WorkflowContext) -> Result<NodeExecution, EngineError> {
        let method = self.config.method.trim().to_uppercase();
        let url = self.config.url.trim().to_owned();
        if url.is_empty() {
            return Err(EngineError::node_config(
                self.id.clone(),
                "HTTP Client 节点需要配置 URL",
            ));
        }

        let requested_at = Utc::now().to_rfc3339();
        let request_timeout_ms = self.config.request_timeout_ms.max(500);
        let (payload_body, content_type, webhook_kind, body_mode) =
            prepare_http_request_body(&self.id, &self.config, &ctx, &requested_at)?;
        let requested_at_for_meta = requested_at.clone();
        let content_type_for_meta = content_type.clone();
        let webhook_kind_for_meta = webhook_kind.clone();
        let body_mode_for_meta = body_mode.clone();

        let reqwest_method = method.parse::<reqwest::Method>().map_err(|error| {
            EngineError::node_config(self.id.clone(), format!("无效的 HTTP 方法: {error}"))
        })?;

        let mut request = self
            .client
            .request(reqwest_method, &url)
            .timeout(std::time::Duration::from_millis(request_timeout_ms as u64));

        for (key, value) in &self.config.headers {
            request = request.header(key.as_str(), value_to_header_string(value));
        }

        if method != "GET" && method != "HEAD" {
            let has_content_type_header = self
                .config
                .headers
                .keys()
                .any(|key| key.eq_ignore_ascii_case("content-type"));
            if !has_content_type_header && !content_type.is_empty() {
                request = request.header("Content-Type", content_type.as_str());
            }
            request = request.body(payload_body.clone());
        }

        let response = request.send().await.map_err(|error| {
            EngineError::stage_execution(
                self.id.clone(),
                ctx.trace_id,
                format!("HTTP 请求失败: {error}"),
            )
        })?;

        let status_code = response.status().as_u16();
        let response_body = response.text().await.map_err(|error| {
            EngineError::stage_execution(
                self.id.clone(),
                ctx.trace_id,
                format!("读取 HTTP 响应体失败: {error}"),
            )
        })?;
        let response_value = parse_json_or_string(&response_body);

        if status_code >= 400 {
            return Err(EngineError::stage_execution(
                self.id.clone(),
                ctx.trace_id,
                format!(
                    "HTTP Alarm 返回状态码 {status_code}: {}",
                    truncate_for_meta(&value_to_template_string(&response_value), 240)
                ),
            ));
        }

        let trace_id = ctx.trace_id;
        let mut payload_map = into_payload_map(ctx.payload);
        let mut http_meta = Map::new();
        http_meta.insert("url".to_owned(), Value::String(url));
        http_meta.insert("method".to_owned(), Value::String(method));
        http_meta.insert(
            "webhook_kind".to_owned(),
            Value::String(webhook_kind_for_meta),
        );
        http_meta.insert("body_mode".to_owned(), Value::String(body_mode_for_meta));
        http_meta.insert(
            "content_type".to_owned(),
            Value::String(content_type_for_meta),
        );
        http_meta.insert(
            "request_timeout_ms".to_owned(),
            Value::from(request_timeout_ms),
        );
        http_meta.insert("status".to_owned(), Value::from(status_code));
        http_meta.insert("ok".to_owned(), Value::Bool(status_code < 400));
        http_meta.insert(
            "requested_at".to_owned(),
            Value::String(requested_at_for_meta),
        );
        http_meta.insert(
            "request_body_preview".to_owned(),
            Value::String(truncate_for_meta(&payload_body, 320)),
        );
        payload_map.insert("_http".to_owned(), Value::Object(http_meta));
        payload_map.insert("http_response".to_owned(), response_value);

        Ok(NodeExecution::broadcast(WorkflowContext::from_parts(
            trace_id,
            Utc::now(),
            Value::Object(payload_map),
        )))
    }
}
