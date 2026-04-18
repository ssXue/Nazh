//! Nazh 的 Tauri 桌面壳层。
//!
//! 向 React 前端暴露四个 IPC 命令：
//! - [`deploy_workflow`] — 解析并部署工作流 DAG。
//! - [`dispatch_payload`] — 向运行中的工作流提交测试载荷。
//! - [`undeploy_workflow`] — 停止当前工作流并中止定时任务。
//! - [`list_connections`] — 获取连接池快照。
//!
//! 引擎事件通过 `Window::emit` 转发给前端。

mod observability;

use nazh_ai_core::{
    AiCompletionRequest, AiCompletionResponse, AiConfigFile, AiConfigUpdate, AiConfigView,
    AiProviderDraft, AiService, AiTestResult, OpenAiCompatibleService,
};
use nazh_engine::{
    ConnectionDefinition, ConnectionRecord, DeployResponse, DispatchResponse, EngineError,
    ExecutionEvent, ListNodeTypesResponse, SerialTriggerNodeConfig, TimerNodeConfig,
    UndeployResponse, WorkflowContext, WorkflowGraph, WorkflowIngress,
    deploy_workflow_with_ai as deploy_workflow_graph, shared_connection_manager, standard_registry,
};
use observability::{
    ObservabilityContextInput, ObservabilityQueryResult, ObservabilityStore,
    SharedObservabilityStore, query_observability as query_workspace_observability,
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use tauri::{AppHandle, Emitter, Manager, State};
use tokio::fs;
use tokio::sync::{Mutex, RwLock, mpsc};

use std::{
    collections::HashMap,
    io::{Read, Write},
    path::{Component, Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering},
    },
    time::{Duration, Instant},
};

/// IPC 命令输入的最大允许字节数（10 MB）。
const MAX_IPC_INPUT_BYTES: usize = 10 * 1024 * 1024;
const DEAD_LETTER_DIR: &str = "runtime";
const DEAD_LETTER_FILE: &str = "dead-letters.jsonl";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
#[derive(Default)]
enum RuntimeBackpressureStrategy {
    #[default]
    Block,
    RejectNewest,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
struct WorkflowRuntimePolicyInput {
    #[serde(default)]
    manual_queue_capacity: Option<usize>,
    #[serde(default)]
    trigger_queue_capacity: Option<usize>,
    #[serde(default)]
    manual_backpressure_strategy: Option<RuntimeBackpressureStrategy>,
    #[serde(default)]
    trigger_backpressure_strategy: Option<RuntimeBackpressureStrategy>,
    #[serde(default)]
    max_retry_attempts: Option<u32>,
    #[serde(default)]
    initial_retry_backoff_ms: Option<u64>,
    #[serde(default)]
    max_retry_backoff_ms: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct WorkflowRuntimePolicy {
    manual_queue_capacity: usize,
    trigger_queue_capacity: usize,
    manual_backpressure_strategy: RuntimeBackpressureStrategy,
    trigger_backpressure_strategy: RuntimeBackpressureStrategy,
    max_retry_attempts: u32,
    initial_retry_backoff_ms: u64,
    max_retry_backoff_ms: u64,
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
    fn from_input(input: Option<WorkflowRuntimePolicyInput>) -> Self {
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
struct DispatchLaneSnapshot {
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
struct RuntimeWorkflowMetadata {
    workflow_id: String,
    project_id: Option<String>,
    project_name: Option<String>,
    environment_id: Option<String>,
    environment_name: Option<String>,
    deployed_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RuntimeWorkflowSummary {
    workflow_id: String,
    #[serde(default)]
    project_id: Option<String>,
    #[serde(default)]
    project_name: Option<String>,
    #[serde(default)]
    environment_id: Option<String>,
    #[serde(default)]
    environment_name: Option<String>,
    deployed_at: String,
    node_count: usize,
    edge_count: usize,
    root_nodes: Vec<String>,
    active: bool,
    policy: WorkflowRuntimePolicy,
    manual_lane: DispatchLaneSnapshot,
    trigger_lane: DispatchLaneSnapshot,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct ScopedExecutionEvent {
    workflow_id: String,
    event: ExecutionEvent,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct ScopedWorkflowResult {
    workflow_id: String,
    result: WorkflowContext,
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
struct DeadLetterRecord {
    id: String,
    timestamp: String,
    workflow_id: String,
    lane: String,
    source: String,
    #[serde(default)]
    target_node_id: Option<String>,
    trace_id: String,
    attempts: u32,
    reason: String,
    #[serde(default)]
    project_id: Option<String>,
    #[serde(default)]
    project_name: Option<String>,
    #[serde(default)]
    environment_id: Option<String>,
    #[serde(default)]
    environment_name: Option<String>,
    payload: Value,
}

#[derive(Debug)]
struct DeadLetterSink {
    file_path: PathBuf,
    metadata: RuntimeWorkflowMetadata,
}

impl DeadLetterSink {
    async fn new(
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
struct WorkflowDispatchRouter {
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
    async fn submit_manual(
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

    fn blocking_submit_trigger_to(
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
        self.enqueue_blocking(
            &self.trigger_tx,
            &self.trigger_metrics,
            self.policy.trigger_backpressure_strategy.clone(),
            envelope,
        )
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

    fn enqueue_blocking(
        &self,
        tx: &mpsc::Sender<DispatchEnvelope>,
        metrics: &Arc<DispatchLaneMetrics>,
        strategy: RuntimeBackpressureStrategy,
        envelope: DispatchEnvelope,
    ) -> Result<(), String> {
        match strategy {
            RuntimeBackpressureStrategy::Block => {
                metrics.depth.fetch_add(1, Ordering::Relaxed);
                match tx.blocking_send(envelope) {
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
                    let _ = tauri::async_runtime::block_on(self.dead_letters.record(
                        &envelope,
                        format!(
                            "工作流 `{}` 的 {} 调度队列已满，触发背压拒绝",
                            self.workflow_id,
                            envelope.lane.label()
                        ),
                        self.observability.as_ref(),
                    ));
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

/// 已部署工作流的运行时包装，包含入口句柄和根触发任务。
struct DesktopWorkflow {
    workflow_id: String,
    metadata: RuntimeWorkflowMetadata,
    policy: WorkflowRuntimePolicy,
    dispatch_router: WorkflowDispatchRouter,
    observability: Option<SharedObservabilityStore>,
    node_count: usize,
    edge_count: usize,
    root_nodes: Vec<String>,
    trigger_tasks: Vec<DesktopTriggerTask>,
    runtime_tasks: Vec<tauri::async_runtime::JoinHandle<()>>,
}

enum TriggerJoinHandle {
    Async(tauri::async_runtime::JoinHandle<()>),
    Thread(std::thread::JoinHandle<()>),
}

struct DesktopTriggerTask {
    cancel: Arc<AtomicBool>,
    join: TriggerJoinHandle,
}

impl DesktopWorkflow {
    /// 中止所有根触发任务和事件转发任务，返回中止的触发任务数量。
    async fn abort_triggers(&mut self) -> usize {
        for task in &self.runtime_tasks {
            task.abort();
        }

        let tasks = self.trigger_tasks.drain(..).collect::<Vec<_>>();
        let aborted = tasks.len();

        for task in &tasks {
            task.cancel.store(true, Ordering::Relaxed);
            if let TriggerJoinHandle::Async(handle) = &task.join {
                handle.abort();
            }
        }

        for task in tasks {
            match task.join {
                TriggerJoinHandle::Async(handle) => {
                    let _ = handle.await;
                }
                TriggerJoinHandle::Thread(handle) => {
                    let _ = handle.join();
                }
            }
        }

        aborted
    }

    fn summary(&self, active: bool) -> RuntimeWorkflowSummary {
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

/// Tauri 托管的应用状态，持有连接池和当前活跃的工作流。
struct DesktopState {
    connection_manager: nazh_engine::SharedConnectionManager,
    workflows: Mutex<HashMap<String, DesktopWorkflow>>,
    active_workflow_id: Mutex<Option<String>>,
    ai_config: Arc<RwLock<AiConfigFile>>,
    ai_service: Arc<dyn AiService>,
}

impl Default for DesktopState {
    fn default() -> Self {
        let ai_config = Arc::new(RwLock::new(AiConfigFile::default()));
        let ai_service = Arc::new(OpenAiCompatibleService::new(Arc::clone(&ai_config)));
        Self {
            connection_manager: shared_connection_manager(),
            workflows: Mutex::new(HashMap::new()),
            active_workflow_id: Mutex::new(None),
            ai_config,
            ai_service,
        }
    }
}

impl DesktopState {
    fn connections_file_path(
        app: &AppHandle,
        workspace_path: Option<&str>,
    ) -> Result<PathBuf, String> {
        let workspace_dir =
            resolve_project_workspace_dir(app, workspace_path).map(|(dir, _)| dir)?;
        Ok(workspace_dir.join("connections.json"))
    }

    fn deployment_session_file_path(
        app: &AppHandle,
        workspace_path: Option<&str>,
    ) -> Result<PathBuf, String> {
        let workspace_dir =
            resolve_project_workspace_dir(app, workspace_path).map(|(dir, _)| dir)?;
        Ok(workspace_dir.join("deployment-session.json"))
    }

    fn ai_config_file_path(app: &AppHandle) -> Result<PathBuf, String> {
        let data_dir = app
            .path()
            .app_local_data_dir()
            .map_err(|error| format!("无法解析应用数据目录: {error}"))?;
        Ok(data_dir.join("ai-config.json"))
    }

    async fn load_connections_from_disk(
        app: &AppHandle,
        manager: nazh_engine::SharedConnectionManager,
        workspace_path: Option<&str>,
    ) {
        match Self::connections_file_path(app, workspace_path) {
            Ok(path) => {
                if path.exists() {
                    if let Ok(text) = fs::read_to_string(&path).await {
                        if let Ok(defs) =
                            serde_json::from_str::<Vec<nazh_engine::ConnectionDefinition>>(&text)
                        {
                            manager.replace_connections(defs).await;
                        } else {
                            manager
                                .replace_connections(Vec::<ConnectionDefinition>::new())
                                .await;
                        }
                    } else {
                        manager
                            .replace_connections(Vec::<ConnectionDefinition>::new())
                            .await;
                    }
                } else {
                    manager
                        .replace_connections(Vec::<ConnectionDefinition>::new())
                        .await;
                }
            }
            Err(_) => {
                manager
                    .replace_connections(Vec::<ConnectionDefinition>::new())
                    .await;
            }
        }
    }

    async fn resolve_workflow_id(
        &self,
        requested_workflow_id: Option<&str>,
    ) -> Result<Option<String>, String> {
        if let Some(requested) = requested_workflow_id
            .map(str::trim)
            .filter(|value| !value.is_empty())
        {
            let workflows = self.workflows.lock().await;
            if workflows.contains_key(requested) {
                return Ok(Some(requested.to_owned()));
            }
            return Err(format!("运行中的工作流 `{requested}` 不存在"));
        }

        Ok(self.active_workflow_id.lock().await.clone())
    }

    async fn choose_fallback_active_workflow(&self) -> Option<String> {
        let workflows = self.workflows.lock().await;
        workflows
            .values()
            .max_by(|left, right| left.metadata.deployed_at.cmp(&right.metadata.deployed_at))
            .map(|workflow| workflow.workflow_id.clone())
    }
}

#[derive(Debug, Clone)]
struct TimerRootSpec {
    node_id: String,
    interval_ms: u64,
    immediate: bool,
}

#[derive(Debug, Clone)]
struct SerialRootSpec {
    node_id: String,
    connection_id: String,
    config: SerialTriggerNodeConfig,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct ProjectWorkspaceStorageInfo {
    workspace_path: String,
    library_file_path: String,
    using_default_location: bool,
    library_exists: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct ProjectWorkspaceLoadResult {
    storage: ProjectWorkspaceStorageInfo,
    library_text: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PersistedDeploymentSession {
    version: u8,
    project_id: String,
    project_name: String,
    environment_id: String,
    environment_name: String,
    deployed_at: String,
    runtime_ast_text: String,
    runtime_connections: Vec<ConnectionDefinition>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PersistedDeploymentSessionCollection {
    version: u8,
    #[serde(default)]
    active_project_id: Option<String>,
    sessions: Vec<PersistedDeploymentSession>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PersistedDeploymentSessionState {
    version: u8,
    #[serde(default)]
    active_project_id: Option<String>,
    sessions: Vec<PersistedDeploymentSession>,
}

fn sort_deployment_sessions_by_freshness(sessions: &mut Vec<PersistedDeploymentSession>) {
    sessions.sort_by(|left, right| right.deployed_at.cmp(&left.deployed_at));
}

fn normalize_deployment_sessions(
    sessions: Vec<PersistedDeploymentSession>,
) -> Vec<PersistedDeploymentSession> {
    let mut sessions = sessions;
    sort_deployment_sessions_by_freshness(&mut sessions);

    let mut seen = std::collections::HashSet::new();
    let mut normalized = Vec::new();
    for session in sessions {
        if seen.insert(session.project_id.clone()) {
            normalized.push(session);
        }
    }
    normalized
}

fn normalize_deployment_session_state(
    state: PersistedDeploymentSessionState,
) -> PersistedDeploymentSessionState {
    let sessions = normalize_deployment_sessions(state.sessions);
    let active_project_id = state
        .active_project_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .filter(|value| sessions.iter().any(|session| session.project_id == *value))
        .map(str::to_owned);

    PersistedDeploymentSessionState {
        version: 3,
        active_project_id,
        sessions,
    }
}

async fn read_deployment_sessions_from_path(
    path: &Path,
) -> Result<Vec<PersistedDeploymentSession>, String> {
    Ok(read_deployment_session_state_from_path(path)
        .await?
        .sessions)
}

async fn read_deployment_session_state_from_path(
    path: &Path,
) -> Result<PersistedDeploymentSessionState, String> {
    if !path.exists() {
        return Ok(PersistedDeploymentSessionState {
            version: 3,
            active_project_id: None,
            sessions: Vec::new(),
        });
    }

    let text = fs::read_to_string(path)
        .await
        .map_err(|error| format!("读取 deployment-session.json 失败: {error}"))?;
    let value = serde_json::from_str::<Value>(&text)
        .map_err(|error| format!("解析 deployment-session.json 失败: {error}"))?;

    if value
        .get("sessions")
        .is_some_and(serde_json::Value::is_array)
    {
        let collection = serde_json::from_value::<PersistedDeploymentSessionCollection>(value)
            .map_err(|error| format!("解析 deployment-session.json 失败: {error}"))?;
        return Ok(normalize_deployment_session_state(
            PersistedDeploymentSessionState {
                version: collection.version,
                active_project_id: collection.active_project_id,
                sessions: collection.sessions,
            },
        ));
    }

    let session = serde_json::from_value::<PersistedDeploymentSession>(value)
        .map_err(|error| format!("解析 deployment-session.json 失败: {error}"))?;
    Ok(normalize_deployment_session_state(
        PersistedDeploymentSessionState {
            version: 1,
            active_project_id: None,
            sessions: vec![session],
        },
    ))
}

async fn write_deployment_session_state_to_path(
    path: &Path,
    state: PersistedDeploymentSessionState,
) -> Result<(), String> {
    let normalized = normalize_deployment_session_state(state);
    let sessions = normalized.sessions.clone();

    if sessions.is_empty() {
        if path.exists() {
            fs::remove_file(path)
                .await
                .map_err(|error| format!("删除 deployment-session.json 失败: {error}"))?;
        }
        return Ok(());
    }

    let dir = path.parent().ok_or("无法确定部署会话文件目录")?;
    fs::create_dir_all(dir)
        .await
        .map_err(|error| format!("创建部署会话目录失败: {error}"))?;

    let payload = PersistedDeploymentSessionCollection {
        version: 3,
        active_project_id: normalized.active_project_id,
        sessions,
    };
    let text = serde_json::to_string_pretty(&payload)
        .map_err(|error| format!("序列化部署会话失败: {error}"))?;
    fs::write(path, text)
        .await
        .map_err(|error| format!("写入 deployment-session.json 失败: {error}"))?;
    Ok(())
}

#[tauri::command]
async fn deploy_workflow(
    app: AppHandle,
    state: State<'_, DesktopState>,
    ast: String,
    connection_definitions: Option<Vec<ConnectionDefinition>>,
    observability_context: Option<ObservabilityContextInput>,
    workflow_id: Option<String>,
    runtime_policy: Option<WorkflowRuntimePolicyInput>,
) -> Result<DeployResponse, String> {
    if ast.len() > MAX_IPC_INPUT_BYTES {
        return Err("AST 超过最大允许大小（10 MB）".to_owned());
    }
    let mut graph = WorkflowGraph::from_json(&ast).map_err(stringify_error)?;
    normalize_sql_writer_paths(&app, &mut graph).map_err(stringify_error)?;
    let workflow_id = derive_workflow_id(
        workflow_id.as_deref(),
        graph.name.as_deref(),
        observability_context.as_ref(),
    );
    let policy = WorkflowRuntimePolicy::from_input(runtime_policy);
    let deployed_at = chrono::Utc::now().to_rfc3339();
    let metadata = RuntimeWorkflowMetadata {
        workflow_id: workflow_id.clone(),
        project_id: observability_context
            .as_ref()
            .map(|context| context.project_id.clone()),
        project_name: observability_context
            .as_ref()
            .map(|context| context.project_name.clone()),
        environment_id: observability_context
            .as_ref()
            .map(|context| context.environment_id.clone()),
        environment_name: observability_context
            .as_ref()
            .map(|context| context.environment_name.clone()),
        deployed_at: deployed_at.clone(),
    };
    let (workspace_dir, _) = resolve_project_workspace_dir(
        &app,
        observability_context
            .as_ref()
            .map(|context| context.workspace_path.as_str()),
    )?;

    if let Some(definitions) = connection_definitions {
        if state.workflows.lock().await.is_empty() {
            state
                .connection_manager
                .replace_connections(definitions)
                .await;
        } else {
            state
                .connection_manager
                .upsert_connections(definitions)
                .await;
        }
    }
    let observability_store = if let Some(context) = observability_context.clone() {
        let store = ObservabilityStore::new(workspace_dir.clone(), context).await?;
        let _ = store
            .record_audit(
                "info",
                "workflow",
                "收到部署请求",
                Some(format!("workflow_id={workflow_id}")),
                None,
                Some(json!({
                    "workflow_id": workflow_id.clone(),
                    "project_name": metadata.project_name.clone(),
                    "environment_name": metadata.environment_name.clone(),
                })),
            )
            .await;
        Some(store)
    } else {
        None
    };
    let timer_roots = collect_timer_root_specs(&graph).map_err(stringify_error)?;
    let serial_roots = collect_serial_root_specs(&graph, state.connection_manager.clone())
        .await
        .map_err(stringify_error)?;
    let node_count = graph.nodes.len();
    let edge_count = graph.edges.len();
    let registry = standard_registry();
    let deployment = match deploy_workflow_graph(
        graph,
        state.connection_manager.clone(),
        Some(Arc::clone(&state.ai_service)),
        &registry,
    )
    .await
    {
        Ok(deployment) => deployment,
        Err(error) => {
            if let Some(store) = &observability_store {
                let _ = store
                    .record_audit(
                        "error",
                        "workflow",
                        "部署失败",
                        Some(error.to_string()),
                        None,
                        None,
                    )
                    .await;
            }
            return Err(stringify_error(error));
        }
    };
    let (ingress, streams) = deployment.into_parts();
    let root_nodes = ingress.root_nodes().to_vec();
    let (mut event_rx, mut result_rx, result_store_ref) = streams.into_receivers();
    let dead_letters = DeadLetterSink::new(workspace_dir, metadata.clone()).await?;
    let (dispatch_router, mut runtime_tasks) = create_dispatch_router(
        ingress.clone(),
        workflow_id.clone(),
        policy.clone(),
        observability_store.clone(),
        dead_letters,
    );

    let existing_workflow = {
        let mut workflows = state.workflows.lock().await;
        workflows.remove(&workflow_id)
    };
    let replaced_existing = existing_workflow.is_some();
    if let Some(mut existing) = existing_workflow {
        existing.abort_triggers().await;
    }

    let mut trigger_tasks = spawn_timer_root_tasks(dispatch_router.clone(), timer_roots);
    trigger_tasks.extend(spawn_serial_root_tasks(
        app.clone(),
        dispatch_router.clone(),
        state.connection_manager.clone(),
        observability_store.clone(),
        workflow_id.clone(),
        serial_roots,
    ));

    let event_app = app.clone();
    let event_store = observability_store.clone();
    let workflow_id_for_event = workflow_id.clone();
    runtime_tasks.push(tauri::async_runtime::spawn(async move {
        while let Some(event) = event_rx.recv().await {
            if let Some(store) = &event_store {
                let _ = store.record_execution_event(&event).await;
            }
            let _ = event_app.emit(
                "workflow://node-status-v2",
                ScopedExecutionEvent {
                    workflow_id: workflow_id_for_event.clone(),
                    event: event.clone(),
                },
            );
            if is_active_workflow(&event_app, &workflow_id_for_event).await {
                let _ = event_app.emit("workflow://node-status", event);
            }
        }
    }));

    let result_app = app.clone();
    let result_store = observability_store.clone();
    let workflow_id_for_result = workflow_id.clone();
    runtime_tasks.push(tauri::async_runtime::spawn(async move {
        while let Some(ctx_ref) = result_rx.recv().await {
            // 从 DataStore 重建 WorkflowContext
            let payload = match result_store_ref.read(&ctx_ref.data_id) {
                Ok(p) => p,
                Err(_) => continue,
            };
            result_store_ref.release(&ctx_ref.data_id);
            let result = nazh_engine::WorkflowContext::from_parts(
                ctx_ref.trace_id,
                ctx_ref.timestamp,
                (*payload).clone(),
            );
            if let Some(store) = &result_store {
                let _ = store.record_result(&result).await;
            }
            let _ = result_app.emit(
                "workflow://result-v2",
                ScopedWorkflowResult {
                    workflow_id: workflow_id_for_result.clone(),
                    result: result.clone(),
                },
            );
            if is_active_workflow(&result_app, &workflow_id_for_result).await {
                let _ = result_app.emit("workflow://result", result);
            }
        }
    }));

    {
        let mut workflows = state.workflows.lock().await;
        workflows.insert(
            workflow_id.clone(),
            DesktopWorkflow {
                workflow_id: workflow_id.clone(),
                metadata: metadata.clone(),
                policy: policy.clone(),
                dispatch_router: dispatch_router.clone(),
                observability: observability_store.clone(),
                node_count,
                edge_count,
                root_nodes: root_nodes.clone(),
                trigger_tasks,
                runtime_tasks,
            },
        );
    }
    {
        let mut active_workflow_id = state.active_workflow_id.lock().await;
        *active_workflow_id = Some(workflow_id.clone());
    }

    let deploy_payload = DeployResponse {
        node_count,
        edge_count,
        root_nodes,
        project_id: metadata.project_id.clone(),
        workflow_id: Some(workflow_id.clone()),
        replaced_existing: Some(replaced_existing),
    };
    if let Some(store) = &observability_store {
        let _ = store
            .record_audit(
                "success",
                "workflow",
                "部署完成",
                Some(format!(
                    "workflow_id={workflow_id} · 节点 {node_count} / 连线 {edge_count}"
                )),
                None,
                Some(json!({
                    "workflow_id": workflow_id.clone(),
                    "node_count": node_count,
                    "edge_count": edge_count,
                    "root_nodes": deploy_payload.root_nodes.clone(),
                    "replaced_existing": replaced_existing,
                    "runtime_policy": {
                        "manual_queue_capacity": policy.manual_queue_capacity,
                        "trigger_queue_capacity": policy.trigger_queue_capacity,
                        "manual_backpressure_strategy": policy.manual_backpressure_strategy,
                        "trigger_backpressure_strategy": policy.trigger_backpressure_strategy,
                        "max_retry_attempts": policy.max_retry_attempts,
                        "initial_retry_backoff_ms": policy.initial_retry_backoff_ms,
                        "max_retry_backoff_ms": policy.max_retry_backoff_ms,
                    }
                })),
            )
            .await;
    }
    let _ = app.emit("workflow://deployed", deploy_payload.clone());
    Ok(deploy_payload)
}

#[tauri::command]
async fn dispatch_payload(
    state: State<'_, DesktopState>,
    payload: Value,
    workflow_id: Option<String>,
) -> Result<DispatchResponse, String> {
    let target_workflow_id = state
        .resolve_workflow_id(workflow_id.as_deref())
        .await?
        .ok_or_else(|| stringify_error(EngineError::WorkflowUnavailable))?;
    let (dispatch_router, observability_store) = {
        let workflows = state.workflows.lock().await;
        let workflow = workflows
            .get(&target_workflow_id)
            .ok_or_else(|| stringify_error(EngineError::WorkflowUnavailable))?;
        (
            workflow.dispatch_router.clone(),
            workflow.observability.clone(),
        )
    };

    let ctx = WorkflowContext::new(payload);
    let trace_id = ctx.trace_id.to_string();
    tracing::info!(workflow_id = %target_workflow_id, trace_id = %trace_id, "收到测试载荷提交请求");
    if let Err(error) = dispatch_router.submit_manual(ctx, "manual-dispatch").await {
        if let Some(store) = &observability_store {
            let _ = store
                .record_audit(
                    "error",
                    "dispatch",
                    "提交测试载荷失败",
                    Some(error.clone()),
                    Some(trace_id.clone()),
                    Some(json!({
                        "workflow_id": target_workflow_id,
                    })),
                )
                .await;
        }
        return Err(error);
    }

    if let Some(store) = &observability_store {
        let _ = store
            .record_audit(
                "info",
                "dispatch",
                "已提交测试载荷",
                Some(format!(
                    "workflow_id={target_workflow_id} · trace_id={trace_id}"
                )),
                Some(trace_id.clone()),
                Some(json!({
                    "workflow_id": target_workflow_id,
                })),
            )
            .await;
    }
    Ok(DispatchResponse {
        trace_id,
        workflow_id: Some(target_workflow_id),
    })
}

#[tauri::command]
async fn undeploy_workflow(
    app: AppHandle,
    state: State<'_, DesktopState>,
    workflow_id: Option<String>,
) -> Result<UndeployResponse, String> {
    let target_workflow_id = state.resolve_workflow_id(workflow_id.as_deref()).await?;
    tracing::info!(workflow_id = ?target_workflow_id, "收到停止运行请求");
    let Some(target_workflow_id) = target_workflow_id else {
        let response = UndeployResponse {
            had_workflow: false,
            aborted_timer_count: 0,
            workflow_id: None,
        };
        let _ = app.emit("workflow://undeployed", response.clone());
        return Ok(response);
    };

    let active_before = state.active_workflow_id.lock().await.clone();
    let (existing_workflow, removed_observability) = {
        let mut workflows = state.workflows.lock().await;
        let removed = workflows.remove(&target_workflow_id);
        let observability = removed
            .as_ref()
            .and_then(|workflow| workflow.observability.clone());
        (removed, observability)
    };

    let response = if let Some(mut workflow) = existing_workflow {
        UndeployResponse {
            had_workflow: true,
            aborted_timer_count: workflow.abort_triggers().await,
            workflow_id: Some(target_workflow_id.clone()),
        }
    } else {
        UndeployResponse {
            had_workflow: false,
            aborted_timer_count: 0,
            workflow_id: Some(target_workflow_id.clone()),
        }
    };

    let remaining_workflow_count = state.workflows.lock().await.len();
    if remaining_workflow_count == 0 {
        state
            .connection_manager
            .mark_all_idle("运行已停止，连接会话已回收到空闲态")
            .await;
    }

    let mut fallback_summary = None;
    if active_before.as_deref() == Some(target_workflow_id.as_str()) {
        let fallback_active = state.choose_fallback_active_workflow().await;
        let mut active_workflow_id = state.active_workflow_id.lock().await;
        *active_workflow_id = fallback_active.clone();
        drop(active_workflow_id);

        if let Some(fallback_workflow_id) = fallback_active {
            let workflows = state.workflows.lock().await;
            fallback_summary = workflows
                .get(&fallback_workflow_id)
                .map(|workflow| workflow.summary(true));
        }
    }

    if let Some(store) = removed_observability {
        let _ = store
            .record_audit(
                if response.had_workflow {
                    "warn"
                } else {
                    "info"
                },
                "workflow",
                if response.had_workflow {
                    "运行已停止"
                } else {
                    "停止请求未命中已部署工作流"
                },
                Some(format!(
                    "workflow_id={} · 已中止 {} 个根触发任务",
                    target_workflow_id, response.aborted_timer_count
                )),
                None,
                Some(json!({
                    "workflow_id": target_workflow_id,
                    "remaining_workflow_count": remaining_workflow_count,
                })),
            )
            .await;
    }

    if active_before.as_deref() == Some(target_workflow_id.as_str()) {
        let _ = app.emit("workflow://undeployed", response.clone());
        if let Some(summary) = fallback_summary {
            let _ = app.emit("workflow://runtime-focus", summary);
        }
    }
    Ok(response)
}

#[tauri::command]
async fn list_connections(state: State<'_, DesktopState>) -> Result<Vec<ConnectionRecord>, String> {
    let connections = state.connection_manager.list().await;
    Ok(connections)
}

#[tauri::command]
async fn list_node_types() -> Result<ListNodeTypesResponse, String> {
    let registry = standard_registry();
    Ok(ListNodeTypesResponse {
        types: registry.registered_types_with_aliases(),
    })
}

#[tauri::command]
async fn list_runtime_workflows(
    state: State<'_, DesktopState>,
) -> Result<Vec<RuntimeWorkflowSummary>, String> {
    let active_workflow_id = state.active_workflow_id.lock().await.clone();
    let workflows = state.workflows.lock().await;
    let mut summaries = workflows
        .values()
        .map(|workflow| {
            workflow.summary(active_workflow_id.as_deref() == Some(workflow.workflow_id.as_str()))
        })
        .collect::<Vec<_>>();
    summaries.sort_by(|left, right| right.deployed_at.cmp(&left.deployed_at));
    Ok(summaries)
}

#[tauri::command]
async fn set_active_runtime_workflow(
    app: AppHandle,
    state: State<'_, DesktopState>,
    workflow_id: String,
) -> Result<RuntimeWorkflowSummary, String> {
    let workflow_id = workflow_id.trim();
    if workflow_id.is_empty() {
        return Err("workflow_id 不能为空".to_owned());
    }

    let summary = {
        let workflows = state.workflows.lock().await;
        let workflow = workflows
            .get(workflow_id)
            .ok_or_else(|| format!("运行中的工作流 `{workflow_id}` 不存在"))?;
        workflow.summary(true)
    };

    {
        let mut active_workflow_id = state.active_workflow_id.lock().await;
        *active_workflow_id = Some(workflow_id.to_owned());
    }

    if let Some(workflow) = state.workflows.lock().await.get(workflow_id)
        && let Some(store) = &workflow.observability
    {
        let _ = store
            .record_audit(
                "info",
                "runtime",
                "已切换当前工作流",
                Some(format!("workflow_id={workflow_id}")),
                None,
                None,
            )
            .await;
    }

    let _ = app.emit("workflow://runtime-focus", summary.clone());
    Ok(summary)
}

#[tauri::command]
async fn list_dead_letters(
    app: AppHandle,
    workspace_path: Option<String>,
    workflow_id: Option<String>,
    limit: Option<usize>,
) -> Result<Vec<DeadLetterRecord>, String> {
    let (workspace_dir, _) = resolve_project_workspace_dir(&app, workspace_path.as_deref())?;
    let file_path = workspace_dir.join(DEAD_LETTER_DIR).join(DEAD_LETTER_FILE);
    if !file_path.exists() {
        return Ok(Vec::new());
    }

    let text = fs::read_to_string(&file_path)
        .await
        .map_err(|error| format!("读取 dead-letter 文件失败: {error}"))?;
    let workflow_filter = workflow_id
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty());
    let max_items = limit.unwrap_or(120).clamp(1, 1_000);
    let mut records = text
        .lines()
        .filter_map(|line| serde_json::from_str::<DeadLetterRecord>(line).ok())
        .filter(|record| {
            workflow_filter
                .as_ref()
                .is_none_or(|filter| record.workflow_id == *filter)
        })
        .collect::<Vec<_>>();

    records.sort_by(|left, right| right.timestamp.cmp(&left.timestamp));
    records.truncate(max_items);
    Ok(records)
}

#[tauri::command]
async fn query_observability(
    app: AppHandle,
    workspace_path: Option<String>,
    trace_id: Option<String>,
    search: Option<String>,
    limit: Option<usize>,
) -> Result<ObservabilityQueryResult, String> {
    let (workspace_dir, _) = resolve_project_workspace_dir(&app, workspace_path.as_deref())?;
    query_workspace_observability(workspace_dir, trace_id, search, limit.unwrap_or(240)).await
}

#[tauri::command]
async fn load_connection_definitions(
    app: AppHandle,
    state: State<'_, DesktopState>,
    workspace_path: Option<String>,
) -> Result<Vec<ConnectionDefinition>, String> {
    let path = DesktopState::connections_file_path(&app, workspace_path.as_deref())?;
    if !path.exists() {
        state
            .connection_manager
            .replace_connections(Vec::<ConnectionDefinition>::new())
            .await;
        return Ok(Vec::new());
    }
    let text = fs::read_to_string(&path)
        .await
        .map_err(|e| format!("读取 connections.json 失败: {e}"))?;
    let defs = serde_json::from_str::<Vec<ConnectionDefinition>>(&text)
        .map_err(|e| format!("解析 connections.json 失败: {e}"))?;
    state
        .connection_manager
        .replace_connections(defs.clone())
        .await;
    Ok(defs)
}

#[tauri::command]
async fn save_connection_definitions(
    app: AppHandle,
    state: State<'_, DesktopState>,
    workspace_path: Option<String>,
    definitions: Vec<ConnectionDefinition>,
) -> Result<(), String> {
    let path = DesktopState::connections_file_path(&app, workspace_path.as_deref())?;
    let dir = path.parent().ok_or("无法确定连接文件目录")?;
    fs::create_dir_all(dir)
        .await
        .map_err(|e| format!("创建连接文件目录失败: {e}"))?;
    let text = serde_json::to_string_pretty(&definitions)
        .map_err(|e| format!("序列化连接定义失败: {e}"))?;
    fs::write(&path, text)
        .await
        .map_err(|e| format!("写入 connections.json 失败: {e}"))?;
    state
        .connection_manager
        .replace_connections(definitions)
        .await;
    Ok(())
}

#[tauri::command]
async fn load_deployment_session_file(
    app: AppHandle,
    workspace_path: Option<String>,
) -> Result<Option<PersistedDeploymentSession>, String> {
    let path = DesktopState::deployment_session_file_path(&app, workspace_path.as_deref())?;
    Ok(read_deployment_sessions_from_path(&path)
        .await?
        .into_iter()
        .next())
}

#[tauri::command]
async fn load_deployment_session_state_file(
    app: AppHandle,
    workspace_path: Option<String>,
) -> Result<PersistedDeploymentSessionState, String> {
    let path = DesktopState::deployment_session_file_path(&app, workspace_path.as_deref())?;
    read_deployment_session_state_from_path(&path).await
}

#[tauri::command]
async fn list_deployment_sessions_file(
    app: AppHandle,
    workspace_path: Option<String>,
) -> Result<Vec<PersistedDeploymentSession>, String> {
    let path = DesktopState::deployment_session_file_path(&app, workspace_path.as_deref())?;
    read_deployment_sessions_from_path(&path).await
}

#[tauri::command]
async fn save_deployment_session_file(
    app: AppHandle,
    workspace_path: Option<String>,
    session: PersistedDeploymentSession,
    active_project_id: Option<String>,
) -> Result<(), String> {
    let path = DesktopState::deployment_session_file_path(&app, workspace_path.as_deref())?;
    let mut state = read_deployment_session_state_from_path(&path).await?;
    state
        .sessions
        .retain(|current| current.project_id != session.project_id);
    state.sessions.push(session);
    if let Some(active_project_id) = active_project_id {
        let trimmed = active_project_id.trim();
        state.active_project_id = if trimmed.is_empty() {
            None
        } else {
            Some(trimmed.to_owned())
        };
    }
    write_deployment_session_state_to_path(&path, state).await
}

#[tauri::command]
async fn set_deployment_session_active_project_file(
    app: AppHandle,
    workspace_path: Option<String>,
    project_id: Option<String>,
) -> Result<(), String> {
    let path = DesktopState::deployment_session_file_path(&app, workspace_path.as_deref())?;
    let mut state = read_deployment_session_state_from_path(&path).await?;
    state.active_project_id = project_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_owned);
    write_deployment_session_state_to_path(&path, state).await
}

#[tauri::command]
async fn remove_deployment_session_file(
    app: AppHandle,
    workspace_path: Option<String>,
    project_id: String,
) -> Result<(), String> {
    let path = DesktopState::deployment_session_file_path(&app, workspace_path.as_deref())?;
    let target_project_id = project_id.trim();
    if target_project_id.is_empty() {
        return Ok(());
    }

    let mut state = read_deployment_session_state_from_path(&path).await?;
    state
        .sessions
        .retain(|session| session.project_id != target_project_id);
    if state.active_project_id.as_deref() == Some(target_project_id) {
        state.active_project_id = None;
    }
    write_deployment_session_state_to_path(&path, state).await
}

#[tauri::command]
async fn clear_deployment_session_file(
    app: AppHandle,
    workspace_path: Option<String>,
) -> Result<(), String> {
    let path = DesktopState::deployment_session_file_path(&app, workspace_path.as_deref())?;

    if !path.exists() {
        return Ok(());
    }

    fs::remove_file(&path)
        .await
        .map_err(|error| format!("删除 deployment-session.json 失败: {error}"))?;
    Ok(())
}

#[tauri::command]
async fn load_ai_config(state: State<'_, DesktopState>) -> Result<AiConfigView, String> {
    let config = state.ai_config.read().await;
    Ok(config.to_view())
}

#[tauri::command]
async fn save_ai_config(
    app: AppHandle,
    state: State<'_, DesktopState>,
    update: AiConfigUpdate,
) -> Result<AiConfigView, String> {
    let path = DesktopState::ai_config_file_path(&app)?;
    let dir = path.parent().ok_or("无法确定 AI 配置文件目录")?;
    fs::create_dir_all(dir)
        .await
        .map_err(|error| format!("创建 AI 配置目录失败: {error}"))?;

    let mut config = state.ai_config.write().await;
    config.merge_update(update);

    let tmp_path = path.with_extension("json.tmp");
    let text = serde_json::to_string_pretty(&*config)
        .map_err(|error| format!("序列化 AI 配置失败: {error}"))?;
    fs::write(&tmp_path, &text)
        .await
        .map_err(|error| format!("写入 AI 配置临时文件失败: {error}"))?;
    fs::rename(&tmp_path, &path)
        .await
        .map_err(|error| format!("原子重命名 AI 配置文件失败: {error}"))?;

    Ok(config.to_view())
}

#[tauri::command]
async fn test_ai_provider(
    state: State<'_, DesktopState>,
    draft: AiProviderDraft,
) -> Result<AiTestResult, String> {
    state
        .ai_service
        .test_connection(draft)
        .await
        .map_err(|error| error.to_string())
}

#[tauri::command]
async fn copilot_complete(
    state: State<'_, DesktopState>,
    request: AiCompletionRequest,
) -> Result<AiCompletionResponse, String> {
    state
        .ai_service
        .complete(request)
        .await
        .map_err(|error| error.to_string())
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct SerialPortInfo {
    path: String,
    port_type: String,
    description: String,
}

#[tauri::command]
async fn list_serial_ports() -> Result<Vec<SerialPortInfo>, String> {
    let ports = serialport::available_ports().map_err(|e| format!("枚举串口失败: {e}"))?;

    let infos = ports
        .into_iter()
        .map(|port| {
            let path = port.port_name;
            let port_type = classify_serial_port(&path);
            let description = format!("{:?}", port.port_type);
            SerialPortInfo {
                path,
                port_type,
                description,
            }
        })
        .collect();

    Ok(infos)
}

fn classify_serial_port(path: &str) -> String {
    let path_lower = path.to_lowercase();
    if path_lower.contains("bluetooth") || path_lower.contains("bt-") {
        "bluetooth".to_string()
    } else if path_lower.contains("/dev/cu.") || path_lower.contains("/dev/tty.") {
        "usb-serial".to_string()
    } else if path_lower.contains("/dev/ttyusb")
        || path_lower.contains("/dev/ttyacm")
        || path_lower.contains("/dev/ttyama")
    {
        "usb-serial".to_string()
    } else {
        "builtin".to_string()
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct TestSerialResult {
    ok: bool,
    message: String,
}

#[tauri::command]
async fn test_serial_connection(
    port_path: String,
    baud_rate: u32,
    data_bits: u8,
    parity: String,
    stop_bits: u8,
    flow_control: String,
) -> Result<TestSerialResult, String> {
    if port_path.trim().is_empty() {
        return Ok(TestSerialResult {
            ok: false,
            message: "端口路径不能为空".to_string(),
        });
    }

    let timeout = Duration::from_secs(3);
    let port_result = serialport::new(port_path.clone(), baud_rate.max(1))
        .timeout(timeout)
        .data_bits(serial_data_bits(data_bits))
        .parity(serial_parity(&parity))
        .stop_bits(serial_stop_bits(stop_bits))
        .flow_control(serial_flow_control(&flow_control))
        .open();

    match port_result {
        Ok(_port) => Ok(TestSerialResult {
            ok: true,
            message: format!("端口 {port_path} 打开成功"),
        }),
        Err(error) => Ok(TestSerialResult {
            ok: false,
            message: format!("端口 {port_path} 打开失败: {error}"),
        }),
    }
}

#[tauri::command]
async fn load_project_library_file(
    app: AppHandle,
    workspace_path: Option<String>,
) -> Result<ProjectWorkspaceLoadResult, String> {
    let storage = resolve_project_workspace_storage(&app, workspace_path.as_deref())?;
    let library_text = if storage.library_exists {
        Some(
            fs::read_to_string(&storage.library_file_path)
                .await
                .map_err(|error| format!("读取工程库失败: {error}"))?,
        )
    } else {
        None
    };

    Ok(ProjectWorkspaceLoadResult {
        storage,
        library_text,
    })
}

#[tauri::command]
async fn save_project_library_file(
    app: AppHandle,
    workspace_path: Option<String>,
    library_text: String,
) -> Result<ProjectWorkspaceStorageInfo, String> {
    if library_text.len() > MAX_IPC_INPUT_BYTES {
        return Err("工程库文件超过最大允许大小（10 MB）".to_owned());
    }
    let storage = resolve_project_workspace_storage(&app, workspace_path.as_deref())?;
    let workspace_dir = PathBuf::from(&storage.workspace_path);

    fs::create_dir_all(&workspace_dir)
        .await
        .map_err(|error| format!("创建工程目录失败: {error}"))?;
    fs::write(&storage.library_file_path, library_text)
        .await
        .map_err(|error| format!("写入工程库失败: {error}"))?;

    resolve_project_workspace_storage(&app, workspace_path.as_deref())
}

fn stringify_error(error: EngineError) -> String {
    error.to_string()
}

fn resolve_project_workspace_storage(
    app: &AppHandle,
    workspace_path: Option<&str>,
) -> Result<ProjectWorkspaceStorageInfo, String> {
    let (workspace_dir, using_default_location) =
        resolve_project_workspace_dir(app, workspace_path)?;
    let library_file_path = workspace_dir.join("project-library.json");

    Ok(ProjectWorkspaceStorageInfo {
        workspace_path: workspace_dir.to_string_lossy().to_string(),
        library_file_path: library_file_path.to_string_lossy().to_string(),
        using_default_location,
        library_exists: library_file_path.exists(),
    })
}

/// 检查工作路径是否指向已知的系统敏感目录。
fn is_safe_workspace_path(path: &std::path::Path) -> Result<(), String> {
    let path_str = path.to_string_lossy();
    let forbidden_prefixes = [
        "/etc",
        "/var",
        "/sys",
        "/proc",
        "/dev",
        "/System",
        "/Library",
        "/usr",
        "/bin",
        "/sbin",
        "/private/etc",
        "/private/var",
    ];
    for prefix in &forbidden_prefixes {
        if path_str.starts_with(prefix) {
            return Err(format!("工作路径不允许指向系统目录: {prefix}"));
        }
    }
    Ok(())
}

fn resolve_project_workspace_dir(
    app: &AppHandle,
    workspace_path: Option<&str>,
) -> Result<(PathBuf, bool), String> {
    let trimmed = workspace_path.unwrap_or_default().trim();
    if trimmed.is_empty() {
        let default_dir = app
            .path()
            .app_local_data_dir()
            .map_err(|error| format!("无法解析默认工程目录: {error}"))?
            .join("workspace");
        return Ok((default_dir, true));
    }

    let expanded = expand_user_path(app, trimmed)?;
    if !expanded.is_absolute() {
        return Err("工作路径需要填写绝对路径。".to_owned());
    }

    is_safe_workspace_path(&expanded)?;

    Ok((expanded, false))
}

fn normalize_queue_capacity(value: usize) -> usize {
    value.clamp(1, 4_096)
}

fn derive_workflow_id(
    requested_workflow_id: Option<&str>,
    graph_name: Option<&str>,
    observability_context: Option<&ObservabilityContextInput>,
) -> String {
    if let Some(requested) = requested_workflow_id
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        return requested.to_owned();
    }

    if let Some(project_id) = observability_context
        .map(|context| context.project_id.trim())
        .filter(|value| !value.is_empty())
    {
        return project_id.to_owned();
    }

    let candidate = graph_name.map(str::trim).filter(|value| !value.is_empty());

    let sanitized = candidate
        .map(|value| {
            value
                .chars()
                .map(|ch| {
                    if ch.is_ascii_alphanumeric() {
                        ch.to_ascii_lowercase()
                    } else if matches!(ch, '-' | '_') {
                        ch
                    } else {
                        '-'
                    }
                })
                .collect::<String>()
                .trim_matches('-')
                .to_owned()
        })
        .filter(|value| !value.is_empty());

    sanitized.unwrap_or_else(|| format!("workflow-{}", chrono::Utc::now().timestamp_millis()))
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

fn create_dispatch_router(
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
                        continue;
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

async fn is_active_workflow(app: &AppHandle, workflow_id: &str) -> bool {
    let state: State<'_, DesktopState> = app.state();

    state.active_workflow_id.lock().await.as_deref() == Some(workflow_id)
}

fn expand_user_path(app: &AppHandle, raw_path: &str) -> Result<PathBuf, String> {
    if raw_path == "~" || raw_path.starts_with("~/") {
        let home_dir = app
            .path()
            .home_dir()
            .map_err(|error| format!("无法解析用户目录: {error}"))?;
        let suffix = raw_path.trim_start_matches('~').trim_start_matches('/');
        return Ok(if suffix.is_empty() {
            home_dir
        } else {
            home_dir.join(suffix)
        });
    }

    Ok(PathBuf::from(raw_path))
}

fn count_incoming_edges(graph: &WorkflowGraph) -> std::collections::HashMap<String, usize> {
    let mut incoming_counts = graph
        .nodes
        .keys()
        .map(|node_id| (node_id.clone(), 0_usize))
        .collect::<std::collections::HashMap<_, _>>();

    for edge in &graph.edges {
        if let Some(count) = incoming_counts.get_mut(&edge.to) {
            *count += 1;
        }
    }

    incoming_counts
}

fn collect_timer_root_specs(graph: &WorkflowGraph) -> Result<Vec<TimerRootSpec>, EngineError> {
    let incoming_counts = count_incoming_edges(graph);
    let mut timer_roots = Vec::new();

    for (node_id, node_definition) in &graph.nodes {
        if incoming_counts.get(node_id).copied().unwrap_or_default() != 0 {
            continue;
        }

        if node_definition.node_type != "timer" {
            continue;
        }

        let config: TimerNodeConfig = serde_json::from_value(node_definition.config.clone())
            .map_err(|error| {
                EngineError::node_config(node_definition.id.clone(), error.to_string())
            })?;

        timer_roots.push(TimerRootSpec {
            node_id: node_id.clone(),
            interval_ms: config.interval_ms.max(1),
            immediate: config.immediate,
        });
    }

    Ok(timer_roots)
}

fn is_serial_trigger_type(node_type: &str) -> bool {
    matches!(node_type, "serialTrigger" | "serial/trigger" | "serial")
}

fn is_serial_connection_type(connection_type: &str) -> bool {
    matches!(
        connection_type.trim().to_ascii_lowercase().as_str(),
        "serial" | "serialport" | "serial_port" | "uart" | "rs232" | "rs485"
    )
}

async fn collect_serial_root_specs(
    graph: &WorkflowGraph,
    connection_manager: nazh_engine::SharedConnectionManager,
) -> Result<Vec<SerialRootSpec>, EngineError> {
    let incoming_counts = count_incoming_edges(graph);
    let mut serial_roots = Vec::new();

    for (node_id, node_definition) in &graph.nodes {
        if incoming_counts.get(node_id).copied().unwrap_or_default() != 0 {
            continue;
        }

        if !is_serial_trigger_type(&node_definition.node_type) {
            continue;
        }

        let connection_id = node_definition
            .connection_id
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| {
                EngineError::node_config(node_id.clone(), "串口触发节点需要绑定串口连接资源")
            })?;
        let connection = connection_manager.get(connection_id).await.ok_or_else(|| {
            EngineError::node_config(
                node_id.clone(),
                format!("串口连接资源 `{connection_id}` 未注册"),
            )
        })?;

        if !is_serial_connection_type(&connection.kind) {
            let reason = format!("连接资源 `{connection_id}` 不是串口类型");
            let _ = connection_manager
                .mark_invalid_configuration(connection_id, &reason)
                .await;
            return Err(EngineError::node_config(node_id.clone(), reason));
        }

        let mut config: SerialTriggerNodeConfig = serde_json::from_value(connection.metadata)
            .map_err(|error| {
                EngineError::node_config(node_definition.id.clone(), error.to_string())
            })?;
        if let Some(inject) = node_definition
            .config
            .get("inject")
            .and_then(Value::as_object)
        {
            config.inject.clone_from(inject);
        }
        config.port_path = config.port_path.trim().to_owned();

        if config.port_path.is_empty() {
            let reason = format!("串口连接资源 `{connection_id}` 需要配置 port_path");
            let _ = connection_manager
                .mark_invalid_configuration(connection_id, &reason)
                .await;
            return Err(EngineError::node_config(node_id.clone(), reason));
        }

        serial_roots.push(SerialRootSpec {
            node_id: node_id.clone(),
            connection_id: connection_id.to_owned(),
            config,
        });
    }

    Ok(serial_roots)
}

fn spawn_timer_root_tasks(
    dispatch_router: WorkflowDispatchRouter,
    timer_roots: Vec<TimerRootSpec>,
) -> Vec<DesktopTriggerTask> {
    timer_roots
        .into_iter()
        .map(|timer_root| {
            let dispatch_router = dispatch_router.clone();
            let cancel = Arc::new(AtomicBool::new(false));
            let task_cancel = Arc::clone(&cancel);
            let join = tauri::async_runtime::spawn(async move {
                if timer_root.immediate && !task_cancel.load(Ordering::Relaxed) {
                    let _ = dispatch_router
                        .submit_trigger_to(
                            &timer_root.node_id,
                            WorkflowContext::new(Value::Object(Default::default())),
                            format!("timer:{}", timer_root.node_id),
                        )
                        .await;
                }

                let delay = Duration::from_millis(timer_root.interval_ms);

                loop {
                    tokio::time::sleep(delay).await;
                    if task_cancel.load(Ordering::Relaxed) {
                        break;
                    }
                    let _ = dispatch_router
                        .submit_trigger_to(
                            &timer_root.node_id,
                            WorkflowContext::new(Value::Object(Default::default())),
                            format!("timer:{}", timer_root.node_id),
                        )
                        .await;
                }
            });

            DesktopTriggerTask {
                cancel,
                join: TriggerJoinHandle::Async(join),
            }
        })
        .collect()
}

fn spawn_serial_root_tasks(
    app: AppHandle,
    dispatch_router: WorkflowDispatchRouter,
    connection_manager: nazh_engine::SharedConnectionManager,
    observability: Option<SharedObservabilityStore>,
    workflow_id: String,
    serial_roots: Vec<SerialRootSpec>,
) -> Vec<DesktopTriggerTask> {
    serial_roots
        .into_iter()
        .map(|serial_root| {
            let app = app.clone();
            let dispatch_router = dispatch_router.clone();
            let connection_manager = connection_manager.clone();
            let observability = observability.clone();
            let workflow_id = workflow_id.clone();
            let cancel = Arc::new(AtomicBool::new(false));
            let task_cancel = Arc::clone(&cancel);
            let join = std::thread::spawn(move || {
                run_serial_root_reader(
                    app,
                    dispatch_router,
                    connection_manager,
                    observability,
                    workflow_id,
                    serial_root,
                    task_cancel,
                );
            });

            DesktopTriggerTask {
                cancel,
                join: TriggerJoinHandle::Thread(join),
            }
        })
        .collect()
}

fn run_serial_root_reader(
    app: AppHandle,
    dispatch_router: WorkflowDispatchRouter,
    connection_manager: nazh_engine::SharedConnectionManager,
    observability: Option<SharedObservabilityStore>,
    workflow_id: String,
    serial_root: SerialRootSpec,
    cancel: Arc<AtomicBool>,
) {
    let config = serial_root.config.clone();
    let read_timeout = Duration::from_millis(config.read_timeout_ms.clamp(10, 2_000));
    let idle_gap = Duration::from_millis(config.idle_gap_ms.clamp(1, 10_000));
    let max_frame_bytes = config.max_frame_bytes.clamp(1, 8_192);
    let delimiter = decode_serial_delimiter(&config.delimiter);

    while !cancel.load(Ordering::Relaxed) {
        let lease = match tauri::async_runtime::block_on(
            connection_manager.borrow(&serial_root.connection_id),
        ) {
            Ok(lease) => lease,
            Err(error) => {
                let retry_after_ms = retry_delay_from_error(&error).unwrap_or(800);
                emit_serial_trigger_failure(
                    &app,
                    &workflow_id,
                    observability.as_ref(),
                    &serial_root.node_id,
                    error.to_string(),
                );
                sleep_with_cancel(&cancel, Duration::from_millis(retry_after_ms));
                continue;
            }
        };
        let heartbeat_interval = Duration::from_millis(
            governance_u64(&lease.metadata, "heartbeat_interval_ms")
                .unwrap_or(3_000)
                .clamp(250, 30_000),
        );

        let connect_started_at = Instant::now();
        let port_result = serialport::new(config.port_path.clone(), config.baud_rate.max(1))
            .timeout(read_timeout)
            .data_bits(serial_data_bits(config.data_bits))
            .parity(serial_parity(&config.parity))
            .stop_bits(serial_stop_bits(config.stop_bits))
            .flow_control(serial_flow_control(&config.flow_control))
            .open();
        let mut port = match port_result {
            Ok(port) => {
                let connect_latency_ms = connect_started_at.elapsed().as_millis() as u64;
                let _ = tauri::async_runtime::block_on(connection_manager.record_connect_success(
                    &serial_root.connection_id,
                    format!("串口 {} 已建立监听，等待外设上报数据", config.port_path),
                    Some(connect_latency_ms),
                ));
                port
            }
            Err(error) => {
                let reason = format!("串口打开失败: {error}");
                let retry_after_ms = tauri::async_runtime::block_on(
                    connection_manager.record_connect_failure(&serial_root.connection_id, &reason),
                )
                .unwrap_or(800);
                let _ = tauri::async_runtime::block_on(
                    connection_manager.release(&serial_root.connection_id),
                );
                emit_serial_trigger_failure(
                    &app,
                    &workflow_id,
                    observability.as_ref(),
                    &serial_root.node_id,
                    reason,
                );
                sleep_with_cancel(&cancel, Duration::from_millis(retry_after_ms));
                continue;
            }
        };
        let mut last_heartbeat_sent_at = Instant::now();

        let mut buffer = Vec::with_capacity(max_frame_bytes.min(512));
        let mut scratch = [0_u8; 64];
        let mut last_byte_at: Option<Instant> = None;
        let mut disconnected_reason: Option<String> = None;

        while !cancel.load(Ordering::Relaxed) {
            match port.read(&mut scratch) {
                Ok(0) => {
                    flush_idle_serial_frame(
                        &app,
                        &dispatch_router,
                        &workflow_id,
                        &serial_root,
                        observability.as_ref(),
                        &mut buffer,
                        last_byte_at,
                        idle_gap,
                    );

                    if last_heartbeat_sent_at.elapsed() >= heartbeat_interval {
                        let _ =
                            tauri::async_runtime::block_on(connection_manager.record_heartbeat(
                                &serial_root.connection_id,
                                format!("串口 {} 心跳正常，监听仍在进行中", config.port_path),
                            ));
                        last_heartbeat_sent_at = Instant::now();
                    }
                }
                Ok(bytes_read) => {
                    buffer.extend_from_slice(&scratch[..bytes_read]);
                    last_byte_at = Some(Instant::now());

                    let _ = tauri::async_runtime::block_on(connection_manager.record_heartbeat(
                        &serial_root.connection_id,
                        format!("串口 {} 收到 {} 字节输入", config.port_path, bytes_read),
                    ));
                    last_heartbeat_sent_at = Instant::now();

                    while let Some(frame) = drain_serial_delimited_frame(&mut buffer, &delimiter) {
                        submit_serial_frame(
                            &app,
                            &dispatch_router,
                            &workflow_id,
                            &serial_root,
                            observability.as_ref(),
                            &frame,
                        );
                    }

                    if buffer.len() >= max_frame_bytes {
                        let frame = buffer.drain(..max_frame_bytes).collect::<Vec<_>>();
                        submit_serial_frame(
                            &app,
                            &dispatch_router,
                            &workflow_id,
                            &serial_root,
                            observability.as_ref(),
                            &frame,
                        );
                    }
                }
                Err(error) if error.kind() == std::io::ErrorKind::TimedOut => {
                    if buffer.is_empty() {
                        if last_heartbeat_sent_at.elapsed() >= heartbeat_interval {
                            let _ = tauri::async_runtime::block_on(
                                connection_manager.record_heartbeat(
                                    &serial_root.connection_id,
                                    format!("串口 {} 空闲等待中，链路仍存活", config.port_path),
                                ),
                            );
                            last_heartbeat_sent_at = Instant::now();
                        }
                        continue;
                    }

                    let Some(last_byte_at_instant) = last_byte_at else {
                        continue;
                    };

                    if last_byte_at_instant.elapsed() < idle_gap {
                        continue;
                    }

                    flush_idle_serial_frame(
                        &app,
                        &dispatch_router,
                        &workflow_id,
                        &serial_root,
                        observability.as_ref(),
                        &mut buffer,
                        last_byte_at,
                        idle_gap,
                    );
                }
                Err(error) => {
                    disconnected_reason = Some(format!("串口读取失败: {error}"));
                    break;
                }
            }
        }

        if !cancel.load(Ordering::Relaxed) && !buffer.is_empty() {
            submit_serial_frame(
                &app,
                &dispatch_router,
                &workflow_id,
                &serial_root,
                observability.as_ref(),
                &buffer,
            );
        }

        if cancel.load(Ordering::Relaxed) {
            let _ = tauri::async_runtime::block_on(
                connection_manager.release(&serial_root.connection_id),
            );
            let reason = format!("串口 {} 监听已停止", config.port_path);
            let _ = tauri::async_runtime::block_on(
                connection_manager.mark_disconnected(&serial_root.connection_id, &reason),
            );
            break;
        }

        let reason =
            disconnected_reason.unwrap_or_else(|| format!("串口 {} 连接已断开", config.port_path));
        let retry_after_ms = tauri::async_runtime::block_on(
            connection_manager.record_connect_failure(&serial_root.connection_id, &reason),
        )
        .unwrap_or(800);
        let _ =
            tauri::async_runtime::block_on(connection_manager.release(&serial_root.connection_id));
        emit_serial_trigger_failure(
            &app,
            &workflow_id,
            observability.as_ref(),
            &serial_root.node_id,
            reason,
        );
        sleep_with_cancel(&cancel, Duration::from_millis(retry_after_ms));
    }
}

fn flush_idle_serial_frame(
    app: &AppHandle,
    dispatch_router: &WorkflowDispatchRouter,
    workflow_id: &str,
    serial_root: &SerialRootSpec,
    observability: Option<&SharedObservabilityStore>,
    buffer: &mut Vec<u8>,
    last_byte_at: Option<Instant>,
    idle_gap: Duration,
) {
    if buffer.is_empty() {
        return;
    }

    if last_byte_at.is_some_and(|instant| instant.elapsed() >= idle_gap) {
        let frame = std::mem::take(buffer);
        submit_serial_frame(
            app,
            dispatch_router,
            workflow_id,
            serial_root,
            observability,
            &frame,
        );
    }
}

fn drain_serial_delimited_frame(buffer: &mut Vec<u8>, delimiter: &[u8]) -> Option<Vec<u8>> {
    if delimiter.is_empty() || buffer.len() < delimiter.len() {
        return None;
    }

    let delimiter_index = buffer
        .windows(delimiter.len())
        .position(|window| window == delimiter)?;
    let frame = buffer.drain(..delimiter_index).collect::<Vec<_>>();
    let _ = buffer.drain(..delimiter.len()).count();
    Some(frame)
}

fn submit_serial_frame(
    app: &AppHandle,
    dispatch_router: &WorkflowDispatchRouter,
    workflow_id: &str,
    serial_root: &SerialRootSpec,
    observability: Option<&SharedObservabilityStore>,
    frame: &[u8],
) {
    if frame.is_empty() {
        return;
    }

    let payload = json!({
        "_serial_frame": {
            "ascii": String::from_utf8_lossy(frame).to_string(),
            "hex": bytes_to_hex(frame),
            "byte_len": frame.len(),
            "port_path": serial_root.config.port_path.as_str(),
            "connection_id": serial_root.connection_id.as_str(),
            "baud_rate": serial_root.config.baud_rate,
            "data_bits": serial_root.config.data_bits,
            "parity": serial_root.config.parity.as_str(),
            "stop_bits": serial_root.config.stop_bits,
            "flow_control": serial_root.config.flow_control.as_str(),
            "encoding": serial_root.config.encoding.as_str(),
        }
    });

    if let Err(error) = dispatch_router.blocking_submit_trigger_to(
        &serial_root.node_id,
        WorkflowContext::new(payload),
        format!("serial:{}", serial_root.node_id),
    ) {
        emit_serial_trigger_failure(app, workflow_id, observability, &serial_root.node_id, error);
    }
}

fn emit_serial_trigger_failure(
    app: &AppHandle,
    workflow_id: &str,
    observability: Option<&SharedObservabilityStore>,
    node_id: &str,
    message: String,
) {
    let context = WorkflowContext::new(Value::Object(Default::default()));
    if let Some(store) = observability {
        let _ = tauri::async_runtime::block_on(store.record_external_failure(
            node_id,
            "串口触发失败".to_owned(),
            Some(message.clone()),
            Some(context.trace_id.to_string()),
            None,
        ));
    }
    let event = ExecutionEvent::Failed {
        stage: node_id.to_owned(),
        trace_id: context.trace_id,
        error: message,
    };
    let _ = app.emit(
        "workflow://node-status-v2",
        ScopedExecutionEvent {
            workflow_id: workflow_id.to_owned(),
            event: event.clone(),
        },
    );
    if tauri::async_runtime::block_on(is_active_workflow(app, workflow_id)) {
        let _ = app.emit("workflow://node-status", event);
    }
}

fn retry_delay_from_error(error: &EngineError) -> Option<u64> {
    match error {
        EngineError::ConnectionRateLimited { retry_after_ms, .. }
        | EngineError::ConnectionCircuitOpen { retry_after_ms, .. } => Some(*retry_after_ms),
        _ => None,
    }
}

fn governance_u64(metadata: &Value, key: &str) -> Option<u64> {
    metadata
        .as_object()
        .and_then(|value| value.get("governance"))
        .and_then(Value::as_object)
        .and_then(|governance| governance.get(key))
        .and_then(Value::as_u64)
}

fn sleep_with_cancel(cancel: &AtomicBool, duration: Duration) {
    let start = Instant::now();
    while start.elapsed() < duration {
        if cancel.load(Ordering::Relaxed) {
            break;
        }
        std::thread::sleep(
            Duration::from_millis(100).min(duration.saturating_sub(start.elapsed())),
        );
    }
}

fn serial_data_bits(value: u8) -> serialport::DataBits {
    match value {
        5 => serialport::DataBits::Five,
        6 => serialport::DataBits::Six,
        7 => serialport::DataBits::Seven,
        _ => serialport::DataBits::Eight,
    }
}

fn serial_parity(value: &str) -> serialport::Parity {
    match value.trim().to_ascii_lowercase().as_str() {
        "odd" | "o" => serialport::Parity::Odd,
        "even" | "e" => serialport::Parity::Even,
        _ => serialport::Parity::None,
    }
}

fn serial_stop_bits(value: u8) -> serialport::StopBits {
    if value == 2 {
        serialport::StopBits::Two
    } else {
        serialport::StopBits::One
    }
}

fn serial_flow_control(value: &str) -> serialport::FlowControl {
    match value.trim().to_ascii_lowercase().as_str() {
        "software" | "xonxoff" => serialport::FlowControl::Software,
        "hardware" | "rtscts" => serialport::FlowControl::Hardware,
        _ => serialport::FlowControl::None,
    }
}

fn decode_serial_delimiter(value: &str) -> Vec<u8> {
    if value.is_empty() {
        return Vec::new();
    }

    let trimmed = value.trim();
    if let Some(hex) = trimmed
        .strip_prefix("hex:")
        .or_else(|| trimmed.strip_prefix("0x"))
    {
        return parse_hex_bytes(hex);
    }

    let mut bytes = Vec::new();
    let mut chars = value.chars();

    while let Some(ch) = chars.next() {
        if ch != '\\' {
            let mut encoded = [0_u8; 4];
            bytes.extend_from_slice(ch.encode_utf8(&mut encoded).as_bytes());
            continue;
        }

        match chars.next() {
            Some('n') => bytes.push(b'\n'),
            Some('r') => bytes.push(b'\r'),
            Some('t') => bytes.push(b'\t'),
            Some('\\') => bytes.push(b'\\'),
            Some(other) => {
                let mut encoded = [0_u8; 4];
                bytes.extend_from_slice(other.encode_utf8(&mut encoded).as_bytes());
            }
            None => bytes.push(b'\\'),
        }
    }

    bytes
}

fn parse_hex_bytes(value: &str) -> Vec<u8> {
    let nibbles = value.bytes().filter_map(hex_nibble).collect::<Vec<_>>();
    let mut bytes = Vec::with_capacity(nibbles.len() / 2);

    for pair in nibbles.chunks(2) {
        if pair.len() == 2 {
            bytes.push((pair[0] << 4) | pair[1]);
        }
    }

    bytes
}

fn hex_nibble(value: u8) -> Option<u8> {
    match value {
        b'0'..=b'9' => Some(value - b'0'),
        b'a'..=b'f' => Some(value - b'a' + 10),
        b'A'..=b'F' => Some(value - b'A' + 10),
        _ => None,
    }
}

fn bytes_to_hex(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789ABCDEF";
    let mut output = String::with_capacity(bytes.len().saturating_mul(3).saturating_sub(1));

    for (index, byte) in bytes.iter().enumerate() {
        if index > 0 {
            output.push(' ');
        }
        output.push(HEX[(byte >> 4) as usize] as char);
        output.push(HEX[(byte & 0x0f) as usize] as char);
    }

    output
}

fn normalize_sql_writer_paths(
    app: &AppHandle,
    graph: &mut WorkflowGraph,
) -> Result<(), EngineError> {
    let data_root = app
        .path()
        .app_local_data_dir()
        .map_err(|error| EngineError::invalid_graph(format!("无法解析桌面数据目录: {error}")))?;

    for node_definition in graph.nodes.values_mut() {
        if node_definition.node_type != "sqlWriter" && node_definition.node_type != "sql/writer" {
            continue;
        }

        let Some(config_map) = node_definition.config.as_object_mut() else {
            continue;
        };

        let raw_database_path = config_map
            .get("database_path")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .unwrap_or("./nazh-local.sqlite3");

        if Path::new(raw_database_path).is_absolute() {
            continue;
        }

        let resolved_path = data_root
            .join("sqlite")
            .join(sanitize_relative_path(raw_database_path));
        config_map.insert(
            "database_path".to_owned(),
            Value::String(resolved_path.to_string_lossy().to_string()),
        );
    }

    Ok(())
}

fn sanitize_relative_path(raw_path: &str) -> PathBuf {
    let mut sanitized = PathBuf::new();

    for component in Path::new(raw_path).components() {
        match component {
            Component::Normal(segment) => sanitized.push(segment),
            Component::CurDir => {}
            Component::ParentDir | Component::RootDir | Component::Prefix(_) => {}
        }
    }

    if sanitized.as_os_str().is_empty() {
        sanitized.push("nazh-local.sqlite3");
    }

    sanitized
}

#[cfg(test)]
mod tests {
    use super::sanitize_relative_path;
    use std::path::PathBuf;

    #[test]
    fn sanitize_relative_path_removes_escape_segments() {
        let sanitized = sanitize_relative_path("../data/./edge-runtime.sqlite3");
        assert_eq!(sanitized, PathBuf::from("data/edge-runtime.sqlite3"));
    }

    #[test]
    fn sanitize_relative_path_falls_back_when_empty() {
        let sanitized = sanitize_relative_path("./");
        assert_eq!(sanitized, PathBuf::from("nazh-local.sqlite3"));
    }
}

/// 初始化全局 tracing subscriber，输出到 stderr。
///
/// 通过 `RUST_LOG` 环境变量控制日志级别，默认为 `nazh_engine=info,nazh_desktop_lib=info`。
fn init_tracing() {
    use tracing_subscriber::EnvFilter;

    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("nazh_engine=info,nazh_desktop_lib=info"));

    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_target(true)
        .with_thread_ids(false)
        .with_file(false)
        .with_line_number(false)
        .init();
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    init_tracing();

    let builder = tauri::Builder::default()
        .manage(DesktopState::default())
        .setup(|app| {
            let app_handle = app.handle().clone();
            let state: State<'_, DesktopState> = app.state();
            let manager = state.connection_manager.clone();
            let ai_config_arc = state.ai_config.clone();
            tauri::async_runtime::spawn({
                let app_handle = app_handle.clone();
                async move {
                    DesktopState::load_connections_from_disk(&app_handle, manager, None).await;
                }
            });
            tauri::async_runtime::spawn(async move {
                if let Ok(path) = DesktopState::ai_config_file_path(&app_handle) {
                    if path.exists() {
                        if let Ok(text) = tokio::fs::read_to_string(&path).await {
                            if let Ok(mut file_config) = serde_json::from_str::<AiConfigFile>(&text) {
                                file_config.normalize();
                                let mut config = ai_config_arc.write().await;
                                *config = file_config;
                            }
                        }
                    }
                }
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            deploy_workflow,
            dispatch_payload,
            undeploy_workflow,
            list_connections,
            list_node_types,
            list_runtime_workflows,
            set_active_runtime_workflow,
            list_dead_letters,
            query_observability,
            load_connection_definitions,
            save_connection_definitions,
            load_deployment_session_file,
            load_deployment_session_state_file,
            list_deployment_sessions_file,
            save_deployment_session_file,
            set_deployment_session_active_project_file,
            remove_deployment_session_file,
            clear_deployment_session_file,
            list_serial_ports,
            test_serial_connection,
            load_project_library_file,
            save_project_library_file,
            load_ai_config,
            save_ai_config,
            test_ai_provider,
            copilot_complete
        ]);

    if let Err(error) = builder.run(tauri::generate_context!()) {
        tracing::error!("Nazh 桌面壳层运行失败: {error}");
    }
}
