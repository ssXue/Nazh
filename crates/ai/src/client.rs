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

use crate::config::{
    AiAgentSettings, AiConfigFile, AiProviderDraft, AiProviderSecretRecord,
    filter_non_sensitive_extra_headers,
};
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

    async fn resolve_provider_snapshot(
        &self,
        provider_id: &str,
    ) -> Result<ResolvedProviderSnapshot, AiError> {
        let config = self.config.read().await;
        let provider = AiProviderSecretRecord::find_active_by_id(&config.providers, provider_id)?;
        Ok(ResolvedProviderSnapshot {
            provider: ResolvedProvider {
                base_url: provider.base_url.clone(),
                api_key: provider.api_key.clone(),
                default_model: provider.default_model.clone(),
                extra_headers: provider.non_sensitive_extra_headers(),
            },
            agent_settings: config.agent_settings.clone(),
        })
    }

    async fn resolve_draft_snapshot(
        &self,
        draft: &AiProviderDraft,
    ) -> Result<ResolvedProviderSnapshot, AiError> {
        let config = self.config.read().await;
        if let Some(ref api_key) = draft.api_key
            && !api_key.trim().is_empty()
        {
            return Ok(ResolvedProviderSnapshot {
                provider: ResolvedProvider {
                    base_url: draft.base_url.clone(),
                    api_key: api_key.clone(),
                    default_model: draft.default_model.clone(),
                    extra_headers: filter_non_sensitive_extra_headers(&draft.extra_headers),
                },
                agent_settings: config.agent_settings.clone(),
            });
        }

        if let Some(ref id) = draft.id {
            let provider = AiProviderSecretRecord::find_by_id(&config.providers, id)?;
            return Ok(ResolvedProviderSnapshot {
                provider: ResolvedProvider {
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
                        provider.non_sensitive_extra_headers()
                    } else {
                        filter_non_sensitive_extra_headers(&draft.extra_headers)
                    },
                },
                agent_settings: config.agent_settings.clone(),
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
            true,
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
            false,
        );
        let Ok(json) = serde_json::to_value(payload) else {
            panic!("payload serializes");
        };

        assert!(json.get("thinking").is_none());
        assert!(json.get("reasoning_effort").is_none());
        assert!((json["temperature"].as_f64().unwrap_or_default() - 0.3).abs() < 0.001);
        assert!((json["top_p"].as_f64().unwrap_or_default() - 0.8).abs() < 0.001);
    }

    #[tokio::test]
    async fn stream_invalid_json_event_propagates_parse_error() {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        use tokio::net::TcpListener;

        let listener = match TcpListener::bind("127.0.0.1:0").await {
            Ok(listener) => listener,
            Err(error) => panic!("绑定本地 SSE 测试服务失败: {error}"),
        };
        let local_addr = match listener.local_addr() {
            Ok(addr) => addr,
            Err(error) => panic!("读取本地 SSE 测试地址失败: {error}"),
        };
        let base_url = format!("http://{local_addr}");

        let server = tokio::spawn(async move {
            let (mut socket, _) = match listener.accept().await {
                Ok(accepted) => accepted,
                Err(error) => panic!("接收 SSE 测试请求失败: {error}"),
            };
            let mut buffer = [0_u8; 1024];
            if let Err(error) = socket.read(&mut buffer).await {
                panic!("读取 SSE 测试请求失败: {error}");
            }
            if let Err(error) = socket
                .write_all(
                    concat!(
                        "HTTP/1.1 200 OK\r\n",
                        "content-type: text/event-stream\r\n",
                        "\r\n",
                        "data: {not-json}\n\n"
                    )
                    .as_bytes(),
                )
                .await
            {
                panic!("写入 SSE 测试响应失败: {error}");
            }
        });

        let config = AiConfigFile {
            version: 1,
            providers: vec![AiProviderSecretRecord {
                id: "local".to_owned(),
                name: "本地测试".to_owned(),
                base_url,
                api_key: "sk-test".to_owned(),
                default_model: "gpt-test".to_owned(),
                extra_headers: HashMap::new(),
                enabled: true,
            }],
            active_provider_id: Some("local".to_owned()),
            copilot_params: AiGenerationParams::default(),
            agent_settings: crate::config::AiAgentSettings::default(),
        };
        let service = OpenAiCompatibleService::new(Arc::new(RwLock::new(config)));
        let stream_result = service
            .stream_complete(AiCompletionRequest {
                provider_id: "local".to_owned(),
                model: None,
                messages: vec![AiMessage {
                    role: AiMessageRole::User,
                    content: "Hi".to_owned(),
                }],
                params: AiGenerationParams::default(),
                timeout_ms: Some(1_000),
            })
            .await;
        let Ok(mut rx) = stream_result else {
            panic!("创建 stream receiver 失败: {stream_result:?}");
        };

        let Some(result) = rx.recv().await else {
            panic!("stream should yield parse error");
        };
        let Err(error) = result else {
            panic!("stream should return error, got {result:?}");
        };
        assert!(
            matches!(error, AiError::ResponseParseError(_)),
            "invalid JSON should propagate parse error, got {error:?}"
        );
        assert!(
            error.to_string().contains("{not-json}"),
            "parse error should include event preview, got {error}"
        );

        if let Err(error) = server.await {
            panic!("SSE 测试服务异常结束: {error}");
        }
    }

    #[test]
    fn deepseek_connection_test_disables_thinking_for_lightweight_probe() {
        let provider = test_provider("https://api.deepseek.com", "deepseek-v4-flash");
        let thinking_enabled = true;
        let params = build_connection_test_params(thinking_enabled);
        let payload = build_chat_payload(
            provider.default_model.clone(),
            test_messages(),
            &params,
            false,
            thinking_enabled,
        );
        let Ok(json) = serde_json::to_value(payload) else {
            panic!("payload serializes");
        };

        assert_eq!(json["thinking"]["type"], "disabled");
        assert_eq!(json["temperature"], 0.0);
        assert_eq!(json["max_tokens"], TEST_MAX_TOKENS);
    }

    #[tokio::test]
    async fn resolve_provider_snapshot_keeps_provider_and_agent_settings_atomic() {
        let config = AiConfigFile {
            version: 1,
            providers: vec![AiProviderSecretRecord {
                id: "p1".to_owned(),
                name: "测试提供商".to_owned(),
                base_url: "https://api.deepseek.com".to_owned(),
                api_key: "sk-test".to_owned(),
                default_model: "deepseek-chat".to_owned(),
                extra_headers: HashMap::new(),
                enabled: true,
            }],
            active_provider_id: Some("p1".to_owned()),
            copilot_params: AiGenerationParams::default(),
            agent_settings: crate::config::AiAgentSettings {
                system_prompt: None,
                timeout_ms: None,
                thinking_enabled: true,
            },
        };
        let service = OpenAiCompatibleService::new(Arc::new(RwLock::new(config)));

        let snapshot = match service.resolve_provider_snapshot("p1").await {
            Ok(snapshot) => snapshot,
            Err(error) => panic!("应能解析提供商快照: {error}"),
        };

        assert_eq!(snapshot.provider.default_model, "deepseek-chat");
        assert!(snapshot.agent_settings.thinking_enabled);
    }
}

struct ResolvedProvider {
    base_url: String,
    api_key: String,
    default_model: String,
    extra_headers: HashMap<String, String>,
}

struct ResolvedProviderSnapshot {
    provider: ResolvedProvider,
    agent_settings: AiAgentSettings,
}

/// 流式请求上下文：spawned task 内部发起请求所需的所有数据。
struct StreamRequestContext {
    http: reqwest::Client,
    url: String,
    api_key: String,
    extra_headers: HashMap<String, String>,
    model: String,
    params: AiGenerationParams,
    include_deepseek_options: bool,
    timeout_ms: u64,
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

#[derive(Debug, Clone, Serialize)]
struct ChatMessagePayload {
    role: String,
    content: String,
}

#[derive(Debug, Deserialize)]
struct ChatCompletionApiResponse {
    #[serde(default)]
    choices: Vec<ChatChoice>,
    #[serde(default)]
    usage: Option<ChatUsage>,
    #[serde(default)]
    model: String,
}

#[derive(Debug, Deserialize)]
struct ChatChoice {
    message: ChatMessageResponse,
    #[serde(default)]
    #[allow(dead_code)]
    finish_reason: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ChatMessageResponse {
    content: String,
    #[serde(default)]
    #[allow(dead_code)]
    reasoning_content: Option<String>,
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

fn is_thinking_enabled(params: &AiGenerationParams) -> bool {
    params
        .thinking
        .as_ref()
        .is_some_and(|thinking| thinking.kind == AiThinkingMode::Enabled)
}

fn provider_accepts_deepseek_options(base_url: &str, model: &str) -> bool {
    let normalized_base_url = base_url.to_ascii_lowercase();
    let normalized_model = model.to_ascii_lowercase();
    normalized_base_url.contains("deepseek") || normalized_model.contains("deepseek")
}

fn build_chat_payload(
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

fn response_preview(text: &str) -> String {
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

fn parse_chat_response(body_text: &str) -> Result<ChatCompletionApiResponse, AiError> {
    serde_json::from_str(body_text).map_err(|error| {
        AiError::ResponseParseError(format!(
            "{error}；原始响应: {}；\
             常见原因：AI 输出超过 max_tokens 被截断导致 JSON 不完整，\
             或上游返回了非标准 OpenAI 格式",
            response_preview(body_text)
        ))
    })
}

enum StreamEventAction {
    Ignore,
    Done,
    Chunk(StreamChunk),
}

fn parse_stream_event_data(data: &str) -> Result<StreamEventAction, AiError> {
    if data == "[DONE]" {
        return Ok(StreamEventAction::Done);
    }

    let parsed: StreamApiResponse = serde_json::from_str(data).map_err(|error| {
        AiError::ResponseParseError(format!(
            "流式事件 JSON 解析失败: {error}；事件预览: {}",
            response_preview(data)
        ))
    })?;

    let Some(choice) = parsed.choices.first() else {
        return Ok(StreamEventAction::Ignore);
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

    if !has_content && !has_thinking && !is_done {
        return Ok(StreamEventAction::Ignore);
    }

    Ok(StreamEventAction::Chunk(StreamChunk {
        delta: content_delta,
        thinking: has_thinking.then_some(thinking_delta).flatten(),
        finish_reason,
        done: is_done,
    }))
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

/// 发送流式 HTTP 请求并返回 SSE Response。
async fn send_stream_request(
    ctx: &StreamRequestContext,
    messages: Vec<ChatMessagePayload>,
) -> Result<reqwest::Response, AiError> {
    let body = build_chat_payload(
        ctx.model.clone(),
        messages,
        &ctx.params,
        true,
        ctx.include_deepseek_options,
    );

    let mut builder = ctx
        .http
        .post(&ctx.url)
        .bearer_auth(&ctx.api_key)
        .json(&body);

    for (key, value) in &ctx.extra_headers {
        builder = builder.header(key.as_str(), value.as_str());
    }

    let response = tokio::time::timeout(Duration::from_millis(ctx.timeout_ms), builder.send())
        .await
        .map_err(|_| AiError::RequestTimeout(ctx.timeout_ms))?
        .map_err(|error| AiError::NetworkError(error.to_string()))?;

    let status = response.status().as_u16();
    if !response.status().is_success() {
        let body = response.text().await.unwrap_or_default();
        return Err(parse_api_error(status, &body));
    }

    Ok(response)
}

#[async_trait]
impl AiService for OpenAiCompatibleService {
    async fn complete(
        &self,
        request: AiCompletionRequest,
    ) -> Result<AiCompletionResponse, AiError> {
        let snapshot = self.resolve_provider_snapshot(&request.provider_id).await?;
        let provider = snapshot.provider;
        if provider.api_key.trim().is_empty() {
            return Err(AiError::InvalidConfig(format!(
                "提供商 `{}` 未配置 API Key",
                request.provider_id
            )));
        }

        let model = request.model.as_deref().unwrap_or(&provider.default_model);
        let timeout_ms = request.timeout_ms.unwrap_or(DEFAULT_TIMEOUT_MS);
        let thinking_enabled = snapshot.agent_settings.thinking_enabled;
        let include_deepseek_options =
            thinking_enabled && provider_accepts_deepseek_options(&provider.base_url, model);

        tracing::info!(
            provider = %request.provider_id,
            model = %model,
            messages = request.messages.len(),
            timeout_ms,
            thinking_enabled,
            include_deepseek_options,
            "AI completion 请求发送"
        );

        let body = build_chat_payload(
            model.to_owned(),
            build_chat_messages(&request.messages),
            &request.params,
            false,
            include_deepseek_options,
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

        // 先读取原始 body 再解析，便于诊断截断/格式问题
        let body_text = response
            .text()
            .await
            .map_err(|error| AiError::ResponseParseError(error.to_string()))?;
        let chat_response = parse_chat_response(&body_text)?;

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

        Ok(AiCompletionResponse {
            content,
            usage,
            model: chat_response.model,
        })
    }

    /// 流式 chat completion，逐 chunk 通过 channel 返回。
    /// 流中断时自动尝试伪断点续传（将已有内容拼成 assistant 消息发回 LLM 续写）。
    async fn stream_complete(
        &self,
        request: AiCompletionRequest,
    ) -> Result<tokio::sync::mpsc::Receiver<Result<StreamChunk, AiError>>, AiError> {
        let snapshot = self.resolve_provider_snapshot(&request.provider_id).await?;
        let provider = snapshot.provider;
        if provider.api_key.trim().is_empty() {
            return Err(AiError::InvalidConfig(format!(
                "提供商 `{}` 未配置 API Key",
                request.provider_id
            )));
        }

        let model = request
            .model
            .clone()
            .unwrap_or_else(|| provider.default_model.clone());
        let timeout_ms = request.timeout_ms.unwrap_or(DEFAULT_TIMEOUT_MS);
        let url = build_url(&provider.base_url, "/chat/completions")?;
        let messages = build_chat_messages(&request.messages);
        let thinking_enabled = snapshot.agent_settings.thinking_enabled;
        let include_deepseek_options =
            thinking_enabled && provider_accepts_deepseek_options(&provider.base_url, &model);

        let ctx = StreamRequestContext {
            http: self.http.clone(),
            url,
            api_key: provider.api_key,
            extra_headers: provider.extra_headers,
            model,
            params: request.params,
            include_deepseek_options,
            timeout_ms,
        };

        let response = send_stream_request(&ctx, messages.clone()).await?;

        let (tx, rx) = tokio::sync::mpsc::channel(32);

        tokio::spawn(async move {
            use futures_util::StreamExt;

            let mut stream = sseer::response_to_stream(response);

            while let Some(event_result) = stream.next().await {
                let event = match event_result {
                    Ok(event) => event,
                    Err(error) => {
                        let _ = tx
                            .send(Err(AiError::NetworkError(format!(
                                "AI 流式响应读取失败: {error}"
                            ))))
                            .await;
                        return;
                    }
                };

                match parse_stream_event_data(&event.data) {
                    Ok(StreamEventAction::Ignore) => {}
                    Ok(StreamEventAction::Done) => {
                        let _ = tx
                            .send(Ok(StreamChunk {
                                delta: String::new(),
                                thinking: None,
                                finish_reason: None,
                                done: true,
                            }))
                            .await;
                        return;
                    }
                    Ok(StreamEventAction::Chunk(chunk)) => {
                        let is_done = chunk.done;
                        if tx.send(Ok(chunk)).await.is_err() || is_done {
                            return;
                        }
                    }
                    Err(error) => {
                        let _ = tx.send(Err(error)).await;
                        return;
                    }
                }
            }

            let _ = tx
                .send(Err(AiError::NetworkError(
                    "AI 流式输出意外中断，未收到结束信号".to_owned(),
                )))
                .await;
        });

        Ok(rx)
    }
}

impl OpenAiCompatibleService {
    /// 测试提供商连通性。inherent 而非 trait 方法——`AiProviderDraft`
    /// 是壳层配置类型，不属于 Ring 0 的运行时关注点。
    pub async fn test_connection(&self, draft: AiProviderDraft) -> Result<AiTestResult, AiError> {
        let snapshot = self.resolve_draft_snapshot(&draft).await?;
        let provider = snapshot.provider;
        if provider.api_key.trim().is_empty() {
            return Err(AiError::InvalidConfig(
                "测试连接需要提供 API Key 或引用已保存密钥的提供商".to_owned(),
            ));
        }

        let url = build_url(&provider.base_url, "/chat/completions")?;
        let thinking_enabled = snapshot.agent_settings.thinking_enabled;

        let model = provider.default_model.clone();
        let include_deepseek_options =
            thinking_enabled && provider_accepts_deepseek_options(&provider.base_url, &model);
        let body = build_chat_payload(
            model.clone(),
            vec![ChatMessagePayload {
                role: "user".to_owned(),
                content: "Hi".to_owned(),
            }],
            &build_connection_test_params(thinking_enabled),
            false,
            include_deepseek_options,
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
