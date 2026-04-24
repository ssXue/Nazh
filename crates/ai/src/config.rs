//! AI 配置模型：磁盘层、IPC 读取层、IPC 写入层。
//!
//! 密钥暴露原则：
//! - `api_key` 只存在于后端磁盘层 `AiProviderSecretRecord`
//! - 前端读取配置时只拿到 `AiProviderView`，永远不回传已保存的明文密钥
//! - 前端写入配置时使用 `AiSecretInput::{Keep, Clear, Set}` 表达密钥变更

use std::collections::HashMap;

use serde::{Deserialize, Serialize};
#[cfg(feature = "ts-export")]
use ts_rs::TS;

use crate::error::AiError;

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
        extra_headers: upsert.extra_headers,
        enabled: upsert.enabled,
    }
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
            extra_headers: self.extra_headers.clone(),
            enabled: self.enabled,
            has_api_key: !self.api_key.trim().is_empty(),
        }
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

/// DeepSeek/OpenAI 兼容的思考模式开关。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-export", derive(TS))]
#[cfg_attr(feature = "ts-export", ts(export))]
#[serde(rename_all = "lowercase")]
pub enum AiThinkingMode {
    Enabled,
    Disabled,
}

/// DeepSeek/OpenAI 兼容的思考模式配置。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-export", derive(TS))]
#[cfg_attr(feature = "ts-export", ts(export))]
#[serde(rename_all = "camelCase")]
pub struct AiThinkingConfig {
    #[serde(rename = "type")]
    pub kind: AiThinkingMode,
}

/// DeepSeek 推理强度。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-export", derive(TS))]
#[cfg_attr(feature = "ts-export", ts(export))]
#[serde(rename_all = "lowercase")]
pub enum AiReasoningEffort {
    High,
    Max,
}

/// Copilot 默认生成参数。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-export", derive(TS))]
#[cfg_attr(feature = "ts-export", ts(export))]
#[serde(rename_all = "camelCase")]
pub struct AiGenerationParams {
    #[serde(default = "default_copilot_temperature")]
    #[cfg_attr(feature = "ts-export", ts(optional))]
    pub temperature: Option<f32>,
    #[serde(default = "default_copilot_max_tokens")]
    #[cfg_attr(feature = "ts-export", ts(optional))]
    pub max_tokens: Option<u32>,
    #[serde(default = "default_copilot_top_p")]
    #[cfg_attr(feature = "ts-export", ts(optional))]
    pub top_p: Option<f32>,
    #[serde(default)]
    #[cfg_attr(feature = "ts-export", ts(optional))]
    pub thinking: Option<AiThinkingConfig>,
    #[serde(default)]
    #[cfg_attr(feature = "ts-export", ts(optional))]
    pub reasoning_effort: Option<AiReasoningEffort>,
}

#[allow(clippy::unnecessary_wraps)]
fn default_copilot_temperature() -> Option<f32> {
    Some(0.7)
}
#[allow(clippy::unnecessary_wraps)]
fn default_copilot_max_tokens() -> Option<u32> {
    Some(2048)
}
#[allow(clippy::unnecessary_wraps)]
fn default_copilot_top_p() -> Option<f32> {
    Some(1.0)
}

impl Default for AiGenerationParams {
    fn default() -> Self {
        Self {
            temperature: default_copilot_temperature(),
            max_tokens: default_copilot_max_tokens(),
            top_p: default_copilot_top_p(),
            thinking: None,
            reasoning_effort: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_provider_record(id: &str, api_key: &str) -> AiProviderSecretRecord {
        AiProviderSecretRecord {
            id: id.to_owned(),
            name: format!("测试提供商-{id}"),
            base_url: "https://api.example.com/v1".to_owned(),
            api_key: api_key.to_owned(),
            default_model: "test-model".to_owned(),
            extra_headers: HashMap::new(),
            enabled: true,
        }
    }

    #[test]
    fn to_view_excludes_api_key_and_marks_saved() {
        let record = sample_provider_record("p1", "sk-secret-123");
        let file = AiConfigFile {
            version: 1,
            providers: vec![record],
            active_provider_id: Some("p1".to_owned()),
            copilot_params: AiGenerationParams::default(),
            agent_settings: AiAgentSettings::default(),
        };
        let view = file.to_view();
        assert_eq!(view.providers.len(), 1);
        assert!(view.providers[0].has_api_key);
        assert_eq!(view.providers[0].id, "p1");
    }

    #[test]
    fn to_view_empty_key_marks_unsaved() {
        let record = sample_provider_record("p2", "");
        let file = AiConfigFile {
            version: 1,
            providers: vec![record],
            active_provider_id: None,
            copilot_params: AiGenerationParams::default(),
            agent_settings: AiAgentSettings::default(),
        };
        let view = file.to_view();
        assert!(!view.providers[0].has_api_key);
    }

    #[test]
    fn merge_update_keep_preserves_existing_key() {
        let record = sample_provider_record("p1", "sk-old-key");
        let mut file = AiConfigFile {
            version: 1,
            providers: vec![record],
            active_provider_id: None,
            copilot_params: AiGenerationParams::default(),
            agent_settings: AiAgentSettings::default(),
        };
        file.merge_update(AiConfigUpdate {
            version: 1,
            providers: vec![AiProviderUpsert {
                id: "p1".to_owned(),
                name: "新名称".to_owned(),
                base_url: "https://new.example.com/v1".to_owned(),
                default_model: "new-model".to_owned(),
                extra_headers: HashMap::new(),
                enabled: true,
                api_key: AiSecretInput::Keep,
            }],
            active_provider_id: Some("p1".to_owned()),
            copilot_params: AiGenerationParams::default(),
            agent_settings: AiAgentSettings::default(),
        });
        assert_eq!(file.providers[0].api_key, "sk-old-key");
        assert_eq!(file.providers[0].name, "新名称");
        assert_eq!(file.active_provider_id.as_deref(), Some("p1"));
    }

    #[test]
    fn merge_update_clear_removes_key() {
        let record = sample_provider_record("p1", "sk-old-key");
        let mut file = AiConfigFile {
            version: 1,
            providers: vec![record],
            active_provider_id: None,
            copilot_params: AiGenerationParams::default(),
            agent_settings: AiAgentSettings::default(),
        };
        file.merge_update(AiConfigUpdate {
            version: 1,
            providers: vec![AiProviderUpsert {
                id: "p1".to_owned(),
                name: "p1".to_owned(),
                base_url: "https://api.example.com/v1".to_owned(),
                default_model: "model".to_owned(),
                extra_headers: HashMap::new(),
                enabled: true,
                api_key: AiSecretInput::Clear,
            }],
            active_provider_id: None,
            copilot_params: AiGenerationParams::default(),
            agent_settings: AiAgentSettings::default(),
        });
        assert!(file.providers[0].api_key.is_empty());
    }

    #[test]
    fn merge_update_set_replaces_key() {
        let mut file = AiConfigFile::default();
        file.merge_update(AiConfigUpdate {
            version: 1,
            providers: vec![AiProviderUpsert {
                id: "p-new".to_owned(),
                name: "新提供商".to_owned(),
                base_url: "https://api.example.com/v1".to_owned(),
                default_model: "model".to_owned(),
                extra_headers: HashMap::new(),
                enabled: true,
                api_key: AiSecretInput::Set("sk-brand-new".to_owned()),
            }],
            active_provider_id: None,
            copilot_params: AiGenerationParams::default(),
            agent_settings: AiAgentSettings::default(),
        });
        assert_eq!(file.providers[0].api_key, "sk-brand-new");
    }

    #[test]
    fn merge_update_only_keeps_one_enabled_provider() {
        let mut file = AiConfigFile::default();
        file.merge_update(AiConfigUpdate {
            version: 1,
            providers: vec![
                AiProviderUpsert {
                    id: "p1".to_owned(),
                    name: "主提供商".to_owned(),
                    base_url: "https://api.example.com/v1".to_owned(),
                    default_model: "model-a".to_owned(),
                    extra_headers: HashMap::new(),
                    enabled: true,
                    api_key: AiSecretInput::Set("sk-a".to_owned()),
                },
                AiProviderUpsert {
                    id: "p2".to_owned(),
                    name: "备选提供商".to_owned(),
                    base_url: "https://api.example.org/v1".to_owned(),
                    default_model: "model-b".to_owned(),
                    extra_headers: HashMap::new(),
                    enabled: true,
                    api_key: AiSecretInput::Set("sk-b".to_owned()),
                },
            ],
            active_provider_id: Some("p2".to_owned()),
            copilot_params: AiGenerationParams::default(),
            agent_settings: AiAgentSettings::default(),
        });

        assert_eq!(file.active_provider_id.as_deref(), Some("p2"));
        assert!(!file.providers[0].enabled);
        assert!(file.providers[1].enabled);
    }

    #[test]
    fn merge_update_without_active_provider_falls_back_to_first_provider() {
        let mut file = AiConfigFile::default();
        file.merge_update(AiConfigUpdate {
            version: 1,
            providers: vec![
                AiProviderUpsert {
                    id: "p1".to_owned(),
                    name: "主提供商".to_owned(),
                    base_url: "https://api.example.com/v1".to_owned(),
                    default_model: "model-a".to_owned(),
                    extra_headers: HashMap::new(),
                    enabled: false,
                    api_key: AiSecretInput::Set("sk-a".to_owned()),
                },
                AiProviderUpsert {
                    id: "p2".to_owned(),
                    name: "备选提供商".to_owned(),
                    base_url: "https://api.example.org/v1".to_owned(),
                    default_model: "model-b".to_owned(),
                    extra_headers: HashMap::new(),
                    enabled: true,
                    api_key: AiSecretInput::Set("sk-b".to_owned()),
                },
            ],
            active_provider_id: None,
            copilot_params: AiGenerationParams::default(),
            agent_settings: AiAgentSettings::default(),
        });

        assert_eq!(file.active_provider_id.as_deref(), Some("p1"));
        assert!(file.providers[0].enabled);
        assert!(!file.providers[1].enabled);
    }

    #[test]
    fn to_view_includes_agent_settings() {
        let file = AiConfigFile {
            version: 1,
            providers: vec![sample_provider_record("p1", "sk-secret-123")],
            active_provider_id: Some("p1".to_owned()),
            copilot_params: AiGenerationParams::default(),
            agent_settings: AiAgentSettings {
                system_prompt: Some("你是全局代理".to_owned()),
                timeout_ms: Some(12_000),
            },
        };

        let view = file.to_view();

        assert_eq!(
            view.agent_settings.system_prompt.as_deref(),
            Some("你是全局代理")
        );
        assert_eq!(view.agent_settings.timeout_ms, Some(12_000));
    }

    #[test]
    fn default_generation_params() {
        let params = AiGenerationParams::default();
        assert_eq!(params.temperature, Some(0.7));
        assert_eq!(params.max_tokens, Some(2048));
        assert_eq!(params.top_p, Some(1.0));
        assert_eq!(params.thinking, None);
        assert_eq!(params.reasoning_effort, None);
    }
}
