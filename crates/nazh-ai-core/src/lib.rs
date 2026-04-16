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

pub use client::OpenAiCompatibleService;
pub use config::{
    AiConfigFile, AiConfigUpdate, AiConfigView, AiGenerationParams, AiProviderDraft,
    AiProviderSecretRecord, AiProviderUpsert, AiProviderView, AiSecretInput,
};
pub use error::AiError;
pub use service::AiService;
pub use types::{
    AiCompletionRequest, AiCompletionResponse, AiMessage, AiMessageRole, AiTestResult, AiTokenUsage,
};

#[cfg(test)]
mod export_bindings {
    //! ts-rs 类型导出入口，通过 `cargo test export_bindings` 触发生成。

    use super::*;
    use ts_rs::TS;

    #[test]
    fn export_ai_types() {
        let _ =
            std::fs::create_dir_all(std::env::var("OUT_DIR").unwrap_or_else(|_| "/tmp".to_owned()));
        let _ = AiConfigView::export();
        let _ = AiConfigUpdate::export();
        let _ = AiGenerationParams::export();
        let _ = AiProviderView::export();
        let _ = AiProviderUpsert::export();
        let _ = AiProviderDraft::export();
        let _ = AiSecretInput::export();
        let _ = AiCompletionRequest::export();
        let _ = AiCompletionResponse::export();
        let _ = AiMessage::export();
        let _ = AiMessageRole::export();
        let _ = AiTestResult::export();
        let _ = AiTokenUsage::export();
    }
}
