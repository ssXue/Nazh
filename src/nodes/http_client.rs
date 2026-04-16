//! HTTP 请求节点，将 payload 发送到指定端点并将响应写入上下文。
//!
//! 支持三种 body 模式：`json`（默认）、`template`（占位符渲染）和
//! `dingtalk_markdown`（钉钉机器人 Markdown 格式）。GET/HEAD 不发送请求体，
//! 其余方法根据 `body_mode` 渲染请求体。响应状态码 >= 400 视为错误。

use async_trait::async_trait;
use chrono::Utc;
use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};

use super::helpers::into_payload_map;
use super::template::{self, TemplateVars};
use super::{NodeExecution, NodeTrait};
use crate::{ContextRef, DataStore, EngineError};

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
    /// 创建新的 HTTP 客户端节点。
    ///
    /// # Errors
    ///
    /// 当 `reqwest::Client` 构建失败时返回 `EngineError`（例如 TLS 后端初始化异常）。
    pub fn new(
        id: impl Into<String>,
        config: HttpClientNodeConfig,
        ai_description: impl Into<String>,
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
            ai_description: ai_description.into(),
            config,
            client,
        })
    }
}

#[async_trait]
impl NodeTrait for HttpClientNode {
    impl_node_meta!("httpClient");

    async fn execute(&self, ctx: &ContextRef, store: &dyn DataStore) -> Result<NodeExecution, EngineError> {
        let payload = store.read_mut(&ctx.data_id)?;
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
            prepare_http_request_body(&self.id, &self.config, &payload, &ctx.trace_id, &ctx.timestamp, &requested_at)?;

        let reqwest_method = method.parse::<reqwest::Method>().map_err(|error| {
            EngineError::node_config(self.id.clone(), format!("无效的 HTTP 方法: {error}"))
        })?;

        let mut request = self
            .client
            .request(reqwest_method, &url)
            .timeout(std::time::Duration::from_millis(request_timeout_ms));

        for (key, value) in &self.config.headers {
            request = request.header(key.as_str(), value_to_header_string(value));
        }

        let body_preview = template::truncate(&payload_body, 320);
        if method != "GET" && method != "HEAD" {
            let has_content_type_header = self
                .config
                .headers
                .keys()
                .any(|key| key.eq_ignore_ascii_case("content-type"));
            if !has_content_type_header && !content_type.is_empty() {
                request = request.header("Content-Type", content_type.as_str());
            }
            request = request.body(payload_body);
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
                    "HTTP 请求返回错误状态码 {status_code}: {}",
                    template::truncate(&template::value_to_display_string(&response_value), 240)
                ),
            ));
        }

        let mut payload_map = into_payload_map(payload);
        let http_meta = json!({
            "node_id": self.id,
            "url": url,
            "method": method,
            "webhook_kind": webhook_kind,
            "body_mode": body_mode,
            "content_type": content_type,
            "request_timeout_ms": request_timeout_ms,
            "status": status_code,
            "requested_at": requested_at,
            "request_body_preview": body_preview,
        });
        payload_map.insert("_http".to_owned(), http_meta);
        payload_map.insert("http_response".to_owned(), response_value);

        Ok(NodeExecution::broadcast(Value::Object(payload_map)))
    }
}
