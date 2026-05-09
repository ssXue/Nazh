use nazh_core::ai::{AiCompletionResponse, AiError, AiTokenUsage};

use super::protocol::{ApiErrorResponse, ChatCompletionApiResponse};

pub(super) fn response_preview(text: &str) -> String {
    const PREVIEW_CHARS: usize = 200;

    let mut preview = String::new();
    for (index, ch) in text.chars().enumerate() {
        if index >= PREVIEW_CHARS {
            return format!("{preview}...（共 {} 字节）", text.len());
        }
        preview.push(ch);
    }
    preview
}

pub(super) fn parse_chat_response(body_text: &str) -> Result<ChatCompletionApiResponse, AiError> {
    serde_json::from_str(body_text).map_err(|error| {
        AiError::ResponseParseError(format!(
            "{error}；原始响应: {}；\
             常见原因：AI 输出超过 max_tokens 被截断导致 JSON 不完整，\
             或上游返回了非标准 OpenAI 格式",
            response_preview(body_text)
        ))
    })
}

pub(super) fn parse_api_error(status: u16, body: &str) -> AiError {
    if let Ok(error_response) = serde_json::from_str::<ApiErrorResponse>(body) {
        let message = error_response
            .error
            .map_or_else(|| body.to_owned(), |detail| detail.message);
        if status == 401 || status == 403 {
            return AiError::AuthenticationFailed(message);
        }
        return AiError::ApiError { status, message };
    }
    AiError::ApiError {
        status,
        message: body.to_owned(),
    }
}

pub(super) fn chat_response_to_completion(
    chat_response: ChatCompletionApiResponse,
) -> AiCompletionResponse {
    let content = chat_response
        .choices
        .into_iter()
        .next()
        .map(|choice| choice.message.content)
        .unwrap_or_default();

    let usage = chat_response.usage.map(|u| AiTokenUsage {
        prompt_tokens: u.prompt_tokens,
        completion_tokens: u.completion_tokens,
        total_tokens: u.total_tokens,
    });

    tracing::info!(
        content_len = content.len(),
        prompt_tokens = usage.as_ref().map_or(0, |u| u.prompt_tokens),
        completion_tokens = usage.as_ref().map_or(0, |u| u.completion_tokens),
        "AI completion 响应成功"
    );

    AiCompletionResponse {
        content,
        usage,
        model: chat_response.model,
    }
}
