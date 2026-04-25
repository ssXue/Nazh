//! HTTP 请求节点，将 payload 发送到指定端点并将响应写入上下文。
//!
//! 支持三种 body 模式：`json`（默认）、`template`（占位符渲染）和
//! `dingtalk_markdown`（钉钉机器人 Markdown 格式）。GET/HEAD 不发送请求体，
//! 其余方法根据 `body_mode` 渲染请求体。响应状态码 >= 400 视为错误。

use async_trait::async_trait;
use chrono::Utc;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};

use uuid::Uuid;

use crate::template::{self, TemplateVars};
use connections::{SharedConnectionManager, connection_metadata};
use nazh_core::{EngineError, NodeExecution, NodeTrait, into_payload_map};

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

/// HTTP 头部值转换：复用模板模块的显示逻辑。
fn value_to_header_string(value: &Value) -> String {
    template::value_to_display_string(value)
}

fn parse_json_or_string(raw: &str) -> Value {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        Value::Null
    } else {
        serde_json::from_str(trimmed).unwrap_or_else(|_| Value::String(trimmed.to_owned()))
    }
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
    payload: &Value,
    trace_id: &uuid::Uuid,
    timestamp: &chrono::DateTime<Utc>,
    requested_at: &str,
) -> Result<(String, String, String, String), EngineError> {
    let webhook_kind = normalize_http_webhook_kind(&config.webhook_kind).to_owned();
    let body_mode = normalize_http_body_mode(&config.body_mode, &webhook_kind).to_owned();
    let event_timestamp = timestamp.to_rfc3339();

    let vars = TemplateVars {
        payload,
        trace_id,
        node_id,
        timestamp: &event_timestamp,
        extras: &[("requested_at", requested_at)],
    };

    let body = match body_mode.as_str() {
        "template" => {
            let tpl = if config.body_template.trim().is_empty() {
                "{{payload}}"
            } else {
                config.body_template.as_str()
            };
            template::render(tpl, &vars)
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
            let rendered_title = template::render(title_template, &vars);
            let rendered_body = template::render(body_template, &vars);

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
        _ => serde_json::to_string(payload).map_err(|error| {
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HttpClientNodeConfig {
    #[serde(default)]
    pub connection_id: Option<String>,
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
            connection_id: None,
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
    config: HttpClientNodeConfig,
    client: reqwest::Client,
    connection_manager: SharedConnectionManager,
}

impl HttpClientNode {
    /// 创建新的 HTTP 客户端节点。
    ///
    /// # Errors
    ///
    /// 当 `reqwest::Client` 构建失败时返回 `EngineError`（例如 TLS 后端初始化异常）。
    pub fn new(
        id: impl Into<String>,
        config: HttpClientNodeConfig,
        connection_manager: SharedConnectionManager,
    ) -> Result<Self, EngineError> {
        let id = id.into();
        let client = reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::limited(10))
            .build()
            .map_err(|error| {
                EngineError::node_config(id.clone(), format!("HTTP 客户端初始化失败: {error}"))
            })?;
        Ok(Self {
            id,
            config,
            client,
            connection_manager,
        })
    }

    fn resolve_config(
        &self,
        connection_metadata: Option<&Value>,
    ) -> Result<HttpClientNodeConfig, EngineError> {
        let mut config_value = serde_json::to_value(&self.config)
            .map_err(|error| EngineError::node_config(self.id.clone(), error.to_string()))?;

        if let Some(metadata) = connection_metadata.and_then(Value::as_object) {
            let Some(config_map) = config_value.as_object_mut() else {
                return Err(EngineError::node_config(
                    self.id.clone(),
                    "HTTP Client 配置格式无效".to_owned(),
                ));
            };

            for key in [
                "url",
                "method",
                "headers",
                "webhook_kind",
                "content_type",
                "request_timeout_ms",
                "at_mobiles",
                "at_all",
            ] {
                if let Some(value) = metadata.get(key) {
                    config_map.insert(key.to_owned(), value.clone());
                }
            }
        }

        serde_json::from_value(config_value)
            .map_err(|error| EngineError::node_config(self.id.clone(), error.to_string()))
    }
}

async fn send_http_request(request: reqwest::RequestBuilder) -> Result<(u16, Value), String> {
    let response = request
        .send()
        .await
        .map_err(|error| format!("HTTP 请求失败: {error}"))?;

    let status_code = response.status().as_u16();
    let response_body = response
        .text()
        .await
        .map_err(|error| format!("读取 HTTP 响应体失败: {error}"))?;

    Ok((status_code, parse_json_or_string(&response_body)))
}

struct HttpMetadataParams<'a> {
    node_id: &'a str,
    url: &'a str,
    method: &'a str,
    webhook_kind: &'a str,
    body_mode: &'a str,
    content_type: &'a str,
    request_timeout_ms: u64,
    status_code: u16,
    requested_at: &'a str,
    body_preview: &'a str,
}

fn build_http_response_metadata(params: &HttpMetadataParams<'_>) -> serde_json::Map<String, Value> {
    let mut metadata = serde_json::Map::new();
    metadata.insert(
        "http".to_owned(),
        json!({
            "node_id": params.node_id,
            "url": params.url,
            "method": params.method,
            "webhook_kind": params.webhook_kind,
            "body_mode": params.body_mode,
            "content_type": params.content_type,
            "request_timeout_ms": params.request_timeout_ms,
            "status": params.status_code,
            "requested_at": params.requested_at,
            "request_body_preview": params.body_preview,
        }),
    );
    metadata
}

fn build_http_request(
    node: &HttpClientNode,
    method: &reqwest::Method,
    url: &str,
    request_timeout_ms: u64,
    payload_body: String,
    content_type: &str,
    resolved_config: &HttpClientNodeConfig,
) -> reqwest::RequestBuilder {
    let mut request = node
        .client
        .request(method.clone(), url)
        .timeout(std::time::Duration::from_millis(request_timeout_ms));

    for (key, value) in &resolved_config.headers {
        request = request.header(key.as_str(), value_to_header_string(value));
    }

    if *method != reqwest::Method::GET && *method != reqwest::Method::HEAD {
        let has_content_type_header = resolved_config
            .headers
            .keys()
            .any(|key| key.eq_ignore_ascii_case("content-type"));
        if !has_content_type_header && !content_type.is_empty() {
            request = request.header("Content-Type", content_type);
        }
        request = request.body(payload_body);
    }

    request
}

#[async_trait]
impl NodeTrait for HttpClientNode {
    nazh_core::impl_node_meta!("httpClient");

    async fn transform(
        &self,
        trace_id: Uuid,
        payload: Value,
    ) -> Result<NodeExecution, EngineError> {
        let connection_id = self.config.connection_id.as_deref().ok_or_else(|| {
            EngineError::node_config(
                self.id.clone(),
                "HTTP Client 节点需要在 Connection Studio 中绑定一个 HTTP / Webhook 连接",
            )
        })?;
        let mut guard = Some(self.connection_manager.acquire(connection_id).await?);
        let resolved_config =
            self.resolve_config(guard.as_ref().map(connections::ConnectionGuard::metadata))?;

        let method = resolved_config.method.trim().to_uppercase();
        let url = resolved_config.url.trim().to_owned();
        if url.is_empty() {
            if let Some(connection_guard) = &mut guard {
                connection_guard.mark_failure("HTTP Client 节点需要配置 URL");
            }
            return Err(EngineError::node_config(
                self.id.clone(),
                "HTTP Client 节点需要配置 URL",
            ));
        }

        let now = Utc::now();
        let requested_at = now.to_rfc3339();
        let request_timeout_ms = resolved_config.request_timeout_ms.max(500);
        let (payload_body, content_type, webhook_kind, body_mode) = prepare_http_request_body(
            &self.id,
            &resolved_config,
            &payload,
            &trace_id,
            &now,
            &requested_at,
        )?;

        let reqwest_method = method.parse::<reqwest::Method>().map_err(|error| {
            EngineError::node_config(self.id.clone(), format!("无效的 HTTP 方法: {error}"))
        })?;

        let body_preview = template::truncate(&payload_body, 320);
        let request = build_http_request(
            self,
            &reqwest_method,
            &url,
            request_timeout_ms,
            payload_body,
            &content_type,
            &resolved_config,
        );

        let (status_code, response_value) = match send_http_request(request).await {
            Ok(result) => result,
            Err(message) => {
                if let Some(connection_guard) = &mut guard {
                    connection_guard.mark_failure(&message);
                }
                return Err(EngineError::stage_execution(
                    self.id.clone(),
                    trace_id,
                    message,
                ));
            }
        };

        if status_code >= 400 {
            let message = format!(
                "HTTP 请求返回错误状态码 {status_code}: {}",
                template::truncate(&template::value_to_display_string(&response_value), 240)
            );
            if let Some(connection_guard) = &mut guard {
                connection_guard.mark_failure(&message);
            }
            return Err(EngineError::stage_execution(
                self.id.clone(),
                trace_id,
                message,
            ));
        }

        let mut payload_map = into_payload_map(payload);
        payload_map.insert("http_response".to_owned(), response_value);

        let mut metadata = serde_json::Map::new();
        if let Some(connection_guard) = guard.as_ref() {
            let (key, value) = connection_metadata(&self.id, connection_guard.lease())?;
            metadata.insert(key, value);
        }
        let http_meta = build_http_response_metadata(&HttpMetadataParams {
            node_id: &self.id,
            url: &url,
            method: &method,
            webhook_kind: &webhook_kind,
            body_mode: &body_mode,
            content_type: &content_type,
            request_timeout_ms,
            status_code,
            requested_at: &requested_at,
            body_preview: &body_preview,
        });
        metadata.extend(http_meta);

        if let Some(connection_guard) = &mut guard {
            connection_guard.mark_success();
        }

        Ok(NodeExecution::broadcast(Value::Object(payload_map)).with_metadata(metadata))
    }
}
