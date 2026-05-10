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
/// 设计上仅暴露**运行时**会用到的能力：非流式 `complete`、流式
/// `stream_complete`、文本 `embed`。配置态测试连接（`test_connection`）
/// 属于壳层概念，由具体实现以 inherent 方法提供，不进入此 trait。
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

    /// 生成文本 embedding 向量。
    async fn embed(
        &self,
        request: AiEmbeddingRequest,
    ) -> Result<AiEmbeddingResponse, AiError>;
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

    #[error("AI 工具调用失败（工具 {tool_name}）: {message}")]
    ToolCallError { tool_name: String, message: String },

    #[error("AI 工具调用超过最大循环次数（{0}）")]
    ToolCallLoopLimit(u32),

    #[error("AI embedding 不受支持: {0}")]
    EmbeddingNotSupported(String),
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
    /// 可供模型调用的工具列表（空数组时不发送 `tools` 字段）。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tools: Vec<AiToolDefinition>,
}

/// 聊天消息。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-export", derive(TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct AiMessage {
    pub role: AiMessageRole,
    pub content: String,
    /// 助手消息携带的工具调用（模型决定调用工具时非空）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts-export", ts(optional))]
    pub tool_calls: Option<Vec<AiToolCall>>,
    /// 工具角色消息对应的工具调用 ID。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts-export", ts(optional))]
    pub tool_call_id: Option<String>,
}

impl AiMessage {
    /// 构造简单消息（无工具调用字段）。
    pub fn simple(role: AiMessageRole, content: String) -> Self {
        Self {
            role,
            content,
            tool_calls: None,
            tool_call_id: None,
        }
    }
}

/// 消息角色。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-export", derive(TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub enum AiMessageRole {
    System,
    User,
    Assistant,
    /// 工具执行结果消息（不入 copilot DB，仅在 AI 请求上下文中流转）。
    Tool,
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
    /// 模型发起的工具调用（`finish_reason == "tool_calls"` 时非空）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts-export", ts(optional))]
    pub tool_calls: Option<Vec<AiToolCall>>,
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
    /// 提供商返回的结束原因，例如 stop / length / `tool_calls`。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub finish_reason: Option<String>,
    /// 是否为最后一个 chunk（`finish_reason == "tool_calls"` 时为 false，工具循环未结束）。
    pub done: bool,
    /// 模型发起的工具调用（流式增量累积，`finish_reason == "tool_calls"` 时完整）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_calls: Option<Vec<AiToolCall>>,
}

/// 发送给模型的工具定义（Chat Completions `tools` 数组元素）。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-export", derive(TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct AiToolDefinition {
    pub name: String,
    pub description: String,
    /// JSON Schema 描述工具参数结构。
    pub parameters: serde_json::Value,
}

/// 模型返回的单次工具调用。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-export", derive(TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct AiToolCall {
    /// 本次调用在响应内的唯一 ID。
    pub id: String,
    /// 工具名称。
    pub name: String,
    /// JSON 格式的调用参数。
    pub arguments: String,
}

/// 工具执行结果，回送到对话上下文。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-export", derive(TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct AiToolResult {
    /// 对应 `AiToolCall::id`。
    pub tool_call_id: String,
    /// 工具输出内容（JSON 字符串或纯文本）。
    pub content: String,
    /// 工具调用本身是否失败。
    pub is_error: bool,
}

/// Embedding 向量生成请求。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-export", derive(TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct AiEmbeddingRequest {
    /// 使用哪个提供商。
    pub provider_id: String,
    /// 覆盖默认 embedding 模型。
    #[serde(default)]
    #[cfg_attr(feature = "ts-export", ts(optional))]
    pub model: Option<String>,
    /// 待生成向量的文本列表。
    pub input: Vec<String>,
    /// 超时毫秒。
    #[serde(default)]
    #[cfg_attr(feature = "ts-export", ts(optional))]
    pub timeout_ms: Option<u64>,
}

/// Embedding 向量生成响应。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-export", derive(TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct AiEmbeddingResponse {
    /// 每条输入对应的向量。
    pub embeddings: Vec<Vec<f32>>,
    /// 使用的模型名。
    pub model: String,
    /// Token 用量。
    #[serde(default)]
    #[cfg_attr(feature = "ts-export", ts(optional))]
    pub usage: Option<AiTokenUsage>,
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
