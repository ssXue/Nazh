//! AI 配置模型：磁盘层、IPC 读取层、IPC 写入层。

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use ts_rs::TS;

const fn default_true() -> bool {
    true
}

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
}

impl Default for AiConfigFile {
    fn default() -> Self {
        Self {
            version: 1,
            providers: Vec::new(),
            active_provider_id: None,
            copilot_params: AiGenerationParams::default(),
        }
    }
}

impl AiConfigFile {
    pub fn to_view(&self) -> AiConfigView {
        todo!()
    }
    pub fn merge_update(&mut self, _update: AiConfigUpdate) {
        todo!()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct AiConfigView {
    pub version: u8,
    pub providers: Vec<AiProviderView>,
    #[serde(default)]
    #[ts(optional)]
    pub active_provider_id: Option<String>,
    #[serde(default)]
    pub copilot_params: AiGenerationParams,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct AiConfigUpdate {
    pub version: u8,
    #[serde(default)]
    pub providers: Vec<AiProviderUpsert>,
    #[serde(default)]
    #[ts(optional)]
    pub active_provider_id: Option<String>,
    #[serde(default)]
    pub copilot_params: AiGenerationParams,
}

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

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
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

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
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

#[derive(Debug, Clone, Serialize, Deserialize, TS, Default)]
#[ts(export)]
#[serde(rename_all = "camelCase", tag = "kind", content = "value")]
pub enum AiSecretInput {
    #[default]
    Keep,
    Clear,
    Set(String),
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct AiProviderDraft {
    #[serde(default)]
    #[ts(optional)]
    pub id: Option<String>,
    pub name: String,
    pub base_url: String,
    #[serde(default)]
    #[ts(optional)]
    pub api_key: Option<String>,
    pub default_model: String,
    #[serde(default)]
    pub extra_headers: HashMap<String, String>,
    #[serde(default = "default_true")]
    pub enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct AiGenerationParams {
    #[serde(default)]
    #[ts(optional)]
    pub temperature: Option<f32>,
    #[serde(default)]
    #[ts(optional)]
    pub max_tokens: Option<u32>,
    #[serde(default)]
    #[ts(optional)]
    pub top_p: Option<f32>,
}

impl Default for AiGenerationParams {
    fn default() -> Self {
        Self {
            temperature: Some(0.7),
            max_tokens: Some(2048),
            top_p: Some(1.0),
        }
    }
}
