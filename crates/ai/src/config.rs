//! AI 配置模型：磁盘层、IPC 读取层、IPC 写入层。
//!
//! 密钥暴露原则：
//! - `api_key` 只存在于后端磁盘层 `AiProviderSecretRecord`
//! - 前端读取配置时只拿到 `AiProviderView`，永远不回传已保存的明文密钥
//! - 前端写入配置时使用 `AiSecretInput::{Keep, Clear, Set}` 表达密钥变更

use std::{collections::HashMap, hash::BuildHasher};

use serde::{Deserialize, Serialize};
#[cfg(feature = "ts-export")]
use ts_rs::TS;

use nazh_core::ai::{AiError, AiGenerationParams};

const fn default_true() -> bool {
    true
}

/// 磁盘中的 AI 配置（后端私有，包含密钥）。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AiConfigFile {
    pub version: u8,
    #[serde(default)]
    pub providers: Vec<AiProviderSecretRecord>,
    #[serde(default)]
    pub active_provider_id: Option<String>,
    #[serde(default)]
    pub copilot_params: AiGenerationParams,
    #[serde(default)]
    pub agent_settings: AiAgentSettings,
}

impl Default for AiConfigFile {
    fn default() -> Self {
        Self {
            version: 1,
            providers: Vec::new(),
            active_provider_id: None,
            copilot_params: AiGenerationParams::default(),
            agent_settings: AiAgentSettings::default(),
        }
    }
}

impl AiConfigFile {
    /// 投影为前端可见的只读视图（不含明文密钥）。
    pub fn to_view(&self) -> AiConfigView {
        AiConfigView {
            version: self.version,
            providers: self
                .providers
                .iter()
                .map(AiProviderSecretRecord::to_view)
                .collect(),
            active_provider_id: self.active_provider_id.clone(),
            copilot_params: self.copilot_params.clone(),
            agent_settings: self.agent_settings.clone(),
        }
    }

    /// 将前端写入模型合并为新的磁盘配置。
    pub fn merge_update(&mut self, update: AiConfigUpdate) {
        self.version = update.version;
        self.active_provider_id = update.active_provider_id;
        self.copilot_params = update.copilot_params;
        self.agent_settings = update.agent_settings;

        let existing_map: HashMap<String, AiProviderSecretRecord> = {
            let mut map = HashMap::new();
            for provider in self.providers.drain(..) {
                map.insert(provider.id.clone(), provider);
            }
            map
        };

        self.providers = update
            .providers
            .into_iter()
            .map(|upsert| {
                let existing = existing_map.get(&upsert.id);
                merge_provider_upsert(existing, upsert)
            })
            .collect();

        self.normalize();
    }

    /// 规范化全局 AI：任意时刻最多只有一个启用的 provider。
    pub fn normalize(&mut self) {
        self.active_provider_id =
            normalize_active_provider_id(self.active_provider_id.clone(), &mut self.providers);
    }
}

fn normalize_active_provider_id(
    requested_active_provider_id: Option<String>,
    providers: &mut [AiProviderSecretRecord],
) -> Option<String> {
    let resolved_active_provider_id = requested_active_provider_id
        .filter(|id| providers.iter().any(|provider| provider.id == *id))
        .or_else(|| providers.first().map(|provider| provider.id.clone()));

    for provider in providers {
        provider.enabled = resolved_active_provider_id.as_deref() == Some(provider.id.as_str());
    }

    resolved_active_provider_id
}

fn merge_provider_upsert(
    existing: Option<&AiProviderSecretRecord>,
    upsert: AiProviderUpsert,
) -> AiProviderSecretRecord {
    let api_key = match upsert.api_key {
        AiSecretInput::Keep => existing
            .map(|record| record.api_key.clone())
            .unwrap_or_default(),
        AiSecretInput::Clear => String::new(),
        AiSecretInput::Set(new_key) => new_key,
    };

    AiProviderSecretRecord {
        id: upsert.id,
        name: upsert.name,
        base_url: upsert.base_url,
        api_key,
        default_model: upsert.default_model,
        extra_headers: filter_non_sensitive_extra_headers(&upsert.extra_headers),
        enabled: upsert.enabled,
    }
}

fn is_sensitive_extra_header(name: &str) -> bool {
    let normalized = name.trim().to_ascii_lowercase();
    matches!(
        normalized.as_str(),
        "authorization"
            | "proxy-authorization"
            | "x-api-key"
            | "api-key"
            | "apikey"
            | "x-auth-token"
            | "x-access-token"
            | "access-token"
            | "token"
            | "cookie"
            | "set-cookie"
    )
}

/// 过滤仅允许作为非敏感明文配置保存和回传的 extra headers。
pub fn filter_non_sensitive_extra_headers<S: BuildHasher>(
    headers: &HashMap<String, String, S>,
) -> HashMap<String, String> {
    headers
        .iter()
        .filter(|(key, _)| !is_sensitive_extra_header(key))
        .map(|(key, value)| (key.clone(), value.clone()))
        .collect()
}

/// 前端读取配置时使用的只读视图（不含明文密钥）。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-export", derive(TS))]
#[cfg_attr(feature = "ts-export", ts(export))]
#[serde(rename_all = "camelCase")]
pub struct AiConfigView {
    pub version: u8,
    pub providers: Vec<AiProviderView>,
    #[serde(default)]
    #[cfg_attr(feature = "ts-export", ts(optional))]
    pub active_provider_id: Option<String>,
    #[serde(default)]
    pub copilot_params: AiGenerationParams,
    #[serde(default)]
    pub agent_settings: AiAgentSettings,
}

/// 前端保存配置时使用的写入模型。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-export", derive(TS))]
#[cfg_attr(feature = "ts-export", ts(export))]
#[serde(rename_all = "camelCase")]
pub struct AiConfigUpdate {
    pub version: u8,
    #[serde(default)]
    pub providers: Vec<AiProviderUpsert>,
    #[serde(default)]
    #[cfg_attr(feature = "ts-export", ts(optional))]
    pub active_provider_id: Option<String>,
    #[serde(default)]
    pub copilot_params: AiGenerationParams,
    #[serde(default)]
    pub agent_settings: AiAgentSettings,
}

/// 全局脚本 AI 代理设置。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-export", derive(TS))]
#[cfg_attr(feature = "ts-export", ts(export))]
#[serde(rename_all = "camelCase")]
pub struct AiAgentSettings {
    #[serde(default)]
    #[cfg_attr(feature = "ts-export", ts(optional))]
    pub system_prompt: Option<String>,
    #[serde(default)]
    #[cfg_attr(feature = "ts-export", ts(optional))]
    pub timeout_ms: Option<u64>,
    #[serde(default)]
    pub thinking_enabled: bool,
    /// 是否启用 copilot 工具调用。
    #[serde(default)]
    pub tool_calling_enabled: bool,
}

/// 磁盘中的单个 AI 提供商记录（含密钥）。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AiProviderSecretRecord {
    pub id: String,
    pub name: String,
    pub base_url: String,
    pub api_key: String,
    pub default_model: String,
    #[serde(default)]
    pub extra_headers: HashMap<String, String>,
    #[serde(default = "default_true")]
    pub enabled: bool,
}

impl AiProviderSecretRecord {
    /// 投影为前端可见视图。
    fn to_view(&self) -> AiProviderView {
        AiProviderView {
            id: self.id.clone(),
            name: self.name.clone(),
            base_url: self.base_url.clone(),
            default_model: self.default_model.clone(),
            extra_headers: self.non_sensitive_extra_headers(),
            enabled: self.enabled,
            has_api_key: !self.api_key.trim().is_empty(),
        }
    }

    /// 返回可明文使用的非敏感 extra headers。
    pub fn non_sensitive_extra_headers(&self) -> HashMap<String, String> {
        filter_non_sensitive_extra_headers(&self.extra_headers)
    }

    /// 根据 ID 在列表中查找提供商。
    pub fn find_by_id<'a>(providers: &'a [Self], id: &str) -> Result<&'a Self, AiError> {
        providers
            .iter()
            .find(|provider| provider.id == id)
            .ok_or_else(|| AiError::ProviderNotFound(id.to_owned()))
    }

    /// 根据 ID 查找提供商，若已禁用则报错。
    pub fn find_active_by_id<'a>(providers: &'a [Self], id: &str) -> Result<&'a Self, AiError> {
        let provider = Self::find_by_id(providers, id)?;
        if !provider.enabled {
            return Err(AiError::ProviderDisabled(id.to_owned()));
        }
        Ok(provider)
    }
}

/// 前端可见的提供商配置视图。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-export", derive(TS))]
#[cfg_attr(feature = "ts-export", ts(export))]
#[serde(rename_all = "camelCase")]
pub struct AiProviderView {
    pub id: String,
    pub name: String,
    pub base_url: String,
    pub default_model: String,
    #[serde(default)]
    pub extra_headers: HashMap<String, String>,
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default)]
    pub has_api_key: bool,
}

/// 前端保存配置时的提供商输入。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-export", derive(TS))]
#[cfg_attr(feature = "ts-export", ts(export))]
#[serde(rename_all = "camelCase")]
pub struct AiProviderUpsert {
    pub id: String,
    pub name: String,
    pub base_url: String,
    pub default_model: String,
    #[serde(default)]
    pub extra_headers: HashMap<String, String>,
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default)]
    pub api_key: AiSecretInput,
}

/// API Key 的写入指令，避免前端回读已保存明文。
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[cfg_attr(feature = "ts-export", derive(TS))]
#[cfg_attr(feature = "ts-export", ts(export))]
#[serde(rename_all = "camelCase", tag = "kind", content = "value")]
pub enum AiSecretInput {
    #[default]
    Keep,
    Clear,
    Set(String),
}

/// 测试连接时使用的草稿输入。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-export", derive(TS))]
#[cfg_attr(feature = "ts-export", ts(export))]
#[serde(rename_all = "camelCase")]
pub struct AiProviderDraft {
    #[serde(default)]
    #[cfg_attr(feature = "ts-export", ts(optional))]
    pub id: Option<String>,
    pub name: String,
    pub base_url: String,
    #[serde(default)]
    #[cfg_attr(feature = "ts-export", ts(optional))]
    pub api_key: Option<String>,
    pub default_model: String,
    #[serde(default)]
    pub extra_headers: HashMap<String, String>,
    #[serde(default = "default_true")]
    pub enabled: bool,
}

#[cfg(test)]
#[path = "config_tests.rs"]
mod tests;
