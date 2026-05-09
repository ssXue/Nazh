use std::collections::HashMap;

use nazh_core::ai::AiGenerationParams;

use crate::config::AiAgentSettings;

pub(super) struct ResolvedProvider {
    pub(super) base_url: String,
    pub(super) api_key: String,
    pub(super) default_model: String,
    pub(super) extra_headers: HashMap<String, String>,
}

pub(super) struct ResolvedProviderSnapshot {
    pub(super) provider: ResolvedProvider,
    pub(super) agent_settings: AiAgentSettings,
}

/// 流式请求上下文：spawned task 内部发起请求所需的所有数据。
pub(super) struct StreamRequestContext {
    pub(super) http: reqwest::Client,
    pub(super) url: String,
    pub(super) api_key: String,
    pub(super) extra_headers: HashMap<String, String>,
    pub(super) model: String,
    pub(super) params: AiGenerationParams,
    pub(super) include_deepseek_options: bool,
    pub(super) timeout_ms: u64,
}
