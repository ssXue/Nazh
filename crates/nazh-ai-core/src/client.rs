//! `OpenAI` 兼容 HTTP 客户端实现。

use crate::config::AiConfigFile;
use std::sync::Arc;
use tokio::sync::RwLock;

pub struct OpenAiCompatibleService {
    _config: Arc<RwLock<AiConfigFile>>,
}

impl OpenAiCompatibleService {
    pub fn new(config: Arc<RwLock<AiConfigFile>>) -> Self {
        Self { _config: config }
    }
}
