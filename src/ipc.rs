//! Tauri IPC 命令的请求/响应类型。
//!
//! 这些类型定义了前后端 IPC 通信的契约，从 `src-tauri` 迁移至引擎 crate
//! 以实现 ts-rs 统一生成 TypeScript 类型定义。

use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// 工作流部署成功后的响应。
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct DeployResponse {
    pub node_count: usize,
    pub edge_count: usize,
    pub root_nodes: Vec<String>,
    #[serde(default)]
    #[ts(optional)]
    pub project_id: Option<String>,
    #[serde(default)]
    #[ts(optional)]
    pub workflow_id: Option<String>,
    #[serde(default)]
    #[ts(optional)]
    pub replaced_existing: Option<bool>,
}

/// 载荷分发成功后的响应。
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct DispatchResponse {
    pub trace_id: String,
    #[serde(default)]
    #[ts(optional)]
    pub workflow_id: Option<String>,
}

/// 工作流卸载后的响应。
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct UndeployResponse {
    pub had_workflow: bool,
    pub aborted_timer_count: usize,
    #[serde(default)]
    #[ts(optional)]
    pub workflow_id: Option<String>,
}

/// 已注册节点类型的信息条目。
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct NodeTypeEntry {
    /// 节点类型主名称（如 "rhai"）。
    pub name: String,
    /// 别名列表（如 ["code", "code/rhai"]）。
    #[serde(default)]
    pub aliases: Vec<String>,
}

/// `list_node_types` IPC 命令的响应。
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct ListNodeTypesResponse {
    pub types: Vec<NodeTypeEntry>,
}
