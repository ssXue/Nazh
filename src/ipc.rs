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
}

/// 载荷分发成功后的响应。
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct DispatchResponse {
    pub trace_id: String,
}

/// 工作流卸载后的响应。
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct UndeployResponse {
    pub had_workflow: bool,
    pub aborted_timer_count: usize,
}
