use nazh_core::ai::{
    AiError, AiGenerationParams, AiMessage, AiMessageRole, AiReasoningEffort, AiThinkingConfig,
};
use serde::{Deserialize, Serialize};

use super::provider_policy::is_thinking_enabled;

#[derive(Debug, Serialize)]
pub(super) struct ChatCompletionPayload {
    model: String,
    messages: Vec<ChatMessagePayload>,
    #[serde(skip_serializing_if = "Option::is_none")]
    thinking: Option<AiThinkingConfig>,
    #[serde(skip_serializing_if = "Option::is_none")]
    reasoning_effort: Option<AiReasoningEffort>,
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    max_tokens: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    top_p: Option<f32>,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    stream: bool,
}

#[derive(Debug, Clone, Serialize)]
pub(super) struct ChatMessagePayload {
    pub(super) role: String,
    pub(super) content: String,
}

#[derive(Debug, Deserialize)]
pub(super) struct ChatCompletionApiResponse {
    #[serde(default)]
    pub(super) choices: Vec<ChatChoice>,
    #[serde(default)]
    pub(super) usage: Option<ChatUsage>,
    #[serde(default)]
    pub(super) model: String,
}

#[derive(Debug, Deserialize)]
pub(super) struct ChatChoice {
    pub(super) message: ChatMessageResponse,
    #[serde(default)]
    pub(super) finish_reason: Option<String>,
}

#[derive(Debug, Deserialize)]
pub(super) struct ChatMessageResponse {
    pub(super) content: String,
    #[serde(default)]
    #[allow(dead_code)]
    reasoning_content: Option<String>,
}

#[derive(Debug, Deserialize)]
#[allow(clippy::struct_field_names)]
pub(super) struct ChatUsage {
    pub(super) prompt_tokens: u32,
    pub(super) completion_tokens: u32,
    pub(super) total_tokens: u32,
}

#[derive(Debug, Deserialize)]
pub(super) struct StreamApiResponse {
    pub(super) choices: Vec<StreamChoice>,
}

#[derive(Debug, Deserialize)]
pub(super) struct StreamChoice {
    pub(super) delta: StreamDelta,
    #[serde(default)]
    pub(super) finish_reason: Option<String>,
}

#[derive(Debug, Deserialize)]
pub(super) struct StreamDelta {
    #[serde(default)]
    pub(super) content: Option<String>,
    #[serde(default, alias = "reasoning_content")]
    pub(super) thinking: Option<String>,
}

#[derive(Debug, Deserialize)]
pub(super) struct ApiErrorResponse {
    pub(super) error: Option<ApiErrorDetail>,
}

#[derive(Debug, Deserialize)]
pub(super) struct ApiErrorDetail {
    pub(super) message: String,
}

pub(super) fn build_chat_messages(messages: &[AiMessage]) -> Vec<ChatMessagePayload> {
    messages
        .iter()
        .map(|msg| ChatMessagePayload {
            role: match msg.role {
                AiMessageRole::System => "system".to_owned(),
                AiMessageRole::User => "user".to_owned(),
                AiMessageRole::Assistant => "assistant".to_owned(),
            },
            content: msg.content.clone(),
        })
        .collect()
}

#[allow(clippy::unnecessary_wraps)]
pub(super) fn build_url(base_url: &str, path: &str) -> Result<String, AiError> {
    let trimmed = base_url.trim_end_matches('/');
    Ok(format!("{trimmed}{path}"))
}

pub(super) fn build_chat_payload(
    model: String,
    messages: Vec<ChatMessagePayload>,
    params: &AiGenerationParams,
    stream: bool,
    include_thinking_options: bool,
) -> ChatCompletionPayload {
    let omit_sampling_params = include_thinking_options && is_thinking_enabled(params);

    ChatCompletionPayload {
        model,
        messages,
        thinking: include_thinking_options
            .then(|| params.thinking.clone())
            .flatten(),
        reasoning_effort: include_thinking_options
            .then(|| params.reasoning_effort.clone())
            .flatten(),
        temperature: if omit_sampling_params {
            None
        } else {
            params.temperature
        },
        max_tokens: params.max_tokens,
        top_p: if omit_sampling_params {
            None
        } else {
            params.top_p
        },
        stream,
    }
}
