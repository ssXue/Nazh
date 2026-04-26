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

use ai::{
    AiConfigFile, AiConfigUpdate, AiConfigView, AiProviderDraft, AiTestResult,
    OpenAiCompatibleService,
};
use nazh_engine::{
    AiCompletionRequest, AiCompletionResponse, AiService, ConnectionDefinition, ConnectionRecord,
    EngineError, ExecutionEvent, RuntimeResources, SharedResources, WorkflowContext, WorkflowGraph,
    WorkflowIngress, WorkflowNodeDefinition, deploy_workflow_with_ai as deploy_workflow_graph,
    shared_connection_manager, standard_registry,
};
use observability::{
    ObservabilityContextInput, ObservabilityQueryResult, ObservabilityStore,
    SharedObservabilityStore, query_observability as query_workspace_observability,
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use tauri::{AppHandle, Emitter, Manager, State};
use tauri_bindings::{
    DeployResponse, DescribeNodePinsRequest, DescribeNodePinsResponse, DispatchResponse,
    ListNodeTypesResponse, UndeployResponse, list_node_types_response,
};
use tokio::fs;
use tokio::sync::{Mutex, RwLock, mpsc};
#[cfg(target_os = "windows")]
use window_vibrancy::apply_blur;
#[cfg(target_os = "macos")]
use window_vibrancy::{NSVisualEffectMaterial, apply_vibrancy};

use std::{
    collections::HashMap,
    io::Write,
    path::{Component, Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicU64, AtomicUsize, Ordering},
    },
    time::Duration,
};

/// IPC 命令输入的最大允许字节数（10 MB）。
const MAX_IPC_INPUT_BYTES: usize = 10 * 1024 * 1024;
const MAX_EXPORT_FILE_BYTES: usize = 25 * 1024 * 1024;
const DEAD_LETTER_DIR: &str = "runtime";
const DEAD_LETTER_FILE: &str = "dead-letters.jsonl";
const PROJECT_BOARDS_DIR: &str = "boards";
const PROJECT_EXPORTS_DIR: &str = "exports";
const PROJECT_BOARD_FILE_SUFFIX: &str = ".nazh-board.json";
const SQL_WRITER_DEFAULT_DATABASE_PATH: &str = "./nazh-local.sqlite3";

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
struct DesktopWorkflow {
    workflow_id: String,
    metadata: RuntimeWorkflowMetadata,
    policy: WorkflowRuntimePolicy,
    dispatch_router: WorkflowDispatchRouter,
    observability: Option<SharedObservabilityStore>,
    node_count: usize,
    edge_count: usize,
    root_nodes: Vec<String>,
    /// 引擎 lifecycle guards（按部署顺序）。撤销时按逆序 await shutdown。
    lifecycle_guards: Vec<(String, nazh_engine::LifecycleGuard)>,
    /// 撤销根 token——cancel 后所有 guard 内部派生的 child token 同时收到信号。
    shutdown_token: nazh_engine::CancellationToken,
    /// 事件/结果转发任务。
    runtime_tasks: Vec<tauri::async_runtime::JoinHandle<()>>,
}

impl DesktopWorkflow {
    /// 撤销整个运行时：中止事件转发任务 + 广播 cancel + 串行 shutdown 所有 guards。
    ///
    /// 返回 shutdown 的 lifecycle guards 数量——通过 `UndeployResponse`
    /// 的 `aborted_timer_count` 字段透传。字段名沿用历史命名（语义为"已撤销
    /// 的触发器节点数"），改名会破坏 IPC 契约且需同步前端。
    async fn shutdown_runtime(&mut self) -> usize {
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
///
/// `ai_service` 持有具体类型（`Arc<OpenAiCompatibleService>`）而非 `dyn AiService`，
/// 因为壳层除了 trait 方法外还要调用 inherent 的 `test_connection`（草稿配置
/// 测试不属于 Ring 0 运行时关注点）。注入到引擎部署时会自动 unsize 到
/// `Arc<dyn AiService>`。
struct DesktopState {
    connection_manager: nazh_engine::SharedConnectionManager,
    workflows: Mutex<HashMap<String, DesktopWorkflow>>,
    active_workflow_id: Mutex<Option<String>>,
    ai_config: Arc<RwLock<AiConfigFile>>,
    ai_service: Arc<OpenAiCompatibleService>,
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

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct ProjectWorkspaceStorageInfo {
    workspace_path: String,
    boards_directory_path: String,
    using_default_location: bool,
    board_file_count: usize,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct ProjectWorkspaceLoadResult {
    storage: ProjectWorkspaceStorageInfo,
    board_files: Vec<ProjectWorkspaceBoardFile>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ProjectWorkspaceBoardFile {
    file_name: String,
    text: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct SavedWorkspaceFile {
    file_path: String,
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

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct ConnectionDefinitionsLoadResult {
    definitions: Vec<ConnectionDefinition>,
    file_exists: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PersistedDeploymentSessionState {
    version: u8,
    #[serde(default)]
    active_project_id: Option<String>,
    sessions: Vec<PersistedDeploymentSession>,
}

fn sort_deployment_sessions_by_freshness(sessions: &mut [PersistedDeploymentSession]) {
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
#[allow(clippy::too_many_lines)]
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
    let mut graph = WorkflowGraph::from_json(&ast).map_err(|e| stringify_error(&e))?;
    let (workspace_dir, _) = resolve_project_workspace_dir(
        &app,
        observability_context
            .as_ref()
            .map(|context| context.workspace_path.as_str()),
    )?;
    normalize_sql_writer_paths(&mut graph, &workspace_dir).map_err(|e| stringify_error(&e))?;
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
    let node_count = graph.nodes.len();
    let edge_count = graph.edges.len();
    let registry = standard_registry();
    let ai_service: Arc<dyn AiService> = state.ai_service.clone();
    let deployment = match deploy_workflow_graph(
        graph,
        state.connection_manager.clone(),
        Some(ai_service),
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
            return Err(stringify_error(&error));
        }
    };
    let nazh_engine::WorkflowDeploymentParts {
        ingress,
        streams,
        lifecycle_guards,
        shutdown_token,
    } = deployment.into_parts();
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
        existing.shutdown_runtime().await;
    }

    let event_app = app.clone();
    let event_store = observability_store.clone();
    let workflow_id_for_event = workflow_id.clone();
    runtime_tasks.push(tauri::async_runtime::spawn(async move {
        while let Some(event) = event_rx.recv().await {
            if let Some(store) = &event_store {
                let _ = store.record_execution_event(&event).await;
            }
            let _ = event_app.emit(
                "workflow://node-status",
                ScopedExecutionEvent {
                    workflow_id: workflow_id_for_event.clone(),
                    event,
                },
            );
        }
    }));

    let result_app = app.clone();
    let result_store = observability_store.clone();
    let workflow_id_for_result = workflow_id.clone();
    runtime_tasks.push(tauri::async_runtime::spawn(async move {
        while let Some(ctx_ref) = result_rx.recv().await {
            // 从 DataStore 重建 WorkflowContext
            let Ok(payload) = result_store_ref.read(&ctx_ref.data_id) else {
                continue;
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
                "workflow://result",
                ScopedWorkflowResult {
                    workflow_id: workflow_id_for_result.clone(),
                    result,
                },
            );
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
                lifecycle_guards,
                shutdown_token,
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
        .ok_or_else(|| stringify_error(&EngineError::WorkflowUnavailable))?;
    let (dispatch_router, observability_store) = {
        let workflows = state.workflows.lock().await;
        let workflow = workflows
            .get(&target_workflow_id)
            .ok_or_else(|| stringify_error(&EngineError::WorkflowUnavailable))?;
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
            aborted_timer_count: workflow.shutdown_runtime().await,
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
        (*active_workflow_id).clone_from(&fallback_active);
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
    Ok(list_node_types_response(&registry))
}

/// 给定节点类型 + config，返回该节点实例的 input/output pin schema。
///
/// 用于 ADR-0010 Phase 2 前端连接期校验：FlowGram `canAddLine` 钩子
/// 通过缓存的 pin schema 即时判断"上游产出 → 下游期望"是否兼容。
///
/// 实例化是无副作用的（只读 config + 资源句柄克隆，不进入 `on_deploy`）。
/// 返回错误时前端会写 fallback `Any/Any` 缓存——部署期校验作为 backstop。
#[tauri::command]
async fn describe_node_pins(
    request: DescribeNodePinsRequest,
) -> Result<DescribeNodePinsResponse, String> {
    let registry = standard_registry();
    // 把请求拼成 WorkflowNodeDefinition 的 JSON 形态（字段是私有的，外部 crate
    // 无构造器可用——deserialize 是公开的入口）。dummy id 防止与真实节点冲突。
    let definition_json = json!({
        "id": "_describe_pins_probe",
        "type": request.node_type,
        "config": request.config,
    });
    let definition: WorkflowNodeDefinition = serde_json::from_value(definition_json)
        .map_err(|error| format!("无法解析节点定义：{error}"))?;

    // 仅注入 connection_manager——describe_pins 不读连接，只让需要 conn 句柄的
    // 节点构造器（modbus / mqtt / http）能克隆出引用。无 AI service / observability，
    // 这些与 pin schema 无关。
    let resources: SharedResources =
        Arc::new(RuntimeResources::new().with_resource(shared_connection_manager()));

    let node = registry
        .create(&definition, resources)
        .map_err(|error| format!("无法实例化节点：{error}"))?;

    Ok(DescribeNodePinsResponse {
        input_pins: node.input_pins(),
        output_pins: node.output_pins(),
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
) -> Result<ConnectionDefinitionsLoadResult, String> {
    let path = DesktopState::connections_file_path(&app, workspace_path.as_deref())?;
    let file_exists = path.exists();
    if !path.exists() {
        state
            .connection_manager
            .replace_connections(Vec::<ConnectionDefinition>::new())
            .await;
        return Ok(ConnectionDefinitionsLoadResult {
            definitions: Vec::new(),
            file_exists,
        });
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
    Ok(ConnectionDefinitionsLoadResult {
        definitions: defs,
        file_exists,
    })
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

/// 流式 copilot completion，通过 Tauri 事件逐 token 发送到前端。
#[tauri::command]
async fn copilot_complete_stream(
    app: tauri::AppHandle,
    state: State<'_, DesktopState>,
    request: AiCompletionRequest,
    stream_id: String,
) -> Result<(), String> {
    let service = Arc::clone(&state.ai_service);

    let mut rx = service
        .stream_complete(request)
        .await
        .map_err(|error| error.to_string())?;

    let event_name = format!("copilot://stream/{stream_id}");

    let app_clone = app.clone();
    tokio::spawn(async move {
        while let Some(chunk_result) = rx.recv().await {
            match chunk_result {
                Ok(chunk) => {
                    let is_done = chunk.done;
                    let payload: serde_json::Value =
                        serde_json::to_value(&chunk).unwrap_or_default();
                    let _ = app_clone.emit(&event_name, payload);
                    if is_done {
                        break;
                    }
                }
                Err(error) => {
                    let payload: serde_json::Value = serde_json::json!({
                        "error": error.to_string(),
                        "done": true
                    });
                    let _ = app_clone.emit(&event_name, payload);
                    break;
                }
            }
        }
    });

    Ok(())
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
    } else if path_lower.contains("/dev/cu.")
        || path_lower.contains("/dev/tty.")
        || path_lower.contains("/dev/ttyusb")
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
async fn load_project_board_files(
    app: AppHandle,
    workspace_path: Option<String>,
) -> Result<ProjectWorkspaceLoadResult, String> {
    let storage = resolve_project_workspace_storage(&app, workspace_path.as_deref())?;
    let board_file_paths =
        list_project_board_file_paths(Path::new(&storage.boards_directory_path))?;
    let mut board_files = Vec::with_capacity(board_file_paths.len());
    for file_path in board_file_paths {
        let text = fs::read_to_string(&file_path)
            .await
            .map_err(|error| format!("读取看板文件失败 `{}`: {error}", file_path.display()))?;
        let file_name = file_path
            .file_name()
            .and_then(|name| name.to_str())
            .ok_or_else(|| format!("看板文件名无效: {}", file_path.display()))?
            .to_owned();
        board_files.push(ProjectWorkspaceBoardFile { file_name, text });
    }

    Ok(ProjectWorkspaceLoadResult {
        storage,
        board_files,
    })
}

#[tauri::command]
async fn save_project_board_files(
    app: AppHandle,
    workspace_path: Option<String>,
    board_files: Vec<ProjectWorkspaceBoardFile>,
) -> Result<ProjectWorkspaceStorageInfo, String> {
    for board_file in &board_files {
        if board_file.text.len() > MAX_IPC_INPUT_BYTES {
            return Err(format!(
                "看板文件 `{}` 超过最大允许大小（10 MB）",
                board_file.file_name
            ));
        }
    }

    let storage = resolve_project_workspace_storage(&app, workspace_path.as_deref())?;
    let workspace_dir = PathBuf::from(&storage.workspace_path);
    let boards_dir = PathBuf::from(&storage.boards_directory_path);

    fs::create_dir_all(&workspace_dir)
        .await
        .map_err(|error| format!("创建工程目录失败: {error}"))?;
    fs::create_dir_all(&boards_dir)
        .await
        .map_err(|error| format!("创建看板目录失败: {error}"))?;

    let mut expected_paths = std::collections::HashSet::new();
    for board_file in board_files {
        let file_name = sanitize_project_board_file_name(&board_file.file_name)?;
        let file_path = boards_dir.join(&file_name);
        expected_paths.insert(file_path.clone());
        fs::write(&file_path, board_file.text)
            .await
            .map_err(|error| format!("写入看板文件失败 `{}`: {error}", file_path.display()))?;
    }

    for existing_path in list_project_board_file_paths(&boards_dir)? {
        if expected_paths.contains(&existing_path) {
            continue;
        }

        fs::remove_file(&existing_path).await.map_err(|error| {
            format!("删除旧看板文件失败 `{}`: {error}", existing_path.display())
        })?;
    }

    resolve_project_workspace_storage(&app, workspace_path.as_deref())
}

#[tauri::command]
async fn save_flowgram_export_file(
    app: AppHandle,
    workspace_path: Option<String>,
    file_name: String,
    text: Option<String>,
    bytes: Option<Vec<u8>>,
) -> Result<SavedWorkspaceFile, String> {
    let (workspace_dir, _) = resolve_project_workspace_dir(&app, workspace_path.as_deref())?;
    let export_dir = workspace_dir.join(PROJECT_EXPORTS_DIR);
    let sanitized_file_name = sanitize_export_file_name(&file_name)?;
    let target_path = build_nonconflicting_file_path(&export_dir, &sanitized_file_name);

    fs::create_dir_all(&export_dir)
        .await
        .map_err(|error| format!("创建导出目录失败: {error}"))?;

    match (text, bytes) {
        (Some(text), None) => {
            if text.len() > MAX_EXPORT_FILE_BYTES {
                return Err("导出文件超过最大允许大小（25 MB）".to_owned());
            }
            fs::write(&target_path, text).await.map_err(|error| {
                format!("写入导出文件失败 `{}`: {error}", target_path.display())
            })?;
        }
        (None, Some(bytes)) => {
            if bytes.len() > MAX_EXPORT_FILE_BYTES {
                return Err("导出文件超过最大允许大小（25 MB）".to_owned());
            }
            fs::write(&target_path, bytes).await.map_err(|error| {
                format!("写入导出文件失败 `{}`: {error}", target_path.display())
            })?;
        }
        (None, None) => {
            return Err("导出内容不能为空。".to_owned());
        }
        (Some(_), Some(_)) => {
            return Err("导出内容不能同时包含文本和二进制。".to_owned());
        }
    }

    Ok(SavedWorkspaceFile {
        file_path: target_path.to_string_lossy().to_string(),
    })
}

fn stringify_error(error: &EngineError) -> String {
    error.to_string()
}

fn resolve_project_workspace_storage(
    app: &AppHandle,
    workspace_path: Option<&str>,
) -> Result<ProjectWorkspaceStorageInfo, String> {
    let (workspace_dir, using_default_location) =
        resolve_project_workspace_dir(app, workspace_path)?;
    let boards_directory_path = workspace_dir.join(PROJECT_BOARDS_DIR);

    Ok(ProjectWorkspaceStorageInfo {
        workspace_path: workspace_dir.to_string_lossy().to_string(),
        boards_directory_path: boards_directory_path.to_string_lossy().to_string(),
        using_default_location,
        board_file_count: count_project_board_files(&boards_directory_path)?,
    })
}

fn sanitize_project_board_file_name(file_name: &str) -> Result<String, String> {
    let trimmed = file_name.trim();
    if trimmed.is_empty() {
        return Err("看板文件名不能为空。".to_owned());
    }
    if trimmed.contains('/') || trimmed.contains('\\') {
        return Err(format!("看板文件名不允许包含路径分隔符: {trimmed}"));
    }
    if !trimmed.ends_with(PROJECT_BOARD_FILE_SUFFIX) {
        return Err(format!(
            "看板文件名必须以 `{PROJECT_BOARD_FILE_SUFFIX}` 结尾: {trimmed}"
        ));
    }
    Ok(trimmed.to_owned())
}

fn sanitize_export_file_name(file_name: &str) -> Result<String, String> {
    let trimmed = file_name.trim();
    if trimmed.is_empty() {
        return Err("导出文件名不能为空。".to_owned());
    }
    if trimmed.contains('/') || trimmed.contains('\\') {
        return Err(format!("导出文件名不允许包含路径分隔符: {trimmed}"));
    }
    Ok(trimmed.to_owned())
}

fn build_nonconflicting_file_path(dir: &Path, file_name: &str) -> PathBuf {
    let candidate = dir.join(file_name);
    if !candidate.exists() {
        return candidate;
    }

    let path = Path::new(file_name);
    let stem = path
        .file_stem()
        .and_then(|value| value.to_str())
        .filter(|value| !value.is_empty())
        .unwrap_or("flowgram-export");
    let ext = path
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or("");

    let mut index = 2usize;
    loop {
        let next_name = if ext.is_empty() {
            format!("{stem}-{index}")
        } else {
            format!("{stem}-{index}.{ext}")
        };
        let next_path = dir.join(next_name);
        if !next_path.exists() {
            return next_path;
        }
        index += 1;
    }
}

fn count_project_board_files(boards_dir: &Path) -> Result<usize, String> {
    if !boards_dir.exists() {
        return Ok(0);
    }

    let entries = std::fs::read_dir(boards_dir)
        .map_err(|error| format!("读取看板目录失败 `{}`: {error}", boards_dir.display()))?;
    let mut count = 0usize;
    for entry in entries {
        let entry = entry
            .map_err(|error| format!("读取看板目录条目失败 `{}`: {error}", boards_dir.display()))?;
        let path = entry.path();
        if path.is_file()
            && path
                .file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.ends_with(PROJECT_BOARD_FILE_SUFFIX))
        {
            count += 1;
        }
    }

    Ok(count)
}

fn list_project_board_file_paths(boards_dir: &Path) -> Result<Vec<PathBuf>, String> {
    if !boards_dir.exists() {
        return Ok(Vec::new());
    }

    let entries = std::fs::read_dir(boards_dir)
        .map_err(|error| format!("读取看板目录失败 `{}`: {error}", boards_dir.display()))?;
    let mut paths = Vec::new();
    for entry in entries {
        let entry = entry
            .map_err(|error| format!("读取看板目录条目失败 `{}`: {error}", boards_dir.display()))?;
        let path = entry.path();
        if path.is_file()
            && path
                .file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.ends_with(PROJECT_BOARD_FILE_SUFFIX))
        {
            paths.push(path);
        }
    }

    paths.sort_by(|left, right| left.file_name().cmp(&right.file_name()));
    Ok(paths)
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

fn normalize_sql_writer_paths(
    graph: &mut WorkflowGraph,
    workspace_dir: &Path,
) -> Result<(), EngineError> {
    for node_definition in graph.nodes.values_mut() {
        if node_definition.node_type() != "sqlWriter" && node_definition.node_type() != "sql/writer"
        {
            continue;
        }

        let node_id = node_definition.id().to_owned();
        let Some(config_map) = node_definition.config_mut().as_object_mut() else {
            continue;
        };

        let raw_database_path = config_map
            .get("database_path")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .unwrap_or(SQL_WRITER_DEFAULT_DATABASE_PATH)
            .to_owned();

        let resolved_path =
            normalize_sql_writer_database_path(&raw_database_path, workspace_dir, &node_id)?;
        config_map.insert(
            "database_path".to_owned(),
            Value::String(resolved_path.to_string_lossy().to_string()),
        );
    }

    Ok(())
}

fn normalize_sql_writer_database_path(
    raw_path: &str,
    workspace_dir: &Path,
    node_id: &str,
) -> Result<PathBuf, EngineError> {
    let path = Path::new(raw_path);
    if path.is_absolute() {
        if path
            .components()
            .any(|component| component == Component::ParentDir)
        {
            return Err(EngineError::node_config(
                node_id.to_owned(),
                "database_path 不允许包含路径穿越（..）",
            ));
        }

        if !path.starts_with(workspace_dir) {
            return Err(EngineError::node_config(
                node_id.to_owned(),
                "database_path 需要位于当前工作目录内",
            ));
        }

        return Ok(path.to_path_buf());
    }

    Ok(workspace_dir.join(sanitize_relative_path(raw_path)))
}

fn sanitize_relative_path(raw_path: &str) -> PathBuf {
    let mut sanitized = PathBuf::new();

    for component in Path::new(raw_path).components() {
        match component {
            Component::Normal(segment) => sanitized.push(segment),
            Component::CurDir
            | Component::ParentDir
            | Component::RootDir
            | Component::Prefix(_) => {}
        }
    }

    if sanitized.as_os_str().is_empty() {
        sanitized.push("nazh-local.sqlite3");
    }

    sanitized
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

#[cfg(target_os = "macos")]
fn apply_window_glass(window: &tauri::WebviewWindow) {
    if let Err(error) = apply_vibrancy(window, NSVisualEffectMaterial::HudWindow, None, Some(16.0))
    {
        tracing::warn!("应用 macOS 窗口玻璃效果失败: {error}");
    }
}

#[cfg(target_os = "windows")]
fn apply_window_glass(window: &tauri::WebviewWindow) {
    if let Err(error) = apply_blur(window, Some((18, 18, 18, 125))) {
        tracing::warn!("应用 Windows 窗口模糊效果失败: {error}");
    }
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
fn apply_window_glass(_window: &tauri::WebviewWindow) {}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    init_tracing();

    let builder = tauri::Builder::default()
        .manage(DesktopState::default())
        .setup(|app| {
            if let Some(window) = app.get_webview_window("main") {
                apply_window_glass(&window);
            } else {
                tracing::warn!("未找到主窗口，跳过玻璃效果初始化");
            }

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
                if let Ok(path) = DesktopState::ai_config_file_path(&app_handle)
                    && path.exists()
                    && let Ok(text) = tokio::fs::read_to_string(&path).await
                    && let Ok(mut file_config) = serde_json::from_str::<AiConfigFile>(&text)
                {
                    file_config.normalize();
                    let mut config = ai_config_arc.write().await;
                    *config = file_config;
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
            describe_node_pins,
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
            load_project_board_files,
            save_project_board_files,
            save_flowgram_export_file,
            load_ai_config,
            save_ai_config,
            test_ai_provider,
            copilot_complete,
            copilot_complete_stream
        ]);

    if let Err(error) = builder.run(tauri::generate_context!()) {
        tracing::error!("Nazh 桌面壳层运行失败: {error}");
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::{normalize_sql_writer_database_path, sanitize_relative_path};
    use nazh_engine::EngineError;
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

    #[test]
    fn sql_writer_relative_path_resolves_inside_workspace() {
        let workspace = PathBuf::from("/tmp/nazh-workspace");
        let normalized =
            normalize_sql_writer_database_path("./data/edge-runtime.sqlite3", &workspace, "sql_1")
                .unwrap();

        assert_eq!(normalized, workspace.join("data/edge-runtime.sqlite3"));
    }

    #[test]
    fn sql_writer_escape_segments_stay_inside_workspace() {
        let workspace = PathBuf::from("/tmp/nazh-workspace");
        let normalized =
            normalize_sql_writer_database_path("../audit.sqlite3", &workspace, "sql_1").unwrap();

        assert_eq!(normalized, workspace.join("audit.sqlite3"));
    }

    #[test]
    fn sql_writer_absolute_path_inside_workspace_is_allowed() {
        let workspace = PathBuf::from("/tmp/nazh-workspace");
        let normalized = normalize_sql_writer_database_path(
            "/tmp/nazh-workspace/data/audit.sqlite3",
            &workspace,
            "sql_1",
        )
        .unwrap();

        assert_eq!(normalized, workspace.join("data/audit.sqlite3"));
    }

    #[test]
    fn sql_writer_absolute_path_outside_workspace_is_rejected() {
        let workspace = PathBuf::from("/tmp/nazh-workspace");
        let error = normalize_sql_writer_database_path("/tmp/audit.sqlite3", &workspace, "sql_1")
            .unwrap_err();

        assert!(matches!(
            error,
            EngineError::NodeConfig { node_id, message }
                if node_id == "sql_1" && message.contains("工作目录")
        ));
    }
}
