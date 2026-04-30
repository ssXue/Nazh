//! # Nazh AI
//!
//! `OpenAI` 兼容 API 的 HTTP 客户端实现 + 壳层私有 AI 配置模型（磁盘 / IPC）。
//! `AiService` trait 与请求/响应类型在 Ring 0（`nazh_core::ai`）。

pub mod client;
pub mod config;
pub mod types;

pub use client::OpenAiCompatibleService;
pub use config::{
    AiAgentSettings, AiConfigFile, AiConfigUpdate, AiConfigView, AiProviderDraft,
    AiProviderSecretRecord, AiProviderUpsert, AiProviderView, AiSecretInput,
};
pub use types::AiTestResult;

/// ts-rs 类型导出入口。仅在 `ts-export` feature 启用时编译。
#[cfg(feature = "ts-export")]
pub mod export_bindings {
    use crate::config::{
        AiAgentSettings, AiConfigUpdate, AiConfigView, AiProviderDraft, AiProviderUpsert,
        AiProviderView, AiSecretInput,
    };
    use crate::types::AiTestResult;
    use ts_rs::{Config, ExportError, TS};

    pub fn export_all() -> Result<(), ExportError> {
        let cfg = Config::from_env();

        AiAgentSettings::export(&cfg)?;
        AiConfigView::export(&cfg)?;
        AiConfigUpdate::export(&cfg)?;
        AiProviderView::export(&cfg)?;
        AiProviderUpsert::export(&cfg)?;
        AiProviderDraft::export(&cfg)?;
        AiSecretInput::export(&cfg)?;
        AiTestResult::export(&cfg)?;
        Ok(())
    }
}
