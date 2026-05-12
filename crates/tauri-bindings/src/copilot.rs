//! Copilot 对话 IPC 响应类型。

use serde::{Deserialize, Serialize};

#[cfg(feature = "ts-export")]
use ts_rs::TS;

/// Copilot 对话记录。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-export", derive(TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct CopilotConversationResponse {
    pub id: String,
    pub title: String,
    pub created_at: String,
    pub updated_at: String,
}

/// Copilot 消息记录。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-export", derive(TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct CopilotMessageResponse {
    pub id: String,
    pub conversation_id: String,
    pub role: String,
    pub content: String,
    /// 助手消息携带的推理过程（DeepSeek 等模型的 `reasoning_content`）。
    /// 多轮工具调用时必须回传给 API，否则会触发 API 错误。
    pub thinking: Option<String>,
    pub created_at: String,
}
