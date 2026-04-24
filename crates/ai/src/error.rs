//! AI 模块独立错误类型。
//!
//! 不依赖 [`EngineError`]，在 Tauri IPC 边界转 `String`，
//! 在 engine 层使用时做错误转换。

use thiserror::Error;

/// 覆盖 AI 相关操作的所有失败模式。
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
