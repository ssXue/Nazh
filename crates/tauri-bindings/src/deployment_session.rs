use serde::{Deserialize, Serialize};

#[cfg(feature = "ts-export")]
use ts_rs::TS;

/// 持久化部署会话条目。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-export", derive(TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct PersistedDeploymentSession {
    pub version: u8,
    pub project_id: String,
    pub project_name: String,
    pub environment_id: String,
    pub environment_name: String,
    pub deployed_at: String,
    pub runtime_ast_text: String,
}

/// 持久化部署会话集合（文件格式）。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-export", derive(TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct PersistedDeploymentSessionCollection {
    pub version: u8,
    #[serde(default)]
    #[cfg_attr(feature = "ts-export", ts(optional))]
    pub active_project_id: Option<String>,
    pub sessions: Vec<PersistedDeploymentSession>,
}

/// 持久化部署会话状态。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-export", derive(TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct PersistedDeploymentSessionState {
    pub version: u8,
    #[serde(default)]
    #[cfg_attr(feature = "ts-export", ts(optional))]
    pub active_project_id: Option<String>,
    pub sessions: Vec<PersistedDeploymentSession>,
}
