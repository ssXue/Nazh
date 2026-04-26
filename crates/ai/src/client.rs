//! `OpenAI` 兼容 HTTP 客户端：`AiService`（Ring 0）的 reqwest + SSE 实现。

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use nazh_core::ai::{
    AiCompletionRequest, AiCompletionResponse, AiError, AiGenerationParams, AiMessage,
    AiMessageRole, AiReasoningEffort, AiService, AiThinkingConfig, AiThinkingMode, AiTokenUsage,
    StreamChunk,
};
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;

use crate::config::{AiConfigFile, AiProviderDraft, AiProviderSecretRecord};
use crate::types::AiTestResult;

const DEFAULT_TIMEOUT_MS: u64 = 30_000;
const TEST_MAX_TOKENS: u32 = 1;

/// `OpenAI` 兼容 API 客户端。
pub struct OpenAiCompatibleService {
    config: Arc<RwLock<AiConfigFile>>,
    http: reqwest::Client,
}

impl OpenAiCompatibleService {
    /// 创建新的客户端实例。
    pub fn new(config: Arc<RwLock<AiConfigFile>>) -> Self {
        let http = reqwest::Client::builder()
            .timeout(Duration::from_mins(1))
            .build()
            .unwrap_or_default();
        Self { config, http }
    }

    async fn resolve_provider(&self, provider_id: &str) -> Result<ResolvedProvider, AiError> {
        let config = self.config.read().await;
        let provider = AiProviderSecretRecord::find_active_by_id(&config.providers, provider_id)?;
        Ok(ResolvedProvider {
            base_url: provider.base_url.clone(),
            api_key: provider.api_key.clone(),
            default_model: provider.default_model.clone(),
            extra_headers: provider.extra_headers.clone(),
        })
    }

    async fn resolve_draft(&self, draft: &AiProviderDraft) -> Result<ResolvedProvider, AiError> {
        if let Some(ref api_key) = draft.api_key
            && !api_key.trim().is_empty()
        {
            return Ok(ResolvedProvider {
                base_url: draft.base_url.clone(),
                api_key: api_key.clone(),
                default_model: draft.default_model.clone(),
                extra_headers: draft.extra_headers.clone(),
            });
        }

        if let Some(ref id) = draft.id {
            let config = self.config.read().await;
            let provider = AiProviderSecretRecord::find_by_id(&config.providers, id)?;
            return Ok(ResolvedProvider {
                base_url: if draft.base_url.trim().is_empty() {
                    provider.base_url.clone()
                } else {
                    draft.base_url.clone()
                },
                api_key: provider.api_key.clone(),
                default_model: if draft.default_model.trim().is_empty() {
                    provider.default_model.clone()
                } else {
                    draft.default_model.clone()
                },
                extra_headers: if draft.extra_headers.is_empty() {
                    provider.extra_headers.clone()
                } else {
                    draft.extra_headers.clone()
                },
            });
        }

        Err(AiError::InvalidConfig(
            "草稿配置缺少 api_key 且未指定已有提供商 id".to_owned(),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_provider(base_url: &str, default_model: &str) -> ResolvedProvider {
        ResolvedProvider {
            base_url: base_url.to_owned(),
            api_key: "sk-test".to_owned(),
            default_model: default_model.to_owned(),
            extra_headers: HashMap::new(),
        }
    }

    fn test_messages() -> Vec<ChatMessagePayload> {
        vec![ChatMessagePayload {
            role: "user".to_owned(),
            content: "Hi".to_owned(),
        }]
    }

    #[test]
    fn deepseek_payload_sends_thinking_options_and_omits_sampling_when_enabled() {
        let provider = test_provider("https://api.deepseek.com", "deepseek-v4-pro");
        let params = AiGenerationParams {
            temperature: Some(0.8),
            max_tokens: Some(256),
            top_p: Some(0.9),
            thinking: Some(AiThinkingConfig {
                kind: AiThinkingMode::Enabled,
            }),
            reasoning_effort: Some(AiReasoningEffort::Max),
        };

        let payload = build_chat_payload(
            provider.default_model.clone(),
            test_messages(),
            &params,
            false,
            provider_accepts_deepseek_options(&provider, &provider.default_model),
        );
        let Ok(json) = serde_json::to_value(payload) else {
            panic!("payload serializes");
        };

        assert_eq!(json["model"], "deepseek-v4-pro");
        assert_eq!(json["thinking"]["type"], "enabled");
        assert_eq!(json["reasoning_effort"], "max");
        assert_eq!(json["max_tokens"], 256);
        assert!(json.get("temperature").is_none());
        assert!(json.get("top_p").is_none());
    }

    #[test]
    fn non_deepseek_payload_omits_deepseek_specific_options() {
        let provider = test_provider("https://api.openai.com/v1", "gpt-4o-mini");
        let params = AiGenerationParams {
            temperature: Some(0.3),
            max_tokens: Some(128),
            top_p: Some(0.8),
            thinking: Some(AiThinkingConfig {
                kind: AiThinkingMode::Enabled,
            }),
            reasoning_effort: Some(AiReasoningEffort::High),
        };

        let payload = build_chat_payload(
            provider.default_model.clone(),
            test_messages(),
            &params,
            false,
            provider_accepts_deepseek_options(&provider, &provider.default_model),
        );
        let Ok(json) = serde_json::to_value(payload) else {
            panic!("payload serializes");
        };

        assert!(json.get("thinking").is_none());
        assert!(json.get("reasoning_effort").is_none());
        assert!((json["temperature"].as_f64().unwrap_or_default() - 0.3).abs() < 0.001);
        assert!((json["top_p"].as_f64().unwrap_or_default() - 0.8).abs() < 0.001);
    }

    #[test]
    fn deepseek_connection_test_disables_thinking_for_lightweight_probe() {
        let provider = test_provider("https://api.deepseek.com", "deepseek-v4-flash");
        let params = build_connection_test_params(provider_accepts_deepseek_options(
            &provider,
            &provider.default_model,
        ));
        let payload = build_chat_payload(
            provider.default_model.clone(),
            test_messages(),
            &params,
            false,
            provider_accepts_deepseek_options(&provider, &provider.default_model),
        );
        let Ok(json) = serde_json::to_value(payload) else {
            panic!("payload serializes");
        };

        assert_eq!(json["thinking"]["type"], "disabled");
        assert_eq!(json["temperature"], 0.0);
        assert_eq!(json["max_tokens"], TEST_MAX_TOKENS);
    }
}

struct ResolvedProvider {
    base_url: String,
    api_key: String,
    default_model: String,
    extra_headers: HashMap<String, String>,
}

#[derive(Debug, Serialize)]
struct ChatCompletionPayload {
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

#[derive(Debug, Serialize)]
struct ChatMessagePayload {
    role: String,
    content: String,
}

#[derive(Debug, Deserialize)]
struct ChatCompletionApiResponse {
    choices: Vec<ChatChoice>,
    #[serde(default)]
    usage: Option<ChatUsage>,
    model: String,
}

#[derive(Debug, Deserialize)]
struct ChatChoice {
    message: ChatMessageResponse,
}

#[derive(Debug, Deserialize)]
struct ChatMessageResponse {
    content: String,
}

#[derive(Debug, Deserialize)]
#[allow(clippy::struct_field_names)]
struct ChatUsage {
    prompt_tokens: u32,
    completion_tokens: u32,
    total_tokens: u32,
}

#[derive(Debug, Deserialize)]
struct StreamApiResponse {
    choices: Vec<StreamChoice>,
}

#[derive(Debug, Deserialize)]
struct StreamChoice {
    delta: StreamDelta,
    #[serde(default)]
    finish_reason: Option<String>,
}

#[derive(Debug, Deserialize)]
struct StreamDelta {
    #[serde(default)]
    content: Option<String>,
    #[serde(default, alias = "reasoning_content")]
    thinking: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ApiErrorResponse {
    error: Option<ApiErrorDetail>,
}

#[derive(Debug, Deserialize)]
struct ApiErrorDetail {
    message: String,
}

fn build_chat_messages(messages: &[AiMessage]) -> Vec<ChatMessagePayload> {
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
fn build_url(base_url: &str, path: &str) -> Result<String, AiError> {
    let trimmed = base_url.trim_end_matches('/');
    Ok(format!("{trimmed}{path}"))
}

fn provider_accepts_deepseek_options(provider: &ResolvedProvider, model: &str) -> bool {
    let base_url = provider.base_url.to_ascii_lowercase();
    let model = model.to_ascii_lowercase();
    base_url.contains("deepseek.com") || model.starts_with("deepseek-")
}

fn is_thinking_enabled(params: &AiGenerationParams) -> bool {
    params
        .thinking
        .as_ref()
        .is_some_and(|thinking| thinking.kind == AiThinkingMode::Enabled)
}

fn build_chat_payload(
    model: String,
    messages: Vec<ChatMessagePayload>,
    params: &AiGenerationParams,
    stream: bool,
    include_deepseek_options: bool,
) -> ChatCompletionPayload {
    let omit_sampling_params = include_deepseek_options && is_thinking_enabled(params);

    ChatCompletionPayload {
        model,
        messages,
        thinking: include_deepseek_options
            .then(|| params.thinking.clone())
            .flatten(),
        reasoning_effort: include_deepseek_options
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

fn build_connection_test_params(disable_deepseek_thinking: bool) -> AiGenerationParams {
    AiGenerationParams {
        temperature: Some(0.0),
        max_tokens: Some(TEST_MAX_TOKENS),
        top_p: None,
        thinking: disable_deepseek_thinking.then_some(AiThinkingConfig {
            kind: AiThinkingMode::Disabled,
        }),
        reasoning_effort: None,
    }
}

fn parse_api_error(status: u16, body: &str) -> AiError {
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

async fn emit_stream_line(
    line: &str,
    tx: &tokio::sync::mpsc::Sender<Result<StreamChunk, AiError>>,
    saw_explicit_completion: &mut bool,
) -> bool {
    let trimmed = line.trim();
    if trimmed.is_empty() || trimmed == ":" {
        return true;
    }

    if trimmed == "data: [DONE]" {
        *saw_explicit_completion = true;
        if tx
            .send(Ok(StreamChunk {
                delta: String::new(),
                thinking: None,
                finish_reason: None,
                done: true,
            }))
            .await
            .is_err()
        {
            return false;
        }
        return false;
    }

    let Some(data) = trimmed.strip_prefix("data: ") else {
        return true;
    };

    let parsed: StreamApiResponse = match serde_json::from_str(data) {
        Ok(value) => value,
        Err(_) => return true,
    };

    let Some(choice) = parsed.choices.first() else {
        return true;
    };

    let content_delta = choice.delta.content.clone().unwrap_or_default();
    let thinking_delta = choice.delta.thinking.clone();
    let has_content = !content_delta.is_empty();
    let has_thinking = thinking_delta
        .as_ref()
        .is_some_and(|value| !value.is_empty());
    let is_done = choice
        .finish_reason
        .as_deref()
        .map(str::trim)
        .is_some_and(|value| !value.is_empty());
    let finish_reason = choice
        .finish_reason
        .clone()
        .filter(|value| !value.trim().is_empty());

    if is_done {
        *saw_explicit_completion = true;
    }

    if has_content || has_thinking || is_done {
        return tx
            .send(Ok(StreamChunk {
                delta: content_delta,
                thinking: if has_thinking { thinking_delta } else { None },
                finish_reason,
                done: is_done,
            }))
            .await
            .is_ok()
            && !is_done;
    }

    true
}

fn build_stream_body_decode_error(
    error: &reqwest::Error,
    chunk_count: usize,
    byte_count: usize,
) -> AiError {
    let detail = error.to_string();
    let hint = if detail.contains("error decoding response body") {
        "上游流式响应在传输过程中中断或损坏，常见于代理/网关提前断开 chunked SSE 响应。"
    } else {
        "上游流式响应在读取过程中失败，可能是网络抖动或提供商提前断开连接。"
    };

    AiError::NetworkError(format!(
        "{detail}；已接收 {chunk_count} 个分块 / {byte_count} 字节。{hint}"
    ))
}

#[async_trait]
impl AiService for OpenAiCompatibleService {
    async fn complete(
        &self,
        request: AiCompletionRequest,
    ) -> Result<AiCompletionResponse, AiError> {
        let provider = self.resolve_provider(&request.provider_id).await?;
        if provider.api_key.trim().is_empty() {
            return Err(AiError::InvalidConfig(format!(
                "提供商 `{}` 未配置 API Key",
                request.provider_id
            )));
        }

        let model = request.model.as_deref().unwrap_or(&provider.default_model);
        let timeout_ms = request.timeout_ms.unwrap_or(DEFAULT_TIMEOUT_MS);

        let body = build_chat_payload(
            model.to_owned(),
            build_chat_messages(&request.messages),
            &request.params,
            false,
            provider_accepts_deepseek_options(&provider, model),
        );

        let url = build_url(&provider.base_url, "/chat/completions")?;

        let mut builder = self
            .http
            .post(&url)
            .bearer_auth(&provider.api_key)
            .json(&body);

        for (key, value) in &provider.extra_headers {
            builder = builder.header(key.as_str(), value.as_str());
        }

        let response = tokio::time::timeout(Duration::from_millis(timeout_ms), builder.send())
            .await
            .map_err(|_| AiError::RequestTimeout(timeout_ms))?
            .map_err(|error| AiError::NetworkError(error.to_string()))?;

        let status = response.status().as_u16();
        if !response.status().is_success() {
            let body = response.text().await.unwrap_or_default();
            return Err(parse_api_error(status, &body));
        }

        let chat_response: ChatCompletionApiResponse = response
            .json()
            .await
            .map_err(|error| AiError::ResponseParseError(error.to_string()))?;

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

        Ok(AiCompletionResponse {
            content,
            usage,
            model: chat_response.model,
        })
    }

    /// 流式 chat completion，逐 chunk 通过 channel 返回。
    async fn stream_complete(
        &self,
        request: AiCompletionRequest,
    ) -> Result<tokio::sync::mpsc::Receiver<Result<StreamChunk, AiError>>, AiError> {
        let provider = self.resolve_provider(&request.provider_id).await?;
        if provider.api_key.trim().is_empty() {
            return Err(AiError::InvalidConfig(format!(
                "提供商 `{}` 未配置 API Key",
                request.provider_id
            )));
        }

        let model = request
            .model
            .clone()
            .unwrap_or(provider.default_model.clone());
        let timeout_ms = request.timeout_ms.unwrap_or(DEFAULT_TIMEOUT_MS);

        let body = build_chat_payload(
            model.clone(),
            build_chat_messages(&request.messages),
            &request.params,
            true,
            provider_accepts_deepseek_options(&provider, &model),
        );

        let url = build_url(&provider.base_url, "/chat/completions")?;

        let mut builder = self
            .http
            .post(&url)
            .bearer_auth(&provider.api_key)
            .json(&body);

        for (key, value) in &provider.extra_headers {
            builder = builder.header(key.as_str(), value.as_str());
        }

        let response = tokio::time::timeout(Duration::from_millis(timeout_ms), builder.send())
            .await
            .map_err(|_| AiError::RequestTimeout(timeout_ms))?
            .map_err(|error| AiError::NetworkError(error.to_string()))?;

        let status = response.status().as_u16();
        if !response.status().is_success() {
            let body = response.text().await.unwrap_or_default();
            return Err(parse_api_error(status, &body));
        }

        let (tx, rx) = tokio::sync::mpsc::channel(32);

        tokio::spawn(async move {
            use futures_util::StreamExt;
            let mut stream = response.bytes_stream();
            let mut buffer = String::new();
            let newline = char::from(10);
            let mut saw_explicit_completion = false;
            let mut received_chunk_count = 0_usize;
            let mut received_byte_count = 0_usize;

            while let Some(chunk_result) = stream.next().await {
                let chunk = match chunk_result {
                    Ok(bytes) => bytes,
                    Err(error) => {
                        let _ = tx
                            .send(Err(build_stream_body_decode_error(
                                &error,
                                received_chunk_count,
                                received_byte_count,
                            )))
                            .await;
                        return;
                    }
                };
                received_chunk_count += 1;
                received_byte_count += chunk.len();

                buffer.push_str(&String::from_utf8_lossy(&chunk));

                while let Some(nl_pos) = buffer.find(newline) {
                    let line = buffer[..nl_pos].trim().to_owned();
                    buffer = buffer[nl_pos + 1..].to_owned();

                    if !emit_stream_line(&line, &tx, &mut saw_explicit_completion).await {
                        return;
                    }
                }
            }

            if !buffer.trim().is_empty()
                && !emit_stream_line(&buffer, &tx, &mut saw_explicit_completion).await
            {
                return;
            }

            if !saw_explicit_completion {
                let _ = tx
                    .send(Err(AiError::NetworkError(
                        "AI 流式输出意外中断，未收到结束信号".to_owned(),
                    )))
                    .await;
            }
        });

        Ok(rx)
    }
}

impl OpenAiCompatibleService {
    /// 测试提供商连通性。inherent 而非 trait 方法——`AiProviderDraft`
    /// 是壳层配置类型，不属于 Ring 0 的运行时关注点。
    pub async fn test_connection(&self, draft: AiProviderDraft) -> Result<AiTestResult, AiError> {
        let provider = self.resolve_draft(&draft).await?;
        if provider.api_key.trim().is_empty() {
            return Err(AiError::InvalidConfig(
                "测试连接需要提供 API Key 或引用已保存密钥的提供商".to_owned(),
            ));
        }

        let url = build_url(&provider.base_url, "/chat/completions")?;

        let model = provider.default_model.clone();
        let body = build_chat_payload(
            model.clone(),
            vec![ChatMessagePayload {
                role: "user".to_owned(),
                content: "Hi".to_owned(),
            }],
            &build_connection_test_params(provider_accepts_deepseek_options(&provider, &model)),
            false,
            provider_accepts_deepseek_options(&provider, &model),
        );

        let mut builder = self
            .http
            .post(&url)
            .bearer_auth(&provider.api_key)
            .json(&body);

        for (key, value) in &provider.extra_headers {
            builder = builder.header(key.as_str(), value.as_str());
        }

        let started_at = std::time::Instant::now();

        let response = tokio::time::timeout(Duration::from_secs(15), builder.send())
            .await
            .map_err(|_| AiError::RequestTimeout(15_000))?
            .map_err(|error| AiError::NetworkError(error.to_string()))?;

        #[allow(clippy::cast_possible_truncation)]
        let latency_ms = started_at.elapsed().as_millis() as u64;

        let status = response.status().as_u16();
        if !response.status().is_success() {
            let body = response.text().await.unwrap_or_default();
            let error = parse_api_error(status, &body);
            return Ok(AiTestResult {
                success: false,
                message: error.to_string(),
                latency_ms: Some(latency_ms),
            });
        }

        Ok(AiTestResult {
            success: true,
            message: format!(
                "连接成功（模型 {}，延迟 {} ms）",
                provider.default_model, latency_ms
            ),
            latency_ms: Some(latency_ms),
        })
    }
}
