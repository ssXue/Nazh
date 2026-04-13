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

use nazh_engine::{
    deploy_workflow as deploy_workflow_graph, shared_connection_manager, ConnectionDefinition,
    ConnectionRecord, DeployResponse, DispatchResponse, EngineError, ExecutionEvent,
    SerialTriggerNodeConfig, TimerNodeConfig, UndeployResponse, WorkflowContext, WorkflowGraph,
    WorkflowIngress,
};
use observability::{
    query_observability as query_workspace_observability, ObservabilityContextInput,
    ObservabilityQueryResult, ObservabilityStore, SharedObservabilityStore,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tauri::{AppHandle, Emitter, Manager, State};
use tokio::sync::Mutex;
use tokio::fs;

use std::{
    io::Read,
    path::{Component, Path, PathBuf},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::{Duration, Instant},
};

/// IPC 命令输入的最大允许字节数（10 MB）。
const MAX_IPC_INPUT_BYTES: usize = 10 * 1024 * 1024;

/// 已部署工作流的运行时包装，包含入口句柄和根触发任务。
struct DesktopWorkflow {
    ingress: WorkflowIngress,
    trigger_tasks: Vec<DesktopTriggerTask>,
    forwarding_tasks: Vec<tauri::async_runtime::JoinHandle<()>>,
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
        for task in &self.forwarding_tasks {
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
}

/// Tauri 托管的应用状态，持有连接池和当前活跃的工作流。
struct DesktopState {
    connection_manager: nazh_engine::SharedConnectionManager,
    workflow: Mutex<Option<DesktopWorkflow>>,
    observability: Mutex<Option<SharedObservabilityStore>>,
}

impl Default for DesktopState {
    fn default() -> Self {
        Self {
            connection_manager: shared_connection_manager(),
            workflow: Mutex::new(None),
            observability: Mutex::new(None),
        }
    }
}

impl DesktopState {
    fn connections_file_path(
        app: &AppHandle,
        workspace_path: Option<&str>,
    ) -> Result<PathBuf, String> {
        let workspace_dir = resolve_project_workspace_dir(app, workspace_path)
            .map(|(dir, _)| dir)
            .map_err(|e| e)?;
        Ok(workspace_dir.join("connections.json"))
    }

    fn deployment_session_file_path(
        app: &AppHandle,
        workspace_path: Option<&str>,
    ) -> Result<PathBuf, String> {
        let workspace_dir = resolve_project_workspace_dir(app, workspace_path)
            .map(|(dir, _)| dir)
            .map_err(|e| e)?;
        Ok(workspace_dir.join("deployment-session.json"))
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
                        if let Ok(defs) = serde_json::from_str::<Vec<nazh_engine::ConnectionDefinition>>(&text) {
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

#[tauri::command]
async fn deploy_workflow(
    app: AppHandle,
    state: State<'_, DesktopState>,
    ast: String,
    connection_definitions: Option<Vec<ConnectionDefinition>>,
    observability_context: Option<ObservabilityContextInput>,
) -> Result<DeployResponse, String> {
    if ast.len() > MAX_IPC_INPUT_BYTES {
        return Err("AST 超过最大允许大小（10 MB）".to_owned());
    }
    let mut graph = WorkflowGraph::from_json(&ast).map_err(stringify_error)?;
    normalize_sql_writer_paths(&app, &mut graph).map_err(stringify_error)?;
    if let Some(definitions) = connection_definitions {
        state.connection_manager.replace_connections(definitions).await;
    }
    let observability_store = if let Some(context) = observability_context.clone() {
        let (workspace_dir, _) =
            resolve_project_workspace_dir(&app, Some(context.workspace_path.as_str()))?;
        let store = ObservabilityStore::new(workspace_dir, context).await?;
        let _ = store
            .record_audit(
                "info",
                "workflow",
                "收到部署请求",
                observability_context.as_ref().map(|context| {
                    format!("{} · {}", context.project_name, context.environment_name)
                }),
                None,
                None,
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
    let deployment = match deploy_workflow_graph(graph, state.connection_manager.clone()).await {
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
    let (mut event_rx, mut result_rx) = streams.into_receivers();

    let mut trigger_tasks = spawn_timer_root_tasks(ingress.clone(), timer_roots);
    trigger_tasks.extend(spawn_serial_root_tasks(
        app.clone(),
        ingress.clone(),
        state.connection_manager.clone(),
        observability_store.clone(),
        serial_roots,
    ));

    {
        let mut workflow_guard = state.workflow.lock().await;
        if let Some(mut existing) = workflow_guard.take() {
            existing.abort_triggers().await;
        }

        let mut forwarding_tasks = Vec::new();

        let event_app = app.clone();
        let event_store = observability_store.clone();
        forwarding_tasks.push(tauri::async_runtime::spawn(async move {
            while let Some(event) = event_rx.recv().await {
                if let Some(store) = &event_store {
                    let _ = store.record_execution_event(&event).await;
                }
                let _ = event_app.emit("workflow://node-status", event);
            }
        }));

        let result_app = app.clone();
        let result_store = observability_store.clone();
        forwarding_tasks.push(tauri::async_runtime::spawn(async move {
            while let Some(result) = result_rx.recv().await {
                if let Some(store) = &result_store {
                    let _ = store.record_result(&result).await;
                }
                let _ = result_app.emit("workflow://result", result);
            }
        }));

        *workflow_guard = Some(DesktopWorkflow {
            ingress,
            trigger_tasks,
            forwarding_tasks,
        });
    }
    {
        let mut observability_guard = state.observability.lock().await;
        *observability_guard = observability_store.clone();
    }

    let deploy_payload = DeployResponse {
        node_count,
        edge_count,
        root_nodes,
    };
    if let Some(store) = &observability_store {
        let _ = store
            .record_audit(
                "success",
                "workflow",
                "部署完成",
                Some(format!("节点 {} / 连线 {}", node_count, edge_count)),
                None,
                Some(json!({
                    "node_count": node_count,
                    "edge_count": edge_count,
                    "root_nodes": deploy_payload.root_nodes.clone(),
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
) -> Result<DispatchResponse, String> {
    let observability_store = {
        let observability_guard = state.observability.lock().await;
        observability_guard.clone()
    };
    let ingress = {
        let workflow_guard = state.workflow.lock().await;
        workflow_guard
            .as_ref()
            .map(|workflow| workflow.ingress.clone())
            .ok_or_else(|| stringify_error(EngineError::WorkflowUnavailable))?
    };

    let ctx = WorkflowContext::new(payload);
    let trace_id = ctx.trace_id.to_string();
    if let Err(error) = ingress.submit(ctx).await {
        if let Some(store) = &observability_store {
            let _ = store
                .record_audit(
                    "error",
                    "dispatch",
                    "提交测试载荷失败",
                    Some(error.to_string()),
                    Some(trace_id.clone()),
                    None,
                )
                .await;
        }
        return Err(stringify_error(error));
    }

    if let Some(store) = &observability_store {
        let _ = store
            .record_audit(
                "info",
                "dispatch",
                "已提交测试载荷",
                Some(format!("trace_id={trace_id}")),
                Some(trace_id.clone()),
                None,
            )
            .await;
    }
    Ok(DispatchResponse { trace_id })
}

#[tauri::command]
async fn undeploy_workflow(
    app: AppHandle,
    state: State<'_, DesktopState>,
) -> Result<UndeployResponse, String> {
    let observability_store = {
        let observability_guard = state.observability.lock().await;
        observability_guard.clone()
    };
    let existing_workflow = {
        let mut workflow_guard = state.workflow.lock().await;
        workflow_guard.take()
    };

    let response = if let Some(mut workflow) = existing_workflow {
        UndeployResponse {
            had_workflow: true,
            aborted_timer_count: workflow.abort_triggers().await,
        }
    } else {
        UndeployResponse {
            had_workflow: false,
            aborted_timer_count: 0,
        }
    };

    state
        .connection_manager
        .mark_all_idle("运行已停止，连接会话已回收到空闲态")
        .await;
    if let Some(store) = &observability_store {
        let _ = store
            .record_audit(
                if response.had_workflow { "warn" } else { "info" },
                "workflow",
                if response.had_workflow {
                    "运行已停止"
                } else {
                    "停止请求未命中已部署工作流"
                },
                Some(format!(
                    "已中止 {} 个根触发任务",
                    response.aborted_timer_count
                )),
                None,
                None,
            )
            .await;
    }
    {
        let mut observability_guard = state.observability.lock().await;
        *observability_guard = None;
    }

    let _ = app.emit("workflow://undeployed", response.clone());
    Ok(response)
}

#[tauri::command]
async fn list_connections(state: State<'_, DesktopState>) -> Result<Vec<ConnectionRecord>, String> {
    let connections = state.connection_manager.list().await;
    Ok(connections)
}

#[tauri::command]
async fn query_observability(
    app: AppHandle,
    workspace_path: Option<String>,
    trace_id: Option<String>,
    search: Option<String>,
    limit: Option<usize>,
) -> Result<ObservabilityQueryResult, String> {
    let (workspace_dir, _) =
        resolve_project_workspace_dir(&app, workspace_path.as_deref()).map_err(|error| error)?;
    query_workspace_observability(workspace_dir, trace_id, search, limit.unwrap_or(240)).await
}

#[tauri::command]
async fn load_connection_definitions(
    app: AppHandle,
    state: State<'_, DesktopState>,
    workspace_path: Option<String>,
) -> Result<Vec<ConnectionDefinition>, String> {
    let path =
        DesktopState::connections_file_path(&app, workspace_path.as_deref()).map_err(|e| e)?;
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
    let path =
        DesktopState::connections_file_path(&app, workspace_path.as_deref()).map_err(|e| e)?;
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
    let path =
        DesktopState::deployment_session_file_path(&app, workspace_path.as_deref()).map_err(|e| e)?;

    if !path.exists() {
        return Ok(None);
    }

    let text = fs::read_to_string(&path)
        .await
        .map_err(|error| format!("读取 deployment-session.json 失败: {error}"))?;
    let session = serde_json::from_str::<PersistedDeploymentSession>(&text)
        .map_err(|error| format!("解析 deployment-session.json 失败: {error}"))?;
    Ok(Some(session))
}

#[tauri::command]
async fn save_deployment_session_file(
    app: AppHandle,
    workspace_path: Option<String>,
    session: PersistedDeploymentSession,
) -> Result<(), String> {
    let path =
        DesktopState::deployment_session_file_path(&app, workspace_path.as_deref()).map_err(|e| e)?;
    let dir = path.parent().ok_or("无法确定部署会话文件目录")?;
    fs::create_dir_all(dir)
        .await
        .map_err(|error| format!("创建部署会话目录失败: {error}"))?;
    let text = serde_json::to_string_pretty(&session)
        .map_err(|error| format!("序列化部署会话失败: {error}"))?;
    fs::write(&path, text)
        .await
        .map_err(|error| format!("写入 deployment-session.json 失败: {error}"))?;
    Ok(())
}

#[tauri::command]
async fn clear_deployment_session_file(
    app: AppHandle,
    workspace_path: Option<String>,
) -> Result<(), String> {
    let path =
        DesktopState::deployment_session_file_path(&app, workspace_path.as_deref()).map_err(|e| e)?;

    if !path.exists() {
        return Ok(());
    }

    fs::remove_file(&path)
        .await
        .map_err(|error| format!("删除 deployment-session.json 失败: {error}"))?;
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
    let ports = serialport::available_ports()
        .map_err(|e| format!("枚举串口失败: {e}"))?;

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
            message: format!("端口 {} 打开成功", port_path),
        }),
        Err(error) => Ok(TestSerialResult {
            ok: false,
            message: format!("端口 {} 打开失败: {}", port_path, error),
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

fn count_incoming_edges(
    graph: &WorkflowGraph,
) -> std::collections::HashMap<String, usize> {
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
        let connection = connection_manager
            .get(connection_id)
            .await
            .ok_or_else(|| {
                EngineError::node_config(
                    node_id.clone(),
                    format!("串口连接资源 `{connection_id}` 未注册"),
                )
            })?;

        if !is_serial_connection_type(&connection.kind) {
            let _ = connection_manager
                .mark_invalid_configuration(
                    connection_id,
                    format!("连接资源 `{connection_id}` 不是串口类型"),
                )
                .await;
            return Err(EngineError::node_config(
                node_id.clone(),
                format!("连接资源 `{connection_id}` 不是串口类型"),
            ));
        }

        let mut config: SerialTriggerNodeConfig =
            serde_json::from_value(connection.metadata).map_err(|error| {
                EngineError::node_config(node_definition.id.clone(), error.to_string())
            })?;
        if let Some(inject) = node_definition.config.get("inject").and_then(Value::as_object) {
            config.inject.clone_from(inject);
        }
        config.port_path = config.port_path.trim().to_owned();

        if config.port_path.is_empty() {
            let _ = connection_manager
                .mark_invalid_configuration(
                    connection_id,
                    format!("串口连接资源 `{connection_id}` 需要配置 port_path"),
                )
                .await;
            return Err(EngineError::node_config(
                node_id.clone(),
                format!("串口连接资源 `{connection_id}` 需要配置 port_path"),
            ));
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
    ingress: WorkflowIngress,
    timer_roots: Vec<TimerRootSpec>,
) -> Vec<DesktopTriggerTask> {
    timer_roots
        .into_iter()
        .map(|timer_root| {
            let ingress = ingress.clone();
            let cancel = Arc::new(AtomicBool::new(false));
            let task_cancel = Arc::clone(&cancel);
            let join = tauri::async_runtime::spawn(async move {
                if timer_root.immediate && !task_cancel.load(Ordering::Relaxed) {
                    let _ = ingress
                        .submit_to(
                            &timer_root.node_id,
                            WorkflowContext::new(Value::Object(Default::default())),
                        )
                        .await;
                }

                let delay = Duration::from_millis(timer_root.interval_ms);

                loop {
                    tokio::time::sleep(delay).await;
                    if task_cancel.load(Ordering::Relaxed) {
                        break;
                    }
                    let _ = ingress
                        .submit_to(
                            &timer_root.node_id,
                            WorkflowContext::new(Value::Object(Default::default())),
                        )
                        .await;
                }
            });

            DesktopTriggerTask { cancel, join: TriggerJoinHandle::Async(join) }
        })
        .collect()
}

fn spawn_serial_root_tasks(
    app: AppHandle,
    ingress: WorkflowIngress,
    connection_manager: nazh_engine::SharedConnectionManager,
    observability: Option<SharedObservabilityStore>,
    serial_roots: Vec<SerialRootSpec>,
) -> Vec<DesktopTriggerTask> {
    serial_roots
        .into_iter()
        .map(|serial_root| {
            let app = app.clone();
            let ingress = ingress.clone();
            let connection_manager = connection_manager.clone();
            let observability = observability.clone();
            let cancel = Arc::new(AtomicBool::new(false));
            let task_cancel = Arc::clone(&cancel);
            let join = std::thread::spawn(move || {
                run_serial_root_reader(
                    app,
                    ingress,
                    connection_manager,
                    observability,
                    serial_root,
                    task_cancel,
                );
            });

            DesktopTriggerTask { cancel, join: TriggerJoinHandle::Thread(join) }
        })
        .collect()
}

fn run_serial_root_reader(
    app: AppHandle,
    ingress: WorkflowIngress,
    connection_manager: nazh_engine::SharedConnectionManager,
    observability: Option<SharedObservabilityStore>,
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
                    format!(
                        "串口 {} 已建立监听，等待外设上报数据",
                        config.port_path
                    ),
                    Some(connect_latency_ms),
                ));
                port
            }
            Err(error) => {
                let reason = format!("串口打开失败: {error}");
                let retry_after_ms = tauri::async_runtime::block_on(
                    connection_manager.record_connect_failure(
                        &serial_root.connection_id,
                        reason.clone(),
                    ),
                )
                .unwrap_or(800);
                let _ = tauri::async_runtime::block_on(
                    connection_manager.release(&serial_root.connection_id),
                );
                emit_serial_trigger_failure(
                    &app,
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
                        &ingress,
                        &serial_root,
                        observability.as_ref(),
                        &mut buffer,
                        last_byte_at,
                        idle_gap,
                    );

                    if last_heartbeat_sent_at.elapsed() >= heartbeat_interval {
                        let _ = tauri::async_runtime::block_on(connection_manager.record_heartbeat(
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
                        submit_serial_frame(&app, &ingress, &serial_root, observability.as_ref(), &frame);
                    }

                    if buffer.len() >= max_frame_bytes {
                        let frame = buffer.drain(..max_frame_bytes).collect::<Vec<_>>();
                        submit_serial_frame(&app, &ingress, &serial_root, observability.as_ref(), &frame);
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
                        &ingress,
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
            submit_serial_frame(&app, &ingress, &serial_root, observability.as_ref(), &buffer);
        }

        if cancel.load(Ordering::Relaxed) {
            let _ = tauri::async_runtime::block_on(
                connection_manager.release(&serial_root.connection_id),
            );
            let _ = tauri::async_runtime::block_on(connection_manager.mark_disconnected(
                &serial_root.connection_id,
                format!("串口 {} 监听已停止", config.port_path),
            ));
            break;
        }

        let reason = disconnected_reason
            .unwrap_or_else(|| format!("串口 {} 连接已断开", config.port_path));
        let retry_after_ms = tauri::async_runtime::block_on(connection_manager.record_connect_failure(
            &serial_root.connection_id,
            reason.clone(),
        ))
        .unwrap_or(800);
        let _ = tauri::async_runtime::block_on(connection_manager.release(
            &serial_root.connection_id,
        ));
        emit_serial_trigger_failure(
            &app,
            observability.as_ref(),
            &serial_root.node_id,
            reason,
        );
        sleep_with_cancel(&cancel, Duration::from_millis(retry_after_ms));
    }
}

fn flush_idle_serial_frame(
    app: &AppHandle,
    ingress: &WorkflowIngress,
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
        submit_serial_frame(app, ingress, serial_root, observability, &frame);
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
    ingress: &WorkflowIngress,
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

    if let Err(error) =
        ingress.blocking_submit_to(&serial_root.node_id, WorkflowContext::new(payload))
    {
        emit_serial_trigger_failure(app, observability, &serial_root.node_id, error.to_string());
    }
}

fn emit_serial_trigger_failure(
    app: &AppHandle,
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
    let _ = app.emit(
        "workflow://node-status",
        ExecutionEvent::Failed {
            stage: node_id.to_owned(),
            trace_id: context.trace_id,
            error: message,
        },
    );
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
        std::thread::sleep(Duration::from_millis(100).min(duration.saturating_sub(start.elapsed())));
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
    if let Some(hex) = trimmed.strip_prefix("hex:").or_else(|| trimmed.strip_prefix("0x")) {
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
    let nibbles = value
        .bytes()
        .filter_map(hex_nibble)
        .collect::<Vec<_>>();
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

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let builder = tauri::Builder::default()
        .manage(DesktopState::default())
        .setup(|app| {
            let app_handle = app.handle().clone();
            let state: State<'_, DesktopState> = app.state();
            let manager = state.connection_manager.clone();
            tauri::async_runtime::spawn(async move {
                DesktopState::load_connections_from_disk(&app_handle, manager, None).await;
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            deploy_workflow,
            dispatch_payload,
            undeploy_workflow,
            list_connections,
            query_observability,
            load_connection_definitions,
            save_connection_definitions,
            load_deployment_session_file,
            save_deployment_session_file,
            clear_deployment_session_file,
            list_serial_ports,
            test_serial_connection,
            load_project_library_file,
            save_project_library_file
        ]);

    if let Err(error) = builder.run(tauri::generate_context!()) {
        eprintln!("failed to run Nazh desktop shell: {error}");
    }
}
