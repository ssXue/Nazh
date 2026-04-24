//! # Nazh AI Core
//!
//! AI 公共层：封装 `OpenAI` 兼容 API 的 HTTP 客户端调用与配置模型。
//!
//! 本 crate 不依赖引擎业务逻辑，可被 Tauri 壳层（配置态 Copilot）
//! 和 nazh-engine（运行时 AI 节点）分别引用。

pub mod client;
pub mod config;
pub mod error;
pub mod service;
pub mod types;

pub use client::{OpenAiCompatibleService, StreamChunk};
pub use config::{
    AiAgentSettings, AiConfigFile, AiConfigUpdate, AiConfigView, AiGenerationParams,
    AiProviderDraft, AiProviderSecretRecord, AiProviderUpsert, AiProviderView, AiReasoningEffort,
    AiSecretInput, AiThinkingConfig, AiThinkingMode,
};
pub use error::AiError;
pub use service::AiService;
pub use types::{
    AiCompletionRequest, AiCompletionResponse, AiMessage, AiMessageRole, AiTestResult, AiTokenUsage,
};

/// ts-rs 类型导出入口。仅在 `ts-export` feature 启用时编译。
#[cfg(feature = "ts-export")]
pub mod export_bindings {
    use super::{
        AiCompletionRequest, AiCompletionResponse, AiMessage, AiMessageRole, AiTestResult,
        AiTokenUsage,
    };
    use crate::config::{
        AiAgentSettings, AiConfigUpdate, AiConfigView, AiGenerationParams, AiProviderDraft,
        AiProviderUpsert, AiProviderView, AiReasoningEffort, AiSecretInput, AiThinkingConfig,
        AiThinkingMode,
    };
    use ts_rs::{ExportError, TS};

    pub fn export_all() -> Result<(), ExportError> {
        AiAgentSettings::export()?;
        AiConfigView::export()?;
        AiConfigUpdate::export()?;
        AiGenerationParams::export()?;
        AiThinkingConfig::export()?;
        AiThinkingMode::export()?;
        AiReasoningEffort::export()?;
        AiProviderView::export()?;
        AiProviderUpsert::export()?;
        AiProviderDraft::export()?;
        AiSecretInput::export()?;
        AiCompletionRequest::export()?;
        AiCompletionResponse::export()?;
        AiMessage::export()?;
        AiMessageRole::export()?;
        AiTestResult::export()?;
        AiTokenUsage::export()?;
        Ok(())
    }
}
