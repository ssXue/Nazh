//! AI 请求/响应类型定义。
//!
//! 定义了 chat completion 请求/响应、消息角色、token 用量等类型，
//! Copilot 和运行时节点共用。

use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// Chat completion 请求。
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct AiCompletionRequest {
    /// 使用哪个提供商。
    pub provider_id: String,
    /// 覆盖默认模型。
    #[serde(default)]
    #[ts(optional)]
    pub model: Option<String>,
    /// 消息列表。
    pub messages: Vec<AiMessage>,
    /// 生成参数。
    #[serde(default)]
    pub params: crate::config::AiGenerationParams,
    /// 超时毫秒（None 使用默认 30s）。
    #[serde(default)]
    #[ts(optional)]
    pub timeout_ms: Option<u64>,
}

/// 聊天消息。
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct AiMessage {
    pub role: AiMessageRole,
    pub content: String,
}

/// 消息角色。
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub enum AiMessageRole {
    System,
    User,
    Assistant,
}

/// Chat completion 响应。
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct AiCompletionResponse {
    /// 模型返回的文本内容。
    pub content: String,
    /// 本次消耗的 token 数。
    #[serde(default)]
    #[ts(optional)]
    pub usage: Option<AiTokenUsage>,
    /// 使用的模型名。
    pub model: String,
}

/// Token 用量统计。
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct AiTokenUsage {
    pub prompt_tokens: u32,
    pub completion_tokens: u32,
    pub total_tokens: u32,
}

/// 连通性测试结果。
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct AiTestResult {
    pub success: bool,
    pub message: String,
    #[serde(default)]
    #[ts(optional)]
    pub latency_ms: Option<u64>,
}
