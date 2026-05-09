use serde_json::{Map, Value, json};
use uuid::Uuid;

use nazh_core::EngineError;

use crate::template::{self, TemplateVars};

use super::config::{BarkPushNodeConfig, default_bark_body_template, default_bark_server_url};

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

pub(super) fn resolve_bark_endpoint(server_url: &str, key_or_url: &str) -> Result<String, String> {
    let normalized = key_or_url.trim();
    if normalized.is_empty() {
        return Err("Bark 节点需要配置设备 Key 或推送 URL".to_owned());
    }

    if normalized.starts_with("http://") || normalized.starts_with("https://") {
        let url = url::Url::parse(normalized)
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
            return Err(
                "请填写设备 Key，或使用形如 https://api.day.app/{key} 的 Bark URL".to_owned(),
            );
        }

        let mut endpoint = url;
        endpoint.set_query(None);
        endpoint.set_fragment(None);
        endpoint
            .path_segments_mut()
            .map_err(|()| "无法解析 Bark 推送 URL".to_owned())?
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

pub(super) fn build_bark_request_body(
    config: &BarkPushNodeConfig,
    vars: &TemplateVars<'_>,
    content_mode: &str,
    level: &str,
    archive_mode: &str,
    node_id: &str,
    trace_id: Uuid,
) -> Result<Value, EngineError> {
    let title = render_optional_template(&config.title_template, vars);
    let subtitle = render_optional_template(&config.subtitle_template, vars);
    let content = render_optional_template(
        if config.body_template.trim().is_empty() {
            default_bark_body_template()
        } else {
            config.body_template.as_str()
        },
        vars,
    );
    let badge = parse_badge_value(node_id, trace_id, &config.badge)?;
    let sound = render_optional_template(&config.sound, vars);
    let icon = render_optional_template(&config.icon, vars);
    let group = render_optional_template(&config.group, vars);
    let jump_url = render_optional_template(&config.url, vars);
    let copy = render_optional_template(&config.copy, vars);
    let image = render_optional_template(&config.image, vars);

    let mut request_body = Map::new();
    if let Some(value) = title {
        request_body.insert("title".to_owned(), Value::String(value));
    }
    if let Some(value) = subtitle {
        request_body.insert("subtitle".to_owned(), Value::String(value));
    }
    if let Some(value) = content {
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
    if let Some(value) = sound {
        request_body.insert("sound".to_owned(), Value::String(value));
    }
    if let Some(value) = icon {
        request_body.insert("icon".to_owned(), Value::String(value));
    }
    if let Some(value) = group {
        request_body.insert("group".to_owned(), Value::String(value));
    }
    if let Some(value) = jump_url {
        request_body.insert("url".to_owned(), Value::String(value));
    }
    if let Some(value) = copy {
        request_body.insert("copy".to_owned(), Value::String(value));
    }
    if let Some(value) = image {
        request_body.insert("image".to_owned(), Value::String(value));
    }

    request_body.insert("level".to_owned(), Value::String(level.to_owned()));
    if config.auto_copy {
        request_body.insert("autoCopy".to_owned(), Value::String("1".to_owned()));
    }
    if config.call {
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

    Ok(Value::Object(request_body))
}

pub(super) fn validate_bark_response(
    status_code: u16,
    response_value: &Value,
) -> Result<(), String> {
    if status_code >= 400 {
        return Err(format!(
            "Bark 推送返回错误状态码 {status_code}: {}",
            template::truncate(&template::value_to_display_string(response_value), 240)
        ));
    }

    if let Some(code) = response_value.get("code").and_then(Value::as_i64)
        && code != 200
    {
        let message = response_value
            .get("message")
            .and_then(Value::as_str)
            .map_or_else(|| "Bark 服务返回业务错误".to_owned(), str::to_owned);
        return Err(format!("Bark 推送失败: {message} (code={code})"));
    }

    Ok(())
}

pub(super) struct BarkPushResult {
    pub(super) status_code: u16,
    pub(super) response_value: Value,
    pub(super) request_body_preview: String,
}

pub(super) async fn send_bark_push(
    client: &reqwest::Client,
    endpoint: &str,
    request_body_value: &Value,
    request_timeout_ms: u64,
) -> Result<BarkPushResult, String> {
    let request_body_preview =
        template::truncate(&template::value_to_display_string(request_body_value), 320);

    let response = client
        .post(endpoint)
        .timeout(std::time::Duration::from_millis(request_timeout_ms))
        .header("Content-Type", "application/json; charset=utf-8")
        .json(request_body_value)
        .send()
        .await
        .map_err(|error| format!("Bark 推送失败: {error}"))?;

    let status_code = response.status().as_u16();
    let response_body = response
        .text()
        .await
        .map_err(|error| format!("读取 Bark 响应体失败: {error}"))?;

    Ok(BarkPushResult {
        status_code,
        response_value: parse_json_or_string(&response_body),
        request_body_preview,
    })
}
