//! `OpenAI` 兼容 HTTP 客户端：`AiService`（Ring 0）的 reqwest + SSE 实现。

mod protocol;
mod provider_policy;
mod response;
mod stream;
#[cfg(test)]
mod tests;
mod types;

use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use nazh_core::ai::{AiCompletionRequest, AiCompletionResponse, AiError, AiService, StreamChunk};
use tokio::sync::RwLock;

use crate::config::{
    AiConfigFile, AiProviderDraft, AiProviderSecretRecord, filter_non_sensitive_extra_headers,
};
use crate::types::AiTestResult;

use self::protocol::{ChatMessagePayload, build_chat_messages, build_chat_payload, build_url};
use self::provider_policy::{build_connection_test_params, provider_accepts_deepseek_options};
use self::response::{chat_response_to_completion, parse_api_error, parse_chat_response};
use self::stream::open_stream;
use self::types::{ResolvedProvider, ResolvedProviderSnapshot, StreamRequestContext};

const DEFAULT_TIMEOUT_MS: u64 = 30_000;

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

        Ok(chat_response_to_completion(chat_response))
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

        open_stream(ctx, messages).await
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
