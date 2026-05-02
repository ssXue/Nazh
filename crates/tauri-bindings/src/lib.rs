//! Nazh Tauri 命令的请求/响应类型集中地。
//!
//! 这些类型只服务于 Tauri 桌面壳层与前端的 IPC 契约，不属于引擎运行时；
//! 因此从 Ring 0（`nazh-core`）迁出，独立成一个 crate。详见 ADR-0017。
//!
//! `ts-rs` 通过 `ts-export` feature 启用，CI 用
//! `cargo test -p tauri-bindings --features ts-export export_bindings`
//! 触发本 crate 与所有依赖 crate 的 TypeScript 类型导出。

use std::collections::HashMap;

use nazh_core::{NodeRegistry, PinDefinition, TypedVariableSnapshot};
use nazh_engine::ConnectionDefinition;
use serde::{Deserialize, Serialize};

#[cfg(feature = "ts-export")]
use ts_rs::{Config, TS};

/// 工作流部署成功后的响应。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-export", derive(TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct DeployResponse {
    pub node_count: usize,
    pub edge_count: usize,
    pub root_nodes: Vec<String>,
    #[serde(default)]
    #[cfg_attr(feature = "ts-export", ts(optional))]
    pub project_id: Option<String>,
    #[serde(default)]
    #[cfg_attr(feature = "ts-export", ts(optional))]
    pub workflow_id: Option<String>,
    #[serde(default)]
    #[cfg_attr(feature = "ts-export", ts(optional))]
    pub replaced_existing: Option<bool>,
}

/// 载荷分发成功后的响应。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-export", derive(TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct DispatchResponse {
    pub trace_id: String,
    #[serde(default)]
    #[cfg_attr(feature = "ts-export", ts(optional))]
    pub workflow_id: Option<String>,
}

/// 工作流卸载后的响应。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-export", derive(TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct UndeployResponse {
    pub had_workflow: bool,
    pub aborted_timer_count: usize,
    #[serde(default)]
    #[cfg_attr(feature = "ts-export", ts(optional))]
    pub workflow_id: Option<String>,
}

/// 已注册节点类型的信息条目。
///
/// `capabilities` 是 [`nazh_core::NodeCapabilities`] 的原始位图（`u32::bits()`），
/// 前端需按 ADR-0011 定义的位分配解读。位分配与常量表同步在
/// `web/src/lib/nodeCapabilities.ts`。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-export", derive(TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct NodeTypeEntry {
    /// 节点类型主名称（如 "code"）。
    pub name: String,
    /// 类型级能力标签位图（详见 ADR-0011）。
    #[serde(default)]
    #[cfg_attr(feature = "ts-export", ts(type = "number"))]
    pub capabilities: u32,
}

/// `list_node_types` IPC 命令的响应。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-export", derive(TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct ListNodeTypesResponse {
    pub types: Vec<NodeTypeEntry>,
}

/// `describe_node_pins` IPC 命令的请求。
///
/// 给定节点类型 + config，返回该实例化节点的输入/输出引脚 schema。
/// 服务于前端连接期校验——FlowGram `canAddLine` 钩子通过缓存的 pin
/// schema 即时判断"上游产出 → 下游期望"是否兼容。
///
/// 注意：`config` 必须是合法的节点 config（能让 [`NodeRegistry::create`]
/// 成功）。无效 config 会返回错误，前端缓存写 fallback `Any/Any`，
/// 部署期校验作为 backstop 兜底。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-export", derive(TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct DescribeNodePinsRequest {
    /// 节点类型主名称（如 `"modbusRead"` / `"switch"` / `"mqttClient"`）。
    pub node_type: String,
    /// 节点 config（与 `WorkflowNodeDefinition::config` 同 schema）。
    pub config: serde_json::Value,
}

/// `describe_node_pins` IPC 命令的响应。
///
/// 直接返回 [`PinDefinition`] 列表，前端 ts-rs 已导出该类型——
/// 与节点 trait 的 `input_pins(&self)` / `output_pins(&self)` 同形态。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-export", derive(TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct DescribeNodePinsResponse {
    pub input_pins: Vec<PinDefinition>,
    pub output_pins: Vec<PinDefinition>,
}

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

/// 把 [`NodeRegistry`] 中的节点类型按字母排序后包装成 [`ListNodeTypesResponse`]。
///
/// 排序属于 IPC 展示层关注点，不污染 Ring 0 的注册表 API。
pub fn list_node_types_response(registry: &NodeRegistry) -> ListNodeTypesResponse {
    let mut names: Vec<String> = registry
        .registered_types()
        .into_iter()
        .map(str::to_owned)
        .collect();
    names.sort_unstable();
    ListNodeTypesResponse {
        types: names
            .into_iter()
            .map(|name| {
                let capabilities = registry.capabilities_of(&name).unwrap_or_default().bits();
                NodeTypeEntry { name, capabilities }
            })
            .collect(),
    }
}

// ── 运行时 IPC 类型（从 src-tauri/src/runtime.rs 迁入） ──────────

/// 调度队列背压策略。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[cfg_attr(feature = "ts-export", derive(TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub enum RuntimeBackpressureStrategy {
    #[default]
    Block,
    RejectNewest,
}

/// 工作流运行时策略（队列容量 / 背压 / 重试）。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-export", derive(TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct WorkflowRuntimePolicy {
    pub manual_queue_capacity: usize,
    pub trigger_queue_capacity: usize,
    pub manual_backpressure_strategy: RuntimeBackpressureStrategy,
    pub trigger_backpressure_strategy: RuntimeBackpressureStrategy,
    pub max_retry_attempts: u32,
    pub initial_retry_backoff_ms: u64,
    pub max_retry_backoff_ms: u64,
}

impl Default for WorkflowRuntimePolicy {
    fn default() -> Self {
        Self {
            manual_queue_capacity: 64,
            trigger_queue_capacity: 256,
            manual_backpressure_strategy: RuntimeBackpressureStrategy::Block,
            trigger_backpressure_strategy: RuntimeBackpressureStrategy::RejectNewest,
            max_retry_attempts: 3,
            initial_retry_backoff_ms: 150,
            max_retry_backoff_ms: 2_000,
        }
    }
}

/// 工作流运行时策略输入（所有字段可选，缺省用默认值填充）。
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[cfg_attr(feature = "ts-export", derive(TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct WorkflowRuntimePolicyInput {
    #[serde(default)]
    pub manual_queue_capacity: Option<usize>,
    #[serde(default)]
    pub trigger_queue_capacity: Option<usize>,
    #[serde(default)]
    pub manual_backpressure_strategy: Option<RuntimeBackpressureStrategy>,
    #[serde(default)]
    pub trigger_backpressure_strategy: Option<RuntimeBackpressureStrategy>,
    #[serde(default)]
    pub max_retry_attempts: Option<u32>,
    #[serde(default)]
    pub initial_retry_backoff_ms: Option<u64>,
    #[serde(default)]
    pub max_retry_backoff_ms: Option<u64>,
}

impl WorkflowRuntimePolicy {
    /// 从可选的输入构建 `WorkflowRuntimePolicy`，缺失字段用默认值填充。
    pub fn from_input(input: Option<WorkflowRuntimePolicyInput>) -> Self {
        let defaults = Self::default();
        let Some(input) = input else {
            return defaults;
        };

        Self {
            manual_queue_capacity: input
                .manual_queue_capacity
                .map_or(defaults.manual_queue_capacity, normalize_queue_capacity),
            trigger_queue_capacity: input
                .trigger_queue_capacity
                .map_or(defaults.trigger_queue_capacity, normalize_queue_capacity),
            manual_backpressure_strategy: input
                .manual_backpressure_strategy
                .unwrap_or(defaults.manual_backpressure_strategy),
            trigger_backpressure_strategy: input
                .trigger_backpressure_strategy
                .unwrap_or(defaults.trigger_backpressure_strategy),
            max_retry_attempts: input
                .max_retry_attempts
                .map_or(defaults.max_retry_attempts, |value| value.min(8)),
            initial_retry_backoff_ms: input
                .initial_retry_backoff_ms
                .map_or(defaults.initial_retry_backoff_ms, |value| {
                    value.clamp(25, 5_000)
                }),
            max_retry_backoff_ms: input
                .max_retry_backoff_ms
                .map_or(defaults.max_retry_backoff_ms, |value| {
                    value.clamp(100, 30_000)
                }),
        }
    }
}

fn normalize_queue_capacity(value: usize) -> usize {
    value.clamp(1, 4_096)
}

/// 调度队列指标快照。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-export", derive(TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct DispatchLaneSnapshot {
    pub depth: usize,
    pub accepted: u64,
    pub retried: u64,
    pub dead_lettered: u64,
}

/// 已部署工作流的运行时摘要（`list_runtime_workflows` / `set_active_runtime_workflow`）。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-export", derive(TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct RuntimeWorkflowSummary {
    pub workflow_id: String,
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
    pub deployed_at: String,
    pub node_count: usize,
    pub edge_count: usize,
    pub root_nodes: Vec<String>,
    pub active: bool,
    pub policy: WorkflowRuntimePolicy,
    pub manual_lane: DispatchLaneSnapshot,
    pub trigger_lane: DispatchLaneSnapshot,
}

/// 死信记录（`list_dead_letters`）。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-export", derive(TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct DeadLetterRecord {
    pub id: String,
    pub timestamp: String,
    pub workflow_id: String,
    pub lane: String,
    pub source: String,
    #[serde(default)]
    #[cfg_attr(feature = "ts-export", ts(optional))]
    pub target_node_id: Option<String>,
    pub trace_id: String,
    pub attempts: u32,
    pub reason: String,
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
    pub payload: serde_json::Value,
}

// ── 可观测性 IPC 类型（从 src-tauri/src/observability.rs 迁入） ────

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

// ── 串口 IPC 类型（从 src-tauri/src/commands/serial.rs 迁入） ──────

/// 串口设备信息（`list_serial_ports`）。
#[derive(Debug, Clone, Serialize)]
#[cfg_attr(feature = "ts-export", derive(TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct SerialPortInfo {
    pub path: String,
    pub port_type: String,
    pub description: String,
}

/// 串口连接测试结果（`test_serial_connection`）。
#[derive(Debug, Clone, Serialize)]
#[cfg_attr(feature = "ts-export", derive(TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct TestSerialResult {
    pub ok: bool,
    pub message: String,
}

// ── 部署会话 IPC 类型（从 src-tauri/src/commands/deployment_session.rs 迁入） ──

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
    pub runtime_connections: Vec<ConnectionDefinition>,
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

/// 连接定义加载结果（`load_connection_definitions`）。
#[derive(Debug, Clone, Serialize)]
#[cfg_attr(feature = "ts-export", derive(TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct ConnectionDefinitionsLoadResult {
    pub definitions: Vec<ConnectionDefinition>,
    pub file_exists: bool,
}

/// 触发本 crate 与所有依赖 crate 的 ts-rs 导出。
///
/// 集中入口避免新增类型时漏导出；CI 通过 `git diff --exit-code -- web/src/generated/`
/// 兜底，开发者改了 Rust 类型却忘了 regenerate 会立刻失败。
#[cfg(feature = "ts-export")]
pub fn export_all() -> Result<(), ts_rs::ExportError> {
    nazh_core::export_bindings::export_all()?;
    connections::export_bindings::export_all()?;
    ai::export_bindings::export_all()?;
    nazh_engine::export_bindings::export_all()?;

    let cfg = Config::from_env();

    DeployResponse::export(&cfg)?;
    DispatchResponse::export(&cfg)?;
    UndeployResponse::export(&cfg)?;
    NodeTypeEntry::export(&cfg)?;
    ListNodeTypesResponse::export(&cfg)?;
    DescribeNodePinsRequest::export(&cfg)?;
    DescribeNodePinsResponse::export(&cfg)?;
    SnapshotWorkflowVariablesRequest::export(&cfg)?;
    SnapshotWorkflowVariablesResponse::export(&cfg)?;
    SetWorkflowVariableRequest::export(&cfg)?;
    SetWorkflowVariableResponse::export(&cfg)?;
    VariableChangedPayload::export(&cfg)?;
    VariableDeletedPayload::export(&cfg)?;
    DeleteWorkflowVariableRequest::export(&cfg)?;
    DeleteWorkflowVariableResponse::export(&cfg)?;
    ResetWorkflowVariableRequest::export(&cfg)?;
    ResetWorkflowVariableResponse::export(&cfg)?;
    QueryVariableHistoryRequest::export(&cfg)?;
    QueryVariableHistoryResponse::export(&cfg)?;
    HistoryEntryPayload::export(&cfg)?;
    SetGlobalVariableRequest::export(&cfg)?;
    SetGlobalVariableResponse::export(&cfg)?;
    GlobalVariableSnapshot::export(&cfg)?;
    GetGlobalVariableRequest::export(&cfg)?;
    GetGlobalVariableResponse::export(&cfg)?;
    ListGlobalVariablesRequest::export(&cfg)?;
    ListGlobalVariablesResponse::export(&cfg)?;
    DeleteGlobalVariableRequest::export(&cfg)?;
    ReactiveUpdatePayload::export(&cfg)?;

    // 运行时类型（从 src-tauri 迁入）
    RuntimeBackpressureStrategy::export(&cfg)?;
    WorkflowRuntimePolicy::export(&cfg)?;
    WorkflowRuntimePolicyInput::export(&cfg)?;
    DispatchLaneSnapshot::export(&cfg)?;
    RuntimeWorkflowSummary::export(&cfg)?;
    DeadLetterRecord::export(&cfg)?;

    // 可观测性类型（从 src-tauri 迁入）
    ObservabilityContextInput::export(&cfg)?;
    ObservabilityEntry::export(&cfg)?;
    AlertDeliveryRecord::export(&cfg)?;
    ObservabilityTraceSummary::export(&cfg)?;
    ObservabilityQueryResult::export(&cfg)?;

    // 串口类型（从 src-tauri 迁入）
    SerialPortInfo::export(&cfg)?;
    TestSerialResult::export(&cfg)?;

    // 部署会话类型（从 src-tauri 迁入）
    PersistedDeploymentSession::export(&cfg)?;
    PersistedDeploymentSessionCollection::export(&cfg)?;
    PersistedDeploymentSessionState::export(&cfg)?;

    // 连接类型（从 src-tauri 迁入）
    ConnectionDefinitionsLoadResult::export(&cfg)?;

    trim_typescript_trailing_whitespace(cfg.out_dir())?;
    Ok(())
}

#[cfg(feature = "ts-export")]
fn trim_typescript_trailing_whitespace(dir: &std::path::Path) -> Result<(), ts_rs::ExportError> {
    if !dir.exists() {
        return Ok(());
    }

    for entry in std::fs::read_dir(dir)? {
        let path = entry?.path();
        if path.is_dir() {
            trim_typescript_trailing_whitespace(&path)?;
            continue;
        }

        if path.extension().and_then(|value| value.to_str()) != Some("ts") {
            continue;
        }

        let source = std::fs::read_to_string(&path)?;
        let mut trimmed = source
            .lines()
            .map(str::trim_end)
            .collect::<Vec<_>>()
            .join("\n");
        if source.ends_with('\n') {
            trimmed.push('\n');
        }

        if trimmed != source {
            std::fs::write(path, trimmed)?;
        }
    }

    Ok(())
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use nazh_core::{
        EngineError, NodeCapabilities, NodeTrait, SharedResources, WorkflowNodeDefinition,
    };
    use std::sync::Arc;

    fn stub_factory(
        _def: &WorkflowNodeDefinition,
        _res: SharedResources,
    ) -> Result<Arc<dyn NodeTrait>, EngineError> {
        Err(EngineError::unsupported_node_type("test-stub"))
    }

    #[test]
    fn list_node_types_response_排序后输出全部类型() {
        let mut registry = NodeRegistry::new();
        registry.register_with_capabilities("timer", NodeCapabilities::empty(), stub_factory);
        registry.register_with_capabilities("code", NodeCapabilities::empty(), stub_factory);
        registry.register_with_capabilities("native", NodeCapabilities::empty(), stub_factory);

        let response = list_node_types_response(&registry);
        assert_eq!(response.types.len(), 3);
        assert_eq!(response.types[0].name, "code");
        assert_eq!(response.types[1].name, "native");
        assert_eq!(response.types[2].name, "timer");
    }

    #[test]
    fn list_node_types_response_空注册表返回空列表() {
        let registry = NodeRegistry::new();
        let response = list_node_types_response(&registry);
        assert!(response.types.is_empty());
    }

    #[test]
    fn list_node_types_response_透传能力标签位图() {
        let mut registry = NodeRegistry::new();
        registry.register_with_capabilities("timer", NodeCapabilities::TRIGGER, stub_factory);
        registry.register_with_capabilities(
            "modbusRead",
            NodeCapabilities::DEVICE_IO,
            stub_factory,
        );
        registry.register_with_capabilities("plain", NodeCapabilities::empty(), stub_factory);

        let response = list_node_types_response(&registry);
        let by_name: std::collections::HashMap<&str, u32> = response
            .types
            .iter()
            .map(|entry| (entry.name.as_str(), entry.capabilities))
            .collect();

        assert_eq!(by_name["timer"], NodeCapabilities::TRIGGER.bits());
        assert_eq!(by_name["modbusRead"], NodeCapabilities::DEVICE_IO.bits());
        assert_eq!(by_name["plain"], 0);
    }

    #[cfg(feature = "ts-export")]
    #[test]
    fn export_bindings() {
        super::export_all().unwrap();
    }
}
