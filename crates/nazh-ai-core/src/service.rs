//! AI 服务 trait 定义。

use async_trait::async_trait;
use crate::error::AiError;
use crate::config::AiProviderDraft;
use crate::types::{AiCompletionRequest, AiCompletionResponse, AiTestResult};

#[async_trait]
pub trait AiService: Send + Sync {
    async fn complete(
        &self,
        _request: AiCompletionRequest,
    ) -> Result<AiCompletionResponse, AiError> {
        todo!()
    }

    async fn test_connection(&self, _draft: AiProviderDraft) -> Result<AiTestResult, AiError> {
        todo!()
    }
}
