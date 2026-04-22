//! Bark 推送节点：向 Bark 服务发送 iOS 推送通知。
//!
//! 默认使用 `POST https://api.day.app/{device_key}` 的 JSON 请求格式。
//! `device_key` 字段也支持直接粘贴形如 `https://api.day.app/{key}` 的 URL，
//! 节点会自动提取其中的 key 作为目标端点。

use async_trait::async_trait;
use chrono::Utc;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};
use uuid::Uuid;

use crate::template::{self, TemplateVars};
use nazh_core::{EngineError, NodeExecution, NodeTrait, into_payload_map};

fn default_bark_server_url() -> String {
    "https://api.day.app".to_owned()
}

fn default_bark_content_mode() -> String {
    "body".to_owned()
}

fn default_bark_level() -> String {
    "active".to_owned()
}

fn default_bark_archive_mode() -> String {
    "inherit".to_owned()
}

fn default_bark_request_timeout_ms() -> u64 {
    4_000
}

fn default_bark_title_template() -> &'static str {
    "Nazh 通知 · {{payload.tag}}"
}

fn default_bark_body_template() -> &'static str {
    "{{payload}}"
}

fn normalize_bark_content_mode(value: &str) -> &'static str {
    match value.trim().to_ascii_lowercase().as_str() {
        "markdown" | "md" => "markdown",
        _ => "body",
    }
}

fn normalize_bark_level(value: &str) -> &'static str {
    match value.trim().to_ascii_lowercase().as_str() {
        "critical" => "critical",
        "timesensitive" | "time_sensitive" | "time-sensitive" => "timeSensitive",
        "passive" => "passive",
        _ => "active",
    }
}

fn normalize_bark_archive_mode(value: &str) -> &'static str {
    match value.trim().to_ascii_lowercase().as_str() {
        "archive" | "true" | "save" | "1" => "archive",
        "skip" | "false" | "0" | "no_archive" | "no-archive" => "skip",
        _ => "inherit",
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

fn render_optional_template(template_text: &str, vars: &TemplateVars<'_>) -> Option<String> {
    let trimmed = template_text.trim();
    if trimmed.is_empty() {
        return None;
    }

    let rendered = template::render(trimmed, vars);
    if rendered.trim().is_empty() {
        None
    } else {
        Some(rendered)
    }
}

fn parse_badge_value(node_id: &str, trace_id: Uuid, raw: &str) -> Result<Option<i64>, EngineError> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Ok(None);
    }

    let value = trimmed.parse::<i64>().map_err(|error| {
        EngineError::stage_execution(
            node_id.to_owned(),
            trace_id,
            format!("Bark badge 必须是整数: {error}"),
        )
    })?;

    Ok(Some(value))
}

fn resolve_bark_endpoint(server_url: &str, key_or_url: &str) -> Result<String, String> {
    let normalized = key_or_url.trim();
    if normalized.is_empty() {
        return Err("Bark 节点需要配置设备 Key 或推送 URL".to_owned());
    }

    if normalized.starts_with("http://") || normalized.starts_with("https://") {
        let url = reqwest::Url::parse(normalized)
            .map_err(|error| format!("无效的 Bark 推送 URL: {error}"))?;
        let device_key = url
            .path_segments()
            .and_then(|mut segments| {
                segments
                    .find(|segment| !segment.trim().is_empty())
                    .map(str::to_owned)
            })
            .ok_or_else(|| "Bark 推送 URL 缺少设备 Key".to_owned())?;
        if device_key.eq_ignore_ascii_case("push") {
            return Err("请填写设备 Key，或使用形如 https://api.day.app/{key} 的 Bark URL".to_owned());
        }

        let mut endpoint = url;
        endpoint.set_query(None);
        endpoint.set_fragment(None);
        endpoint
            .path_segments_mut()
            .map_err(|_| "无法解析 Bark 推送 URL".to_owned())?
            .clear()
            .push(&device_key);
        return Ok(endpoint.to_string().trim_end_matches('/').to_owned());
    }

    let base = if server_url.trim().is_empty() {
        default_bark_server_url()
    } else {
        server_url.trim().trim_end_matches('/').to_owned()
    };
    Ok(format!("{base}/{}", normalized.trim_matches('/')))
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BarkPushNodeConfig {
    #[serde(default = "default_bark_server_url")]
    pub server_url: String,
    pub device_key: String,
    #[serde(default = "default_bark_content_mode")]
    pub content_mode: String,
    #[serde(default)]
    pub title_template: String,
    #[serde(default)]
    pub subtitle_template: String,
    #[serde(default)]
    pub body_template: String,
    #[serde(default = "default_bark_level")]
    pub level: String,
    #[serde(default)]
    pub badge: String,
    #[serde(default)]
    pub sound: String,
    #[serde(default)]
    pub icon: String,
    #[serde(default)]
    pub group: String,
    #[serde(default)]
    pub url: String,
    #[serde(default)]
    pub copy: String,
    #[serde(default)]
    pub image: String,
    #[serde(default)]
    pub auto_copy: bool,
    #[serde(default)]
    pub call: bool,
    #[serde(default = "default_bark_archive_mode")]
    pub archive_mode: String,
    #[serde(default = "default_bark_request_timeout_ms")]
    pub request_timeout_ms: u64,
}

impl Default for BarkPushNodeConfig {
    fn default() -> Self {
        Self {
            server_url: default_bark_server_url(),
            device_key: String::new(),
            content_mode: default_bark_content_mode(),
            title_template: default_bark_title_template().to_owned(),
            subtitle_template: String::new(),
            body_template: default_bark_body_template().to_owned(),
            level: default_bark_level(),
            badge: String::new(),
            sound: String::new(),
            icon: String::new(),
            group: String::new(),
            url: String::new(),
            copy: String::new(),
            image: String::new(),
            auto_copy: false,
            call: false,
            archive_mode: default_bark_archive_mode(),
            request_timeout_ms: default_bark_request_timeout_ms(),
        }
    }
}

pub struct BarkPushNode {
    id: String,
    config: BarkPushNodeConfig,
    client: reqwest::Client,
}

impl BarkPushNode {
    pub fn new(id: impl Into<String>, config: BarkPushNodeConfig) -> Result<Self, EngineError> {
        let id = id.into();
        let client = reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::limited(10))
            .build()
            .map_err(|error| {
                EngineError::node_config(id.clone(), format!("Bark 客户端初始化失败: {error}"))
            })?;
        Ok(Self { id, config, client })
    }
}

#[async_trait]
impl NodeTrait for BarkPushNode {
    nazh_core::impl_node_meta!("barkPush");

    async fn transform(
        &self,
        trace_id: Uuid,
        payload: Value,
    ) -> Result<NodeExecution, EngineError> {
        let endpoint = resolve_bark_endpoint(&self.config.server_url, &self.config.device_key)
            .map_err(|message| EngineError::node_config(self.id.clone(), message))?;
        let now = Utc::now();
        let requested_at = now.to_rfc3339();
        let event_timestamp = now.to_rfc3339();
        let content_mode = normalize_bark_content_mode(&self.config.content_mode);
        let level = normalize_bark_level(&self.config.level);
        let archive_mode = normalize_bark_archive_mode(&self.config.archive_mode);
        let request_timeout_ms = self.config.request_timeout_ms.max(500);

        let vars = TemplateVars {
            payload: &payload,
            trace_id: &trace_id,
            node_id: &self.id,
            timestamp: &event_timestamp,
            extras: &[("requested_at", requested_at.as_str())],
        };

        let title = render_optional_template(&self.config.title_template, &vars);
        let subtitle = render_optional_template(&self.config.subtitle_template, &vars);
        let content = render_optional_template(
            if self.config.body_template.trim().is_empty() {
                default_bark_body_template()
            } else {
                self.config.body_template.as_str()
            },
            &vars,
        );
        let badge = parse_badge_value(&self.id, trace_id, &self.config.badge)?;
        let sound = render_optional_template(&self.config.sound, &vars);
        let icon = render_optional_template(&self.config.icon, &vars);
        let group = render_optional_template(&self.config.group, &vars);
        let jump_url = render_optional_template(&self.config.url, &vars);
        let copy = render_optional_template(&self.config.copy, &vars);
        let image = render_optional_template(&self.config.image, &vars);

        let mut request_body = Map::new();
        if let Some(value) = title.clone() {
            request_body.insert("title".to_owned(), Value::String(value));
        }
        if let Some(value) = subtitle.clone() {
            request_body.insert("subtitle".to_owned(), Value::String(value));
        }
        if let Some(value) = content.clone() {
            request_body.insert(
                if content_mode == "markdown" {
                    "markdown".to_owned()
                } else {
                    "body".to_owned()
                },
                Value::String(value),
            );
        }
        if let Some(value) = badge {
            request_body.insert("badge".to_owned(), json!(value));
        }
        if let Some(value) = sound.clone() {
            request_body.insert("sound".to_owned(), Value::String(value));
        }
        if let Some(value) = icon.clone() {
            request_body.insert("icon".to_owned(), Value::String(value));
        }
        if let Some(value) = group.clone() {
            request_body.insert("group".to_owned(), Value::String(value));
        }
        if let Some(value) = jump_url.clone() {
            request_body.insert("url".to_owned(), Value::String(value));
        }
        if let Some(value) = copy.clone() {
            request_body.insert("copy".to_owned(), Value::String(value));
        }
        if let Some(value) = image.clone() {
            request_body.insert("image".to_owned(), Value::String(value));
        }

        request_body.insert("level".to_owned(), Value::String(level.to_owned()));
        if self.config.auto_copy {
            request_body.insert("autoCopy".to_owned(), Value::String("1".to_owned()));
        }
        if self.config.call {
            request_body.insert("call".to_owned(), Value::String("1".to_owned()));
        }
        match archive_mode {
            "archive" => {
                request_body.insert("isArchive".to_owned(), json!(1));
            }
            "skip" => {
                request_body.insert("isArchive".to_owned(), json!(0));
            }
            _ => {}
        }

        let request_body_value = Value::Object(request_body);
        let request_body_preview = template::truncate(
            &template::value_to_display_string(&request_body_value),
            320,
        );

        let response = self
            .client
            .post(&endpoint)
            .timeout(std::time::Duration::from_millis(request_timeout_ms))
            .header("Content-Type", "application/json; charset=utf-8")
            .json(&request_body_value)
            .send()
            .await
            .map_err(|error| {
                EngineError::stage_execution(
                    self.id.clone(),
                    trace_id,
                    format!("Bark 推送失败: {error}"),
                )
            })?;

        let status_code = response.status().as_u16();
        let response_body = response.text().await.map_err(|error| {
            EngineError::stage_execution(
                self.id.clone(),
                trace_id,
                format!("读取 Bark 响应体失败: {error}"),
            )
        })?;
        let response_value = parse_json_or_string(&response_body);

        if status_code >= 400 {
            return Err(EngineError::stage_execution(
                self.id.clone(),
                trace_id,
                format!(
                    "Bark 推送返回错误状态码 {status_code}: {}",
                    template::truncate(&template::value_to_display_string(&response_value), 240)
                ),
            ));
        }

        if let Some(code) = response_value.get("code").and_then(Value::as_i64) {
            if code != 200 {
                let message = response_value
                    .get("message")
                    .and_then(Value::as_str)
                    .map_or_else(|| "Bark 服务返回业务错误".to_owned(), str::to_owned);
                return Err(EngineError::stage_execution(
                    self.id.clone(),
                    trace_id,
                    format!("Bark 推送失败: {message} (code={code})"),
                ));
            }
        }

        let mut payload_map = into_payload_map(payload);
        payload_map.insert("bark_response".to_owned(), response_value.clone());

        let bark_metadata = json!({
            "node_id": self.id,
            "endpoint": endpoint,
            "content_mode": content_mode,
            "level": level,
            "request_timeout_ms": request_timeout_ms,
            "requested_at": requested_at,
        });
        let metadata = serde_json::Map::from_iter([
            (
                "http".to_owned(),
                json!({
                    "node_id": self.id,
                    "url": endpoint,
                    "method": "POST",
                    "webhook_kind": "bark",
                    "body_mode": content_mode,
                    "content_type": "application/json",
                    "request_timeout_ms": request_timeout_ms,
                    "status": status_code,
                    "requested_at": requested_at,
                    "request_body_preview": request_body_preview,
                }),
            ),
            ("bark".to_owned(), bark_metadata),
        ]);

        Ok(NodeExecution::broadcast(Value::Object(payload_map)).with_metadata(metadata))
    }
}
