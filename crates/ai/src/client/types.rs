use std::collections::HashMap;

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
