use serde::{Deserialize, Serialize};

#[cfg(feature = "ts-export")]
use ts_rs::TS;

/// 可观测性上下文输入（`deploy_workflow` 参数之一）。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-export", derive(TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct ObservabilityContextInput {
    pub workspace_path: String,
    pub project_id: String,
    pub project_name: String,
    pub environment_id: String,
    pub environment_name: String,
    #[serde(default)]
    pub deployment_source: String,
}

/// 可观测性条目。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-export", derive(TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct ObservabilityEntry {
    pub id: String,
    pub timestamp: String,
    pub level: String,
    pub category: String,
    pub source: String,
    pub message: String,
    #[serde(default)]
    #[cfg_attr(feature = "ts-export", ts(optional))]
    pub detail: Option<String>,
    #[serde(default)]
    #[cfg_attr(feature = "ts-export", ts(optional))]
    pub trace_id: Option<String>,
    #[serde(default)]
    #[cfg_attr(feature = "ts-export", ts(optional))]
    pub node_id: Option<String>,
    #[serde(default)]
    #[cfg_attr(feature = "ts-export", ts(optional))]
    pub duration_ms: Option<u64>,
    #[serde(default)]
    #[cfg_attr(feature = "ts-export", ts(optional))]
    pub project_id: Option<String>,
    #[serde(default)]
    #[cfg_attr(feature = "ts-export", ts(optional))]
    pub project_name: Option<String>,
    #[serde(default)]
    #[cfg_attr(feature = "ts-export", ts(optional))]
    pub environment_id: Option<String>,
    #[serde(default)]
    #[cfg_attr(feature = "ts-export", ts(optional))]
    pub environment_name: Option<String>,
    #[serde(default)]
    #[cfg_attr(feature = "ts-export", ts(optional))]
    pub data: Option<serde_json::Value>,
    #[serde(default)]
    #[cfg_attr(feature = "ts-export", ts(optional))]
    pub event_kind: Option<String>,
}

/// 告警投递记录。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-export", derive(TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct AlertDeliveryRecord {
    pub id: String,
    pub timestamp: String,
    pub trace_id: String,
    pub node_id: String,
    pub project_id: String,
    pub project_name: String,
    pub environment_id: String,
    pub environment_name: String,
    pub url: String,
    pub method: String,
    pub status: u16,
    pub success: bool,
    #[serde(default)]
    #[cfg_attr(feature = "ts-export", ts(optional))]
    pub webhook_kind: Option<String>,
    #[serde(default)]
    #[cfg_attr(feature = "ts-export", ts(optional))]
    pub body_mode: Option<String>,
    #[serde(default)]
    #[cfg_attr(feature = "ts-export", ts(optional))]
    pub request_timeout_ms: Option<u64>,
    #[serde(default)]
    #[cfg_attr(feature = "ts-export", ts(optional))]
    pub requested_at: Option<String>,
    #[serde(default)]
    #[cfg_attr(feature = "ts-export", ts(optional))]
    pub request_body_preview: Option<String>,
}

/// 可观测性 trace 摘要。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-export", derive(TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct ObservabilityTraceSummary {
    pub trace_id: String,
    pub status: String,
    #[serde(default)]
    #[cfg_attr(feature = "ts-export", ts(optional))]
    pub started_at: Option<String>,
    #[serde(default)]
    #[cfg_attr(feature = "ts-export", ts(optional))]
    pub last_seen_at: Option<String>,
    pub total_events: usize,
    pub node_count: usize,
    pub output_count: usize,
    pub failure_count: usize,
    #[serde(default)]
    #[cfg_attr(feature = "ts-export", ts(optional))]
    pub total_duration_ms: Option<u64>,
    #[serde(default)]
    #[cfg_attr(feature = "ts-export", ts(optional))]
    pub last_node_id: Option<String>,
    #[serde(default)]
    #[cfg_attr(feature = "ts-export", ts(optional))]
    pub project_name: Option<String>,
    #[serde(default)]
    #[cfg_attr(feature = "ts-export", ts(optional))]
    pub environment_name: Option<String>,
}

/// `query_observability` IPC 命令的响应。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-export", derive(TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct ObservabilityQueryResult {
    pub entries: Vec<ObservabilityEntry>,
    pub traces: Vec<ObservabilityTraceSummary>,
    pub alerts: Vec<AlertDeliveryRecord>,
    pub audits: Vec<ObservabilityEntry>,
}

/// 部署审计记录（IPC 响应类型，RFC-0003 Phase 3）。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-export", derive(TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct DeploymentAuditEntry {
    pub id: String,
    pub workflow_id: String,
    pub action: String,
    pub level: String,
    pub timestamp: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts-export", ts(optional))]
    pub project_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts-export", ts(optional))]
    pub project_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts-export", ts(optional))]
    pub environment_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts-export", ts(optional))]
    pub environment_name: Option<String>,
    pub message: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts-export", ts(optional))]
    pub detail: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts-export", ts(optional))]
    pub data: Option<serde_json::Value>,
}

/// `query_deployment_audit` IPC 命令的响应。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-export", derive(TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct DeploymentAuditQueryResult {
    pub records: Vec<DeploymentAuditEntry>,
}
