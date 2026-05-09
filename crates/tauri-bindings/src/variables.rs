use std::collections::HashMap;

use nazh_core::TypedVariableSnapshot;
use serde::{Deserialize, Serialize};

#[cfg(feature = "ts-export")]
use ts_rs::TS;

/// `set_workflow_variable` 命令的请求（ADR-0012 Phase 2）。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-export", derive(TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct SetWorkflowVariableRequest {
    pub workflow_id: String,
    pub name: String,
    pub value: serde_json::Value,
}

/// `set_workflow_variable` 命令的响应（ADR-0012 Phase 2）。
///
/// 成功时返回写入后的快照（含新 `updated_at` / `updated_by = Some("ipc")`）；
/// 类型不匹配 / 变量未声明 / 工作流未部署等错误通过 `Err(String)` 上抛。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-export", derive(TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct SetWorkflowVariableResponse {
    pub snapshot: TypedVariableSnapshot,
}

/// `delete_workflow_variable` 命令的请求（ADR-0012 Phase 3）。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-export", derive(TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct DeleteWorkflowVariableRequest {
    pub workflow_id: String,
    pub name: String,
}

/// `delete_workflow_variable` 命令的响应（ADR-0012 Phase 3）。
///
/// 返回被删除变量在删除前的快照；变量不存在时为 `None`（幂等）。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-export", derive(TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct DeleteWorkflowVariableResponse {
    #[cfg_attr(feature = "ts-export", ts(optional))]
    pub removed_snapshot: Option<TypedVariableSnapshot>,
}

/// `reset_workflow_variable` 命令的请求（ADR-0012 Phase 3）。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-export", derive(TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct ResetWorkflowVariableRequest {
    pub workflow_id: String,
    pub name: String,
}

/// `reset_workflow_variable` 命令的响应（ADR-0012 Phase 3）。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-export", derive(TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct ResetWorkflowVariableResponse {
    pub snapshot: TypedVariableSnapshot,
}

/// `query_variable_history` 命令的请求（ADR-0012 Phase 3）。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-export", derive(TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct QueryVariableHistoryRequest {
    pub workflow_id: String,
    pub name: String,
    #[serde(default)]
    #[cfg_attr(feature = "ts-export", ts(optional))]
    pub limit: Option<u32>,
}

/// 历史记录条目（ADR-0012 Phase 3）。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-export", derive(TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct HistoryEntryPayload {
    pub value: serde_json::Value,
    pub updated_at: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts-export", ts(optional))]
    pub updated_by: Option<String>,
}

/// `query_variable_history` 命令的响应（ADR-0012 Phase 3）。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-export", derive(TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct QueryVariableHistoryResponse {
    pub entries: Vec<HistoryEntryPayload>,
}

/// `set_global_variable` 命令的请求（ADR-0012 Phase 3）。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-export", derive(TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct SetGlobalVariableRequest {
    pub namespace: String,
    pub key: String,
    pub value: serde_json::Value,
    #[serde(default)]
    #[cfg_attr(feature = "ts-export", ts(optional))]
    pub var_type: Option<String>,
}

/// 全局变量快照（ADR-0012 Phase 3）。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-export", derive(TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct GlobalVariableSnapshot {
    pub namespace: String,
    pub key: String,
    pub value: serde_json::Value,
    pub var_type: String,
    pub updated_at: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts-export", ts(optional))]
    pub updated_by: Option<String>,
}

/// `set_global_variable` 命令的响应（ADR-0012 Phase 3）。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-export", derive(TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct SetGlobalVariableResponse {
    pub snapshot: GlobalVariableSnapshot,
}

/// `get_global_variable` 命令的请求（ADR-0012 Phase 3）。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-export", derive(TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct GetGlobalVariableRequest {
    pub namespace: String,
    pub key: String,
}

/// `get_global_variable` 命令的响应（ADR-0012 Phase 3）。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-export", derive(TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct GetGlobalVariableResponse {
    #[cfg_attr(feature = "ts-export", ts(optional))]
    pub snapshot: Option<GlobalVariableSnapshot>,
}

/// `list_global_variables` 命令的请求（ADR-0012 Phase 3）。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-export", derive(TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct ListGlobalVariablesRequest {
    #[serde(default)]
    #[cfg_attr(feature = "ts-export", ts(optional))]
    pub namespace: Option<String>,
}

/// `list_global_variables` 命令的响应（ADR-0012 Phase 3）。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-export", derive(TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct ListGlobalVariablesResponse {
    pub variables: Vec<GlobalVariableSnapshot>,
}

/// `delete_global_variable` 命令的请求（ADR-0012 Phase 3）。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-export", derive(TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct DeleteGlobalVariableRequest {
    pub namespace: String,
    pub key: String,
}

/// `workflow://variable-changed` 事件载荷（ADR-0012 Phase 2）。
///
/// 由 Tauri shell 的变量事件 drain 循环从 [`nazh_core::WorkflowVariableEvent::Changed`]
/// 直接构造——不再从 `ExecutionEvent` 分支（B1-R0-01/B1-R0-05）。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-export", derive(TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct VariableChangedPayload {
    pub workflow_id: String,
    pub name: String,
    pub value: serde_json::Value,
    pub updated_at: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts-export", ts(optional))]
    pub updated_by: Option<String>,
}

/// `workflow://variable-deleted` 事件载荷（ADR-0012 Phase 3）。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-export", derive(TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct VariableDeletedPayload {
    pub workflow_id: String,
    pub name: String,
}

/// Reactive 引脚值变更推送载荷（ADR-0015 Phase 2 IPC）。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-export", derive(TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct ReactiveUpdatePayload {
    pub workflow_id: String,
    pub node_id: String,
    pub pin_id: String,
    pub value: serde_json::Value,
    pub updated_at: String,
}

/// `snapshot_workflow_variables` 命令的请求。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-export", derive(TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct SnapshotWorkflowVariablesRequest {
    /// 要查询的工作流 ID。
    pub workflow_id: String,
}

/// `snapshot_workflow_variables` 命令的响应——按变量名映射到序列化快照。
///
/// `updated_at` 字段为 RFC3339 字符串，避免前端时区差。
/// 空表表示部署已声明但无变量；若部署不存在，命令侧返回错误（而非空表）以避免歧义。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-export", derive(TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct SnapshotWorkflowVariablesResponse {
    pub variables: HashMap<String, TypedVariableSnapshot>,
}
