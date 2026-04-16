//! `OpenAI` 兼容 HTTP 客户端实现。

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;

use crate::config::{AiConfigFile, AiProviderDraft, AiProviderSecretRecord};
use crate::error::AiError;
use crate::service::AiService;
use crate::types::{
    AiCompletionRequest, AiCompletionResponse, AiMessage, AiMessageRole, AiTestResult,
    AiTokenUsage,
};

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
            .timeout(Duration::from_secs(60))
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
        if let Some(ref api_key) = draft.api_key {
            if !api_key.trim().is_empty() {
                return Ok(ResolvedProvider {
                    base_url: draft.base_url.clone(),
                    api_key: api_key.clone(),
                    default_model: draft.default_model.clone(),
                    extra_headers: draft.extra_headers.clone(),
                });
            }
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
    temperature: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    max_tokens: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    top_p: Option<f32>,
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

        let body = ChatCompletionPayload {
            model: model.to_owned(),
            messages: build_chat_messages(&request.messages),
            temperature: request.params.temperature,
            max_tokens: request.params.max_tokens,
            top_p: request.params.top_p,
        };

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

    async fn test_connection(&self, draft: AiProviderDraft) -> Result<AiTestResult, AiError> {
        let provider = self.resolve_draft(&draft).await?;
        if provider.api_key.trim().is_empty() {
            return Err(AiError::InvalidConfig(
                "测试连接需要提供 API Key 或引用已保存密钥的提供商".to_owned(),
            ));
        }

        let url = build_url(&provider.base_url, "/chat/completions")?;

        let body = ChatCompletionPayload {
            model: provider.default_model.clone(),
            messages: vec![ChatMessagePayload {
                role: "user".to_owned(),
                content: "Hi".to_owned(),
            }],
            temperature: Some(0.0),
            max_tokens: Some(TEST_MAX_TOKENS),
            top_p: None,
        };

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
