//! `OpenAI` 兼容 HTTP 客户端：`AiService`（Ring 0）的 async-openai 实现。

mod provider_policy;
mod response;
#[cfg(test)]
mod tests;
mod types;

use std::sync::Arc;
use std::time::Duration;

use async_openai::config::OpenAIConfig;
use async_openai::error::OpenAIError;
use async_openai::traits::RequestOptionsBuilder;
use async_trait::async_trait;
use futures_util::StreamExt;
use nazh_core::ai::{
    AiCompletionRequest, AiCompletionResponse, AiEmbeddingRequest, AiEmbeddingResponse, AiError,
    AiGenerationParams, AiMessage, AiMessageRole, AiReasoningEffort, AiService, AiThinkingMode,
    AiToolCall, AiToolDefinition, StreamChunk,
};
use serde_json::json;
use tokio::sync::RwLock;

use crate::config::{
    AiConfigFile, AiProviderDraft, AiProviderSecretRecord, filter_non_sensitive_extra_headers,
};
use crate::types::AiTestResult;

use self::provider_policy::{build_connection_test_params, provider_accepts_deepseek_options};
use self::response::{map_openai_error, value_to_completion};
use self::types::{ResolvedProvider, ResolvedProviderSnapshot};

const DEFAULT_TIMEOUT_MS: u64 = 30_000;

/// `OpenAI` 兼容 API 客户端。
pub struct OpenAiCompatibleService {
    config: Arc<RwLock<AiConfigFile>>,
}

impl OpenAiCompatibleService {
    /// 创建新的客户端实例。
    pub fn new(config: Arc<RwLock<AiConfigFile>>) -> Self {
        Self { config }
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

fn build_openai_client(provider: &ResolvedProvider) -> async_openai::Client<OpenAIConfig> {
    let config = OpenAIConfig::new()
        .with_api_base(&provider.base_url)
        .with_api_key(&provider.api_key);
    async_openai::Client::with_config(config)
}

/// 将 `AiMessage` 列表转换为 JSON 消息数组。
fn convert_messages(messages: &[AiMessage]) -> Vec<serde_json::Value> {
    messages
        .iter()
        .map(|msg| {
            let role = match msg.role {
                AiMessageRole::System => "system",
                AiMessageRole::User => "user",
                AiMessageRole::Assistant => "assistant",
                AiMessageRole::Tool => "tool",
            };
            let mut value = json!({ "role": role, "content": msg.content });
            if let Some(tool_calls) = &msg.tool_calls {
                value["tool_calls"] = json!(tool_calls.iter().map(|tc| {
                    json!({
                        "id": tc.id,
                        "type": "function",
                        "function": {
                            "name": tc.name,
                            "arguments": tc.arguments,
                        }
                    })
                }).collect::<Vec<_>>());
            }
            if let Some(tool_call_id) = &msg.tool_call_id {
                value["tool_call_id"] = json!(tool_call_id);
            }
            value
        })
        .collect()
}

/// 构建请求 JSON（统一 BYOT 路径）。
fn build_request_json(
    model: &str,
    messages: &[serde_json::Value],
    params: &AiGenerationParams,
    stream: bool,
    is_deepseek: bool,
    thinking_enabled: bool,
    tools: &[AiToolDefinition],
) -> serde_json::Value {
    let omit_sampling_params = is_deepseek && thinking_enabled;

    let mut body = json!({
        "model": model,
        "messages": messages,
        "stream": stream,
    });

    if let Some(max_tokens) = params.max_tokens {
        body["max_tokens"] = json!(max_tokens);
    }

    if !omit_sampling_params {
        if let Some(temperature) = params.temperature {
            body["temperature"] = json!(temperature);
        }
        if let Some(top_p) = params.top_p {
            body["top_p"] = json!(top_p);
        }
    }

    if is_deepseek {
        let thinking_mode = params
            .thinking
            .as_ref()
            .map(|t| t.kind.clone())
            .unwrap_or(if thinking_enabled {
                AiThinkingMode::Enabled
            } else {
                AiThinkingMode::Disabled
            });
        body["thinking"] = json!({ "type": match thinking_mode {
            AiThinkingMode::Enabled => "enabled",
            AiThinkingMode::Disabled => "disabled",
        }});
        if thinking_enabled
            && let Some(ref effort) = params.reasoning_effort
        {
            body["reasoning_effort"] = json!(match effort {
                AiReasoningEffort::High => "high",
                AiReasoningEffort::Max => "max",
            });
        }
    }

    if !tools.is_empty() {
        body["tools"] = json!(tools.iter().map(|t| {
            json!({
                "type": "function",
                "function": {
                    "name": t.name,
                    "description": t.description,
                    "parameters": t.parameters,
                }
            })
        }).collect::<Vec<_>>());
    }

    body
}

/// 将 `extra_headers` 构建为 reqwest HeaderMap，供 `RequestOptionsBuilder::headers()` 使用。
fn build_extra_header_map(
    headers: &std::collections::HashMap<String, String>,
) -> reqwest::header::HeaderMap {
    let mut map = reqwest::header::HeaderMap::new();
    for (key, value) in headers {
        let Ok(name) = reqwest::header::HeaderName::from_bytes(key.as_bytes()) else {
            tracing::warn!(key = %key, "跳过无效 header name");
            continue;
        };
        let Ok(val) = reqwest::header::HeaderValue::from_str(value) else {
            tracing::warn!(key = %key, "跳过无效 header value");
            continue;
        };
        map.insert(name, val);
    }
    map
}

async fn execute_with_timeout<F, T>(
    timeout_ms: u64,
    future: F,
) -> Result<T, AiError>
where
    F: std::future::Future<Output = Result<T, OpenAIError>>,
{
    tokio::time::timeout(Duration::from_millis(timeout_ms), future)
        .await
        .map_err(|_| AiError::RequestTimeout(timeout_ms))?
        .map_err(map_openai_error)
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
        let is_deepseek = provider_accepts_deepseek_options(&provider.base_url, model);

        tracing::info!(
            provider = %request.provider_id,
            model = %model,
            messages = request.messages.len(),
            timeout_ms,
            thinking_enabled,
            is_deepseek,
            "AI completion 请求发送"
        );

        let client = build_openai_client(&provider);
        let messages = convert_messages(&request.messages);
        let body = build_request_json(
            model,
            &messages,
            &request.params,
            false,
            is_deepseek,
            thinking_enabled,
            &request.tools,
        );

        let extra = build_extra_header_map(&provider.extra_headers);
        let value: serde_json::Value = execute_with_timeout(
            timeout_ms,
            client.chat().headers(extra).create_byot(body),
        )
        .await?;

        Ok(value_to_completion(&value))
    }

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
        let thinking_enabled = snapshot.agent_settings.thinking_enabled;
        let is_deepseek = provider_accepts_deepseek_options(&provider.base_url, &model);

        let client = build_openai_client(&provider);
        let messages = convert_messages(&request.messages);
        let body = build_request_json(
            &model,
            &messages,
            &request.params,
            true,
            is_deepseek,
            thinking_enabled,
            &request.tools,
        );

        let extra = build_extra_header_map(&provider.extra_headers);
        let stream: async_openai::types::stream::StreamResponse<serde_json::Value> =
            execute_with_timeout(
                timeout_ms,
                client.chat().headers(extra).create_stream_byot(body),
            )
            .await?;

        let (tx, rx) = tokio::sync::mpsc::channel(32);

        tokio::spawn(async move {
            let mut stream = std::pin::pin!(stream);
            // 流式 tool_calls 增量累积：(id, name, accumulated_arguments)
            let mut tool_calls_map: std::collections::HashMap<usize, (String, String, String)> =
                std::collections::HashMap::new();

            while let Some(result) = stream.next().await {
                match result {
                    Ok(value) => {
                        let Some(choices) = value.get("choices").and_then(|c| c.as_array()) else {
                            continue;
                        };
                        let Some(choice) = choices.first() else {
                            continue;
                        };

                        let delta = choice.get("delta");
                        let content = delta
                            .and_then(|d| d.get("content"))
                            .and_then(|c| c.as_str())
                            .unwrap_or_default();
                        let thinking = delta
                            .and_then(|d| d.get("reasoning_content"))
                            .and_then(|t| t.as_str())
                            .filter(|s| !s.is_empty())
                            .map(String::from);
                        let finish_reason = choice
                            .get("finish_reason")
                            .and_then(|f| f.as_str())
                            .filter(|s| !s.is_empty())
                            .map(String::from);

                        let has_content = !content.is_empty();
                        let has_thinking = thinking.is_some();
                        // tool_calls 不算流结束——后端需要继续执行工具并发起下一轮
                        let is_done = finish_reason.is_some()
                            && finish_reason.as_deref() != Some("tool_calls");

                        // 解析流式 tool_calls 增量
                        let tc_delta = delta.and_then(|d| d.get("tool_calls")).and_then(|t| t.as_array());
                        if let Some(tc_arr) = tc_delta {
                            for tc in tc_arr {
                                let index = tc.get("index").and_then(|i| i.as_u64()).unwrap_or(0) as usize;
                                let entry = tool_calls_map.entry(index).or_insert_with(|| {
                                    (String::new(), String::new(), String::new())
                                });
                                // 首个 delta 通常带 id 和 function.name
                                if let Some(id) = tc.get("id").and_then(|v| v.as_str()) {
                                    entry.0 = id.to_owned();
                                }
                                if let Some(name) = tc.get("function").and_then(|f| f.get("name")).and_then(|n| n.as_str()) {
                                    entry.1 = name.to_owned();
                                }
                                if let Some(args) = tc.get("function").and_then(|f| f.get("arguments")).and_then(|a| a.as_str()) {
                                    entry.2.push_str(args);
                                }
                            }
                        }
                        let has_tool_calls = tc_delta.is_some();

                        if !has_content && !has_thinking && !is_done && !has_tool_calls {
                            continue;
                        }

                        // finish_reason == "tool_calls" 时构建完整的工具调用列表
                        let final_tool_calls = if finish_reason.as_deref() == Some("tool_calls") {
                            let mut calls: Vec<AiToolCall> = tool_calls_map
                                .iter()
                                .map(|(idx, (id, name, args))| {
                                    let call_id = if id.is_empty() {
                                        format!("tc_{idx}")
                                    } else {
                                        id.clone()
                                    };
                                    AiToolCall {
                                        id: call_id,
                                        name: name.clone(),
                                        arguments: args.clone(),
                                    }
                                })
                                .collect::<Vec<_>>();
                            calls.sort_by_key(|c| {
                                // 保持原始 index 顺序
                                tool_calls_map
                                    .iter()
                                    .find_map(|(idx, (id, _, _))| {
                                        if id == &c.id { Some(*idx) } else { None }
                                    })
                                    .unwrap_or(0)
                            });
                            tool_calls_map.clear();
                            Some(calls)
                        } else {
                            None
                        };

                        let chunk = StreamChunk {
                            delta: content.to_owned(),
                            thinking,
                            finish_reason: finish_reason.clone(),
                            done: is_done,
                            tool_calls: final_tool_calls,
                        };

                        if tx.send(Ok(chunk)).await.is_err() || is_done {
                            return;
                        }
                    }
                    Err(error) => {
                        // [DONE] 是 OpenAI SSE 标准流终止标记，async-openai BYOT
                        // 模式尝试将其解析为 JSON 会产生 JSONDeserialize 错误，
                        // 这属于正常流结束，不应作为错误传播。
                        let is_done_marker = matches!(
                            &error,
                            OpenAIError::JSONDeserialize(_, content)
                            if content.trim().eq_ignore_ascii_case("[DONE]")
                        );
                        if is_done_marker {
                            tracing::debug!("SSE 流收到 [DONE] 标记，正常结束");
                            return;
                        }
                        let _ = tx.send(Err(map_openai_error(error))).await;
                        return;
                    }
                }
            }

            // 流意外中断，未收到 [DONE]
            let _ = tx
                .send(Err(AiError::NetworkError(
                    "AI 流式输出意外中断，未收到结束信号".to_owned(),
                )))
                .await;
        });

        Ok(rx)
    }

    async fn embed(
        &self,
        request: AiEmbeddingRequest,
    ) -> Result<AiEmbeddingResponse, AiError> {
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
            .as_deref()
            .unwrap_or(&provider.default_model);
        let timeout_ms = request.timeout_ms.unwrap_or(DEFAULT_TIMEOUT_MS);

        tracing::info!(
            provider = %request.provider_id,
            model = %model,
            input_count = request.input.len(),
            timeout_ms,
            "AI embedding 请求发送"
        );

        let client = build_openai_client(&provider);
        let body = json!({
            "model": model,
            "input": request.input,
        });

        let extra = build_extra_header_map(&provider.extra_headers);
        let value: serde_json::Value = execute_with_timeout(
            timeout_ms,
            client.embeddings().headers(extra).create_byot(body),
        )
        .await?;

        // 解析 embedding 响应
        let embeddings = value
            .get("data")
            .and_then(|d| d.as_array())
            .map(|arr| {
                let mut indexed: Vec<(usize, Vec<f32>)> = arr
                    .iter()
                    .filter_map(|item| {
                        #[allow(clippy::cast_possible_truncation)]
                        let index = item.get("index")?.as_u64()? as usize;
                        let emb = item.get("embedding")?.as_array()?;
                        #[allow(clippy::cast_possible_truncation)]
                        let vec: Vec<f32> = emb.iter().filter_map(|v| v.as_f64().map(|f| f as f32)).collect();
                        Some((index, vec))
                    })
                    .collect();
                indexed.sort_by_key(|(i, _)| *i);
                indexed.into_iter().map(|(_, v)| v).collect()
            })
            .unwrap_or_default();

        Ok(AiEmbeddingResponse {
            embeddings,
            model: model.to_owned(),
            usage: None,
        })
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

        let thinking_enabled = snapshot.agent_settings.thinking_enabled;
        let model = provider.default_model.clone();
        let is_deepseek = provider_accepts_deepseek_options(&provider.base_url, &model);

        let client = build_openai_client(&provider);
        let body = build_request_json(
            &model,
            &[json!({ "role": "user", "content": "Hi" })],
            &build_connection_test_params(thinking_enabled),
            false,
            is_deepseek,
            thinking_enabled,
            &[],
        );

        let extra = build_extra_header_map(&provider.extra_headers);
        let started_at = std::time::Instant::now();

        let result: Result<serde_json::Value, AiError> = execute_with_timeout(
            15_000,
            client.chat().headers(extra).create_byot(body),
        )
        .await;

        match result {
            Ok(_) => {
                #[allow(clippy::cast_possible_truncation)]
                let latency_ms = started_at.elapsed().as_millis() as u64;
                Ok(AiTestResult {
                    success: true,
                    message: format!(
                        "连接成功（模型 {}，延迟 {} ms）",
                        provider.default_model, latency_ms
                    ),
                    latency_ms: Some(latency_ms),
                })
            }
            Err(error) => {
                #[allow(clippy::cast_possible_truncation)]
                let latency_ms = started_at.elapsed().as_millis() as u64;
                Ok(AiTestResult {
                    success: false,
                    message: error.to_string(),
                    latency_ms: Some(latency_ms),
                })
            }
        }
    }
}
