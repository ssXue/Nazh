//! AI 相关的共享类型定义。
//!
//! Ring 0 仅保留工具协议类型（`AiToolCall` / `AiToolDefinition` / `AiToolResult`）、
//! 生成参数（`AiGenerationParams`）和错误枚举（`AiError`）。
//!
//! AI HTTP 调用已全部前移到前端（RFC-0005），Rust 不再持有 HTTP 客户端。

use serde::{Deserialize, Serialize};
use thiserror::Error;
#[cfg(feature = "ts-export")]
use ts_rs::TS;

/// AI 模块独立错误类型。
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
