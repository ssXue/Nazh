//! AI 服务的协议无关接口。
//!
//! Ring 0 只定义 trait 与请求/响应类型，不引入任何 HTTP / SDK 依赖。
//! 具体实现由 `ai` crate 的 `OpenAiCompatibleService` 等承担；未来 Anthropic
//! 原生、本地 Llama / Qwen 等 provider 通过新增 impl crate 接入即可。
//!
//! 配置型类型（`AiConfigFile` / `AiProviderDraft` 等）属于壳层关注点，
//! 仍留在 `ai` crate；本模块只承载运行时上下文中流转的最小类型集合。

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use tokio::sync::mpsc;
#[cfg(feature = "ts-export")]
use ts_rs::TS;

/// AI 服务统一接口。脚本节点 / Code 节点通过 `Arc<dyn AiService>` 注入使用。
///
/// 设计上仅暴露**运行时**会用到的两个能力：非流式 `complete` 与流式
/// `stream_complete`。配置态测试连接（`test_connection`）属于壳层概念，
/// 由具体实现以 inherent 方法提供，不进入此 trait。
#[async_trait]
pub trait AiService: Send + Sync {
    /// 一次性聊天补全。
    async fn complete(&self, request: AiCompletionRequest)
    -> Result<AiCompletionResponse, AiError>;

    /// 流式聊天补全：逐 chunk 通过 channel 返回。
    async fn stream_complete(
        &self,
        request: AiCompletionRequest,
    ) -> Result<mpsc::Receiver<Result<StreamChunk, AiError>>, AiError>;
}

/// AI 模块独立错误类型。
///
/// 所有变体都使用 `String` / 标量 payload，避免与具体协议库（reqwest 等）
/// 形成 Ring 0 反向依赖。Tauri IPC 边界转 `String`，引擎层使用时显式
/// 转换为 `EngineError`。
#[derive(Debug, Error)]
pub enum AiError {
    #[error("AI 提供商 `{0}` 不存在")]
    ProviderNotFound(String),

    #[error("AI 提供商 `{0}` 已禁用")]
    ProviderDisabled(String),

    #[error("AI 请求超时（{0} ms）")]
    RequestTimeout(u64),

    #[error("AI API 认证失败: {0}")]
    AuthenticationFailed(String),

    #[error("AI API 请求失败（状态 {status}）: {message}")]
    ApiError { status: u16, message: String },

    #[error("AI 配置无效: {0}")]
    InvalidConfig(String),

    #[error("AI 响应解析失败: {0}")]
    ResponseParseError(String),

    #[error("AI 网络错误: {0}")]
    NetworkError(String),
}

/// Chat completion 请求。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-export", derive(TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct AiCompletionRequest {
    /// 使用哪个提供商。
    pub provider_id: String,
    /// 覆盖默认模型。
    #[serde(default)]
    #[cfg_attr(feature = "ts-export", ts(optional))]
    pub model: Option<String>,
    /// 消息列表。
    pub messages: Vec<AiMessage>,
    /// 生成参数。
    #[serde(default)]
    pub params: AiGenerationParams,
    /// 超时毫秒（None 使用默认 30s）。
    #[serde(default)]
    #[cfg_attr(feature = "ts-export", ts(optional))]
    pub timeout_ms: Option<u64>,
}

/// 聊天消息。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-export", derive(TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct AiMessage {
    pub role: AiMessageRole,
    pub content: String,
}

/// 消息角色。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-export", derive(TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub enum AiMessageRole {
    System,
    User,
    Assistant,
}

/// Chat completion 响应。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-export", derive(TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct AiCompletionResponse {
    /// 模型返回的文本内容。
    pub content: String,
    /// 本次消耗的 token 数。
    #[serde(default)]
    #[cfg_attr(feature = "ts-export", ts(optional))]
    pub usage: Option<AiTokenUsage>,
    /// 使用的模型名。
    pub model: String,
}

/// Token 用量统计。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-export", derive(TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct AiTokenUsage {
    pub prompt_tokens: u32,
    pub completion_tokens: u32,
    pub total_tokens: u32,
}

/// 流式传输的每个 chunk。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StreamChunk {
    /// 本次 chunk 的文本片段。
    pub delta: String,
    /// 本次 chunk 的思考过程片段（DeepSeek 等模型的 `reasoning_content`）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub thinking: Option<String>,
    /// 提供商返回的结束原因，例如 stop / length。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub finish_reason: Option<String>,
    /// 是否为最后一个 chunk。
    pub done: bool,
}

/// `DeepSeek/OpenAI` 兼容的思考模式开关。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-export", derive(TS), ts(export))]
#[serde(rename_all = "lowercase")]
pub enum AiThinkingMode {
    Enabled,
    Disabled,
}

/// `DeepSeek/OpenAI` 兼容的思考模式配置。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-export", derive(TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct AiThinkingConfig {
    #[serde(rename = "type")]
    pub kind: AiThinkingMode,
}

/// `DeepSeek` 推理强度。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-export", derive(TS), ts(export))]
#[serde(rename_all = "lowercase")]
pub enum AiReasoningEffort {
    High,
    Max,
}

/// Copilot / 节点共享的生成参数。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-export", derive(TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct AiGenerationParams {
    #[serde(default = "default_temperature")]
    #[cfg_attr(feature = "ts-export", ts(optional))]
    pub temperature: Option<f32>,
    #[serde(default = "default_max_tokens")]
    #[cfg_attr(feature = "ts-export", ts(optional))]
    pub max_tokens: Option<u32>,
    #[serde(default = "default_top_p")]
    #[cfg_attr(feature = "ts-export", ts(optional))]
    pub top_p: Option<f32>,
    #[serde(default)]
    #[cfg_attr(feature = "ts-export", ts(optional))]
    pub thinking: Option<AiThinkingConfig>,
    #[serde(default)]
    #[cfg_attr(feature = "ts-export", ts(optional))]
    pub reasoning_effort: Option<AiReasoningEffort>,
}

#[allow(clippy::unnecessary_wraps)]
fn default_temperature() -> Option<f32> {
    Some(0.7)
}
#[allow(clippy::unnecessary_wraps)]
fn default_max_tokens() -> Option<u32> {
    Some(2048)
}
#[allow(clippy::unnecessary_wraps)]
fn default_top_p() -> Option<f32> {
    Some(1.0)
}

impl Default for AiGenerationParams {
    fn default() -> Self {
        Self {
            temperature: default_temperature(),
            max_tokens: default_max_tokens(),
            top_p: default_top_p(),
            thinking: None,
            reasoning_effort: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_generation_params() {
        let params = AiGenerationParams::default();
        assert_eq!(params.temperature, Some(0.7));
        assert_eq!(params.max_tokens, Some(2048));
        assert_eq!(params.top_p, Some(1.0));
        assert_eq!(params.thinking, None);
        assert_eq!(params.reasoning_effort, None);
    }

    #[test]
    fn ai_error_messages_use_chinese_format() {
        let err = AiError::ProviderNotFound("p1".to_owned());
        assert_eq!(err.to_string(), "AI 提供商 `p1` 不存在");
    }
}
