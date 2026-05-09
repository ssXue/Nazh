use serde::{Deserialize, Serialize};

#[cfg(feature = "ts-export")]
use ts_rs::TS;

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
    #[cfg_attr(feature = "ts-export", ts(optional))]
    pub manual_queue_capacity: Option<usize>,
    #[serde(default)]
    #[cfg_attr(feature = "ts-export", ts(optional))]
    pub trigger_queue_capacity: Option<usize>,
    #[serde(default)]
    #[cfg_attr(feature = "ts-export", ts(optional))]
    pub manual_backpressure_strategy: Option<RuntimeBackpressureStrategy>,
    #[serde(default)]
    #[cfg_attr(feature = "ts-export", ts(optional))]
    pub trigger_backpressure_strategy: Option<RuntimeBackpressureStrategy>,
    #[serde(default)]
    #[cfg_attr(feature = "ts-export", ts(optional))]
    pub max_retry_attempts: Option<u32>,
    #[serde(default)]
    #[cfg_attr(feature = "ts-export", ts(optional, type = "number"))]
    pub initial_retry_backoff_ms: Option<u64>,
    #[serde(default)]
    #[cfg_attr(feature = "ts-export", ts(optional, type = "number"))]
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
