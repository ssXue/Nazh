use async_openai::error::OpenAIError;
use nazh_core::ai::{AiCompletionResponse, AiError, AiToolCall, AiTokenUsage};

/// 将 `async-openai` 错误映射为 Ring 0 `AiError`。
pub(super) fn map_openai_error(error: OpenAIError) -> AiError {
    match error {
        OpenAIError::ApiError(api_error) => {
            let is_auth = api_error.code.as_deref() == Some("invalid_api_key")
                || api_error.code.as_deref() == Some("authentication_required")
                || api_error.r#type.as_deref() == Some("invalid_request_error")
                    && api_error.message.contains("Incorrect API key");
            if is_auth {
                return AiError::AuthenticationFailed(api_error.message);
            }
            AiError::ApiError {
                status: 0,
                message: api_error.message,
            }
        }
        OpenAIError::Reqwest(err) => {
            if err.is_timeout() {
                AiError::RequestTimeout(0)
            } else {
                AiError::NetworkError(err.to_string())
            }
        }
        OpenAIError::JSONDeserialize(err, content) => AiError::ResponseParseError(format!(
            "{err}；原始响应前 200 字符: {}",
            truncate_preview(&content, 200)
        )),
        OpenAIError::StreamError(stream_error) => {
            AiError::NetworkError(format!("AI 流式响应错误: {stream_error}"))
        }
        _ => AiError::NetworkError(error.to_string()),
    }
}

/// 从 BYOT `serde_json::Value` 响应提取 `AiCompletionResponse`。
pub(super) fn value_to_completion(value: &serde_json::Value) -> AiCompletionResponse {
    let mut content = String::new();
    let mut finish_reason: Option<String> = None;

    if let Some(choice) = value
        .get("choices")
        .and_then(|c| c.as_array())
        .and_then(|a| a.first())
    {
        content = choice
            .get("message")
            .and_then(|m| m.get("content"))
            .and_then(|c| c.as_str())
            .unwrap_or_default()
            .to_string();
        finish_reason = choice
            .get("finish_reason")
            .and_then(|f| f.as_str())
            .filter(|s| !s.is_empty())
            .map(String::from);
    }

    let usage = value.get("usage").and_then(|u| {
        Some(AiTokenUsage {
            prompt_tokens: u.get("prompt_tokens")?.as_u64()?.try_into().ok()?,
            completion_tokens: u.get("completion_tokens")?.as_u64()?.try_into().ok()?,
            total_tokens: u.get("total_tokens")?.as_u64()?.try_into().ok()?,
        })
    });

    // 从非流式响应中提取 tool_calls
    let tool_calls = value
        .get("choices")
        .and_then(|c| c.as_array())
        .and_then(|a| a.first())
        .and_then(|choice| choice.get("message"))
        .and_then(|msg| msg.get("tool_calls"))
        .and_then(|tc| tc.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|tc| {
                    let id = tc.get("id")?.as_str()?;
                    let name = tc.get("function")?.get("name")?.as_str()?;
                    let arguments = tc.get("function")?.get("arguments")?.as_str()?;
                    Some(AiToolCall {
                        id: id.to_owned(),
                        name: name.to_owned(),
                        arguments: arguments.to_owned(),
                    })
                })
                .collect::<Vec<_>>()
        })
        .filter(|v: &Vec<AiToolCall>| !v.is_empty());

    let model = value
        .get("model")
        .and_then(|m| m.as_str())
        .unwrap_or_default()
        .to_owned();

    if finish_reason.as_deref() == Some("length") {
        tracing::warn!(
            content_len = content.len(),
            prompt_tokens = usage.as_ref().map_or(0, |u| u.prompt_tokens),
            completion_tokens = usage.as_ref().map_or(0, |u| u.completion_tokens),
            "AI 输出因 max_tokens 截断，返回内容可能不完整"
        );
    } else {
        tracing::info!(
            content_len = content.len(),
            prompt_tokens = usage.as_ref().map_or(0, |u| u.prompt_tokens),
            completion_tokens = usage.as_ref().map_or(0, |u| u.completion_tokens),
            "AI completion 响应成功"
        );
    }

    AiCompletionResponse {
        content,
        usage,
        model,
        tool_calls,
    }
}

fn truncate_preview(text: &str, max_chars: usize) -> String {
    let mut preview = String::new();
    for (index, ch) in text.chars().enumerate() {
        if index >= max_chars {
            return format!("{preview}…（共 {} 字节）", text.len());
        }
        preview.push(ch);
    }
    preview
}
