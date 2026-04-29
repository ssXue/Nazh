use std::{
    io::Write,
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicU64, AtomicUsize, Ordering},
    },
    time::Duration,
};

use nazh_engine::{WorkflowContext, WorkflowIngress};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio::sync::mpsc;

use crate::observability::SharedObservabilityStore;

pub(crate) const DEAD_LETTER_DIR: &str = "runtime";
pub(crate) const DEAD_LETTER_FILE: &str = "dead-letters.jsonl";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
#[derive(Default)]
pub(crate) enum RuntimeBackpressureStrategy {
    #[default]
    Block,
    RejectNewest,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub(crate) struct WorkflowRuntimePolicyInput {
    #[serde(default)]
    pub(crate) manual_queue_capacity: Option<usize>,
    #[serde(default)]
    pub(crate) trigger_queue_capacity: Option<usize>,
    #[serde(default)]
    pub(crate) manual_backpressure_strategy: Option<RuntimeBackpressureStrategy>,
    #[serde(default)]
    pub(crate) trigger_backpressure_strategy: Option<RuntimeBackpressureStrategy>,
    #[serde(default)]
    pub(crate) max_retry_attempts: Option<u32>,
    #[serde(default)]
    pub(crate) initial_retry_backoff_ms: Option<u64>,
    #[serde(default)]
    pub(crate) max_retry_backoff_ms: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct WorkflowRuntimePolicy {
    pub(crate) manual_queue_capacity: usize,
    pub(crate) trigger_queue_capacity: usize,
    pub(crate) manual_backpressure_strategy: RuntimeBackpressureStrategy,
    pub(crate) trigger_backpressure_strategy: RuntimeBackpressureStrategy,
    pub(crate) max_retry_attempts: u32,
    pub(crate) initial_retry_backoff_ms: u64,
    pub(crate) max_retry_backoff_ms: u64,
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

impl WorkflowRuntimePolicy {
    pub(crate) fn from_input(input: Option<WorkflowRuntimePolicyInput>) -> Self {
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

#[derive(Debug, Default)]
struct DispatchLaneMetrics {
    depth: AtomicUsize,
    accepted: AtomicU64,
    retried: AtomicU64,
    dead_lettered: AtomicU64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct DispatchLaneSnapshot {
    depth: usize,
    accepted: u64,
    retried: u64,
    dead_lettered: u64,
}

impl DispatchLaneMetrics {
    fn snapshot(&self) -> DispatchLaneSnapshot {
        DispatchLaneSnapshot {
            depth: self.depth.load(Ordering::Relaxed),
            accepted: self.accepted.load(Ordering::Relaxed),
            retried: self.retried.load(Ordering::Relaxed),
            dead_lettered: self.dead_lettered.load(Ordering::Relaxed),
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) struct RuntimeWorkflowMetadata {
    pub(crate) workflow_id: String,
    pub(crate) project_id: Option<String>,
    pub(crate) project_name: Option<String>,
    pub(crate) environment_id: Option<String>,
    pub(crate) environment_name: Option<String>,
    pub(crate) deployed_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct RuntimeWorkflowSummary {
    pub(crate) workflow_id: String,
    #[serde(default)]
    pub(crate) project_id: Option<String>,
    #[serde(default)]
    pub(crate) project_name: Option<String>,
    #[serde(default)]
    pub(crate) environment_id: Option<String>,
    #[serde(default)]
    pub(crate) environment_name: Option<String>,
    pub(crate) deployed_at: String,
    pub(crate) node_count: usize,
    pub(crate) edge_count: usize,
    pub(crate) root_nodes: Vec<String>,
    pub(crate) active: bool,
    pub(crate) policy: WorkflowRuntimePolicy,
    pub(crate) manual_lane: DispatchLaneSnapshot,
    pub(crate) trigger_lane: DispatchLaneSnapshot,
}

#[derive(Debug, Clone)]
enum DispatchLane {
    Manual,
    Trigger,
}

impl DispatchLane {
    fn label(&self) -> &'static str {
        match self {
            Self::Manual => "manual",
            Self::Trigger => "trigger",
        }
    }
}

#[derive(Debug, Clone)]
struct DispatchEnvelope {
    ctx: WorkflowContext,
    lane: DispatchLane,
    source: String,
    target_node_id: Option<String>,
    attempts: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct DeadLetterRecord {
    pub(crate) id: String,
    pub(crate) timestamp: String,
    pub(crate) workflow_id: String,
    pub(crate) lane: String,
    pub(crate) source: String,
    #[serde(default)]
    pub(crate) target_node_id: Option<String>,
    pub(crate) trace_id: String,
    pub(crate) attempts: u32,
    pub(crate) reason: String,
    #[serde(default)]
    pub(crate) project_id: Option<String>,
    #[serde(default)]
    pub(crate) project_name: Option<String>,
    #[serde(default)]
    pub(crate) environment_id: Option<String>,
    #[serde(default)]
    pub(crate) environment_name: Option<String>,
    pub(crate) payload: Value,
}

#[derive(Debug)]
pub(crate) struct DeadLetterSink {
    file_path: PathBuf,
    metadata: RuntimeWorkflowMetadata,
}

impl DeadLetterSink {
    pub(crate) async fn new(
        workspace_dir: PathBuf,
        metadata: RuntimeWorkflowMetadata,
    ) -> Result<Arc<Self>, String> {
        let root_dir = workspace_dir.join(DEAD_LETTER_DIR);
        tokio::task::spawn_blocking({
            let root_dir = root_dir.clone();
            move || std::fs::create_dir_all(&root_dir)
        })
        .await
        .map_err(|error| format!("创建运行时目录失败: {error}"))?
        .map_err(|error| format!("创建运行时目录失败: {error}"))?;

        Ok(Arc::new(Self {
            file_path: root_dir.join(DEAD_LETTER_FILE),
            metadata,
        }))
    }

    async fn record(
        &self,
        envelope: &DispatchEnvelope,
        reason: impl Into<String>,
        observability: Option<&SharedObservabilityStore>,
    ) -> Result<(), String> {
        let reason = reason.into();
        let timestamp = chrono::Utc::now().to_rfc3339();
        let record = DeadLetterRecord {
            id: format!(
                "{}-{}-{}",
                self.metadata.workflow_id,
                envelope.ctx.trace_id,
                chrono::Utc::now().timestamp_millis()
            ),
            timestamp: timestamp.clone(),
            workflow_id: self.metadata.workflow_id.clone(),
            lane: envelope.lane.label().to_owned(),
            source: envelope.source.clone(),
            target_node_id: envelope.target_node_id.clone(),
            trace_id: envelope.ctx.trace_id.to_string(),
            attempts: envelope.attempts,
            reason: reason.clone(),
            project_id: self.metadata.project_id.clone(),
            project_name: self.metadata.project_name.clone(),
            environment_id: self.metadata.environment_id.clone(),
            environment_name: self.metadata.environment_name.clone(),
            payload: envelope.ctx.payload.clone(),
        };

        append_json_line_async(self.file_path.clone(), record.clone()).await?;

        if let Some(store) = observability {
            let _ = store
                .record_audit(
                    "error",
                    "runtime",
                    "消息进入死信队列",
                    Some(reason),
                    Some(envelope.ctx.trace_id.to_string()),
                    Some(serde_json::to_value(record).unwrap_or(Value::Null)),
                )
                .await;
        }

        Ok(())
    }
}

#[derive(Clone)]
pub(crate) struct WorkflowDispatchRouter {
    workflow_id: String,
    policy: WorkflowRuntimePolicy,
    observability: Option<SharedObservabilityStore>,
    dead_letters: Arc<DeadLetterSink>,
    manual_tx: mpsc::Sender<DispatchEnvelope>,
    trigger_tx: mpsc::Sender<DispatchEnvelope>,
    manual_metrics: Arc<DispatchLaneMetrics>,
    trigger_metrics: Arc<DispatchLaneMetrics>,
}

impl WorkflowDispatchRouter {
    pub(crate) async fn submit_manual(
        &self,
        ctx: WorkflowContext,
        source: impl Into<String>,
    ) -> Result<(), String> {
        let envelope = DispatchEnvelope {
            ctx,
            lane: DispatchLane::Manual,
            source: source.into(),
            target_node_id: None,
            attempts: 0,
        };
        self.enqueue_async(
            &self.manual_tx,
            &self.manual_metrics,
            self.policy.manual_backpressure_strategy.clone(),
            envelope,
        )
        .await
    }

    // 当前无消费者——所有触发器节点（timer / serial / mqttClient subscribe）
    // 通过 NodeHandle::emit 直接进 DAG。trigger lane 基础设施保留，未来引擎级
    // 背压能力（参见 ADR-0014 / ADR-0016 草案）落地时复用此入口。
    #[allow(dead_code)]
    async fn submit_trigger_to(
        &self,
        node_id: &str,
        ctx: WorkflowContext,
        source: impl Into<String>,
    ) -> Result<(), String> {
        let envelope = DispatchEnvelope {
            ctx,
            lane: DispatchLane::Trigger,
            source: source.into(),
            target_node_id: Some(node_id.to_owned()),
            attempts: 0,
        };
        self.enqueue_async(
            &self.trigger_tx,
            &self.trigger_metrics,
            self.policy.trigger_backpressure_strategy.clone(),
            envelope,
        )
        .await
    }

    fn manual_snapshot(&self) -> DispatchLaneSnapshot {
        self.manual_metrics.snapshot()
    }

    fn trigger_snapshot(&self) -> DispatchLaneSnapshot {
        self.trigger_metrics.snapshot()
    }

    async fn enqueue_async(
        &self,
        tx: &mpsc::Sender<DispatchEnvelope>,
        metrics: &Arc<DispatchLaneMetrics>,
        strategy: RuntimeBackpressureStrategy,
        envelope: DispatchEnvelope,
    ) -> Result<(), String> {
        match strategy {
            RuntimeBackpressureStrategy::Block => {
                metrics.depth.fetch_add(1, Ordering::Relaxed);
                match tx.send(envelope).await {
                    Ok(()) => {
                        metrics.accepted.fetch_add(1, Ordering::Relaxed);
                        Ok(())
                    }
                    Err(error) => {
                        metrics.depth.fetch_sub(1, Ordering::Relaxed);
                        Err(format!(
                            "工作流 `{}` 的 {} 调度通道已关闭",
                            self.workflow_id,
                            error.0.lane.label()
                        ))
                    }
                }
            }
            RuntimeBackpressureStrategy::RejectNewest => match tx.try_send(envelope) {
                Ok(()) => {
                    metrics.depth.fetch_add(1, Ordering::Relaxed);
                    metrics.accepted.fetch_add(1, Ordering::Relaxed);
                    Ok(())
                }
                Err(mpsc::error::TrySendError::Full(envelope)) => {
                    metrics.dead_lettered.fetch_add(1, Ordering::Relaxed);
                    self.dead_letters
                        .record(
                            &envelope,
                            format!(
                                "工作流 `{}` 的 {} 调度队列已满，触发背压拒绝",
                                self.workflow_id,
                                envelope.lane.label()
                            ),
                            self.observability.as_ref(),
                        )
                        .await?;
                    Err(format!(
                        "工作流 `{}` 的 {} 调度队列已满，请稍后重试",
                        self.workflow_id,
                        envelope.lane.label()
                    ))
                }
                Err(mpsc::error::TrySendError::Closed(envelope)) => Err(format!(
                    "工作流 `{}` 的 {} 调度通道已关闭",
                    self.workflow_id,
                    envelope.lane.label()
                )),
            },
        }
    }
}

/// 已部署工作流的运行时包装，含入口句柄、引擎 lifecycle guards 与撤销 token。
///
/// 触发器节点的后台任务由引擎层 `LifecycleGuard` 管理；壳层只负责撤销编排——
/// cancel `shutdown_token` 广播取消信号，再按逆部署序串行
/// `guard.shutdown().await` 等待 cleanup 完成。
pub(crate) struct DesktopWorkflow {
    pub(crate) workflow_id: String,
    pub(crate) metadata: RuntimeWorkflowMetadata,
    pub(crate) policy: WorkflowRuntimePolicy,
    pub(crate) dispatch_router: WorkflowDispatchRouter,
    pub(crate) observability: Option<SharedObservabilityStore>,
    pub(crate) node_count: usize,
    pub(crate) edge_count: usize,
    pub(crate) root_nodes: Vec<String>,
    /// 引擎 lifecycle guards（按部署顺序）。撤销时按逆序 await shutdown。
    pub(crate) lifecycle_guards: Vec<(String, nazh_engine::LifecycleGuard)>,
    /// 撤销根 token——cancel 后所有 guard 内部派生的 child token 同时收到信号。
    pub(crate) shutdown_token: nazh_engine::CancellationToken,
    /// 部署时注入的共享资源句柄（含 `WorkflowVariables`），供 IPC 读取运行时状态。
    pub(crate) shared_resources: nazh_engine::SharedResources,
    /// 事件/结果转发任务。
    pub(crate) runtime_tasks: Vec<tauri::async_runtime::JoinHandle<()>>,
}

impl DesktopWorkflow {
    /// 撤销整个运行时：中止事件转发任务 + 广播 cancel + 串行 shutdown 所有 guards。
    ///
    /// 返回 shutdown 的 lifecycle guards 数量——通过 `UndeployResponse`
    /// 的 `aborted_timer_count` 字段透传。字段名沿用历史命名（语义为"已撤销
    /// 的触发器节点数"），改名会破坏 IPC 契约且需同步前端。
    pub(crate) async fn shutdown_runtime(&mut self) -> usize {
        for task in &self.runtime_tasks {
            task.abort();
        }
        let guards = std::mem::take(&mut self.lifecycle_guards);
        let count = guards.len();
        // 广播 cancel 让所有节点同时进入清理；再按逆部署序串行 await，
        // 给每个节点完整的 cleanup 窗口而不阻塞其他节点的取消信号。
        self.shutdown_token.cancel();
        for (_, guard) in guards.into_iter().rev() {
            guard.shutdown().await;
        }
        count
    }

    pub(crate) fn summary(&self, active: bool) -> RuntimeWorkflowSummary {
        RuntimeWorkflowSummary {
            workflow_id: self.workflow_id.clone(),
            project_id: self.metadata.project_id.clone(),
            project_name: self.metadata.project_name.clone(),
            environment_id: self.metadata.environment_id.clone(),
            environment_name: self.metadata.environment_name.clone(),
            deployed_at: self.metadata.deployed_at.clone(),
            node_count: self.node_count,
            edge_count: self.edge_count,
            root_nodes: self.root_nodes.clone(),
            active,
            policy: self.policy.clone(),
            manual_lane: self.dispatch_router.manual_snapshot(),
            trigger_lane: self.dispatch_router.trigger_snapshot(),
        }
    }
}

fn normalize_queue_capacity(value: usize) -> usize {
    value.clamp(1, 4_096)
}

async fn append_json_line_async<T>(path: PathBuf, value: T) -> Result<(), String>
where
    T: Serialize + Send + 'static,
{
    tokio::task::spawn_blocking(move || append_json_line(&path, &value))
        .await
        .map_err(|error| format!("写入 JSONL 失败: {error}"))?
}

fn append_json_line<T>(path: &Path, value: &T) -> Result<(), String>
where
    T: Serialize,
{
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .map_err(|error| format!("打开 `{}` 失败: {error}", path.display()))?;
    let line =
        serde_json::to_string(value).map_err(|error| format!("序列化 JSONL 失败: {error}"))?;
    writeln!(file, "{line}").map_err(|error| format!("写入 `{}` 失败: {error}", path.display()))
}

fn decrement_queue_depth(counter: &AtomicUsize) {
    let mut current = counter.load(Ordering::Relaxed);
    while current > 0 {
        match counter.compare_exchange_weak(
            current,
            current - 1,
            Ordering::Relaxed,
            Ordering::Relaxed,
        ) {
            Ok(_) => return,
            Err(next) => current = next,
        }
    }
}

fn retry_backoff_ms(policy: &WorkflowRuntimePolicy, attempt: u32) -> u64 {
    let multiplier = 2_u64.saturating_pow(attempt.saturating_sub(1));
    policy
        .initial_retry_backoff_ms
        .saturating_mul(multiplier)
        .clamp(
            policy.initial_retry_backoff_ms,
            policy
                .max_retry_backoff_ms
                .max(policy.initial_retry_backoff_ms),
        )
}

pub(crate) fn create_dispatch_router(
    ingress: WorkflowIngress,
    workflow_id: String,
    policy: WorkflowRuntimePolicy,
    observability: Option<SharedObservabilityStore>,
    dead_letters: Arc<DeadLetterSink>,
) -> (
    WorkflowDispatchRouter,
    Vec<tauri::async_runtime::JoinHandle<()>>,
) {
    let manual_metrics = Arc::new(DispatchLaneMetrics::default());
    let trigger_metrics = Arc::new(DispatchLaneMetrics::default());
    let (manual_tx, manual_rx) = mpsc::channel(policy.manual_queue_capacity);
    let (trigger_tx, trigger_rx) = mpsc::channel(policy.trigger_queue_capacity);

    let router = WorkflowDispatchRouter {
        workflow_id,
        policy: policy.clone(),
        observability: observability.clone(),
        dead_letters: dead_letters.clone(),
        manual_tx,
        trigger_tx,
        manual_metrics: manual_metrics.clone(),
        trigger_metrics: trigger_metrics.clone(),
    };

    let tasks = vec![
        spawn_dispatch_lane_task(
            ingress.clone(),
            manual_rx,
            policy.clone(),
            observability.clone(),
            dead_letters.clone(),
            manual_metrics,
        ),
        spawn_dispatch_lane_task(
            ingress,
            trigger_rx,
            policy,
            observability,
            dead_letters,
            trigger_metrics,
        ),
    ];

    (router, tasks)
}

fn spawn_dispatch_lane_task(
    ingress: WorkflowIngress,
    mut rx: mpsc::Receiver<DispatchEnvelope>,
    policy: WorkflowRuntimePolicy,
    observability: Option<SharedObservabilityStore>,
    dead_letters: Arc<DeadLetterSink>,
    metrics: Arc<DispatchLaneMetrics>,
) -> tauri::async_runtime::JoinHandle<()> {
    tauri::async_runtime::spawn(async move {
        while let Some(mut envelope) = rx.recv().await {
            decrement_queue_depth(&metrics.depth);

            loop {
                let delivery_result = match envelope.target_node_id.as_deref() {
                    Some(node_id) => ingress.submit_to(node_id, envelope.ctx.clone()).await,
                    None => ingress.submit(envelope.ctx.clone()).await,
                };

                match delivery_result {
                    Ok(()) => break,
                    Err(_error) if envelope.attempts < policy.max_retry_attempts => {
                        envelope.attempts += 1;
                        metrics.retried.fetch_add(1, Ordering::Relaxed);
                        tokio::time::sleep(Duration::from_millis(retry_backoff_ms(
                            &policy,
                            envelope.attempts,
                        )))
                        .await;
                    }
                    Err(error) => {
                        metrics.dead_lettered.fetch_add(1, Ordering::Relaxed);
                        let _ = dead_letters
                            .record(&envelope, error.to_string(), observability.as_ref())
                            .await;
                        break;
                    }
                }
            }
        }
    })
}
