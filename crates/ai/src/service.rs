//! AI 服务统一接口。

use async_trait::async_trait;

use crate::client::StreamChunk;
use crate::config::AiProviderDraft;
use crate::error::AiError;
use crate::types::{AiCompletionRequest, AiCompletionResponse, AiTestResult};

/// AI 服务统一接口，Copilot 和运行时节点共用。
#[async_trait]
pub trait AiService: Send + Sync {
    /// Chat completion。
    async fn complete(&self, request: AiCompletionRequest)
    -> Result<AiCompletionResponse, AiError>;

    /// 测试提供商连通性（支持草稿配置）。
    async fn test_connection(&self, draft: AiProviderDraft) -> Result<AiTestResult, AiError>;

    /// 流式 chat completion，逐 chunk 通过 channel 返回。
    async fn stream_complete(
        &self,
        request: AiCompletionRequest,
    ) -> Result<tokio::sync::mpsc::Receiver<Result<StreamChunk, AiError>>, AiError>;
}
