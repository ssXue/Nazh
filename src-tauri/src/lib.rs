//! Nazh 的 Tauri 桌面壳层。
//!
//! 向 React 前端暴露四个 IPC 命令：
//! - [`deploy_workflow`] — 解析并部署工作流 DAG。
//! - [`dispatch_payload`] — 向运行中的工作流提交测试载荷。
//! - [`undeploy_workflow`] — 停止当前工作流并中止定时任务。
//! - [`list_connections`] — 获取连接池快照。
//!
//! 引擎事件通过 `Window::emit` 转发给前端。

use nazh_engine::{
    deploy_workflow as deploy_workflow_graph, shared_connection_manager, ConnectionRecord,
    DeployResponse, DispatchResponse, EngineError, ExecutionEvent, SerialTriggerNodeConfig,
    TimerNodeConfig, UndeployResponse, WorkflowContext, WorkflowGraph, WorkflowIngress,
};
use serde::Serialize;
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

/// 已部署工作流的运行时包装，包含入口句柄和根触发任务。
struct DesktopWorkflow {
    ingress: WorkflowIngress,
    trigger_tasks: Vec<DesktopTriggerTask>,
}

struct DesktopTriggerTask {
    cancel: Arc<AtomicBool>,
    join: tauri::async_runtime::JoinHandle<()>,
}

impl DesktopWorkflow {
    /// 中止所有根触发任务，返回中止数量。
    async fn abort_triggers(&mut self) -> usize {
        let tasks = self.trigger_tasks.drain(..).collect::<Vec<_>>();
        let aborted = tasks.len();

        for task in &tasks {
            task.cancel.store(true, Ordering::Relaxed);
            task.join.abort();
        }

        for task in tasks {
            let _ = task.join.await;
        }

        aborted
    }
}

/// Tauri 托管的应用状态，持有连接池和当前活跃的工作流。
struct DesktopState {
    connection_manager: nazh_engine::SharedConnectionManager,
    workflow: Mutex<Option<DesktopWorkflow>>,
}

impl Default for DesktopState {
    fn default() -> Self {
        Self {
            connection_manager: shared_connection_manager(),
            workflow: Mutex::new(None),
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

#[tauri::command]
async fn deploy_workflow(
    app: AppHandle,
    state: State<'_, DesktopState>,
    ast: String,
) -> Result<DeployResponse, String> {
    let mut graph = WorkflowGraph::from_json(&ast).map_err(stringify_error)?;
    normalize_sql_writer_paths(&app, &mut graph).map_err(stringify_error)?;
    let timer_roots = collect_timer_root_specs(&graph).map_err(stringify_error)?;
    let serial_roots = collect_serial_root_specs(&graph).map_err(stringify_error)?;
    let node_count = graph.nodes.len();
    let edge_count = graph.edges.len();
    let deployment = deploy_workflow_graph(graph, state.connection_manager.clone())
        .await
        .map_err(stringify_error)?;
    let (ingress, streams) = deployment.into_parts();
    let root_nodes = ingress.root_nodes().to_vec();
    let (mut event_rx, mut result_rx) = streams.into_receivers();

    let existing_workflow = {
        let mut workflow_guard = state.workflow.lock().await;
        workflow_guard.take()
    };

    if let Some(mut existing) = existing_workflow {
        existing.abort_triggers().await;
    }

    let mut trigger_tasks = spawn_timer_root_tasks(ingress.clone(), timer_roots);
    trigger_tasks.extend(spawn_serial_root_tasks(
        app.clone(),
        ingress.clone(),
        serial_roots,
    ));

    {
        let mut workflow_guard = state.workflow.lock().await;
        *workflow_guard = Some(DesktopWorkflow {
            ingress,
            trigger_tasks,
        });
    }

    let event_app = app.clone();
    tauri::async_runtime::spawn(async move {
        while let Some(event) = event_rx.recv().await {
            let _ = event_app.emit("workflow://node-status", event);
        }
    });

    let result_app = app.clone();
    tauri::async_runtime::spawn(async move {
        while let Some(result) = result_rx.recv().await {
            let _ = result_app.emit("workflow://result", result);
        }
    });

    let deploy_payload = DeployResponse {
        node_count,
        edge_count,
        root_nodes,
    };
    let _ = app.emit("workflow://deployed", deploy_payload.clone());
    Ok(deploy_payload)
}

#[tauri::command]
async fn dispatch_payload(
    state: State<'_, DesktopState>,
    payload: Value,
) -> Result<DispatchResponse, String> {
    let ingress = {
        let workflow_guard = state.workflow.lock().await;
        workflow_guard
            .as_ref()
            .map(|workflow| workflow.ingress.clone())
            .ok_or_else(|| stringify_error(EngineError::WorkflowUnavailable))?
    };

    let ctx = WorkflowContext::new(payload);
    let trace_id = ctx.trace_id.to_string();
    ingress.submit(ctx).await.map_err(stringify_error)?;
    Ok(DispatchResponse { trace_id })
}

#[tauri::command]
async fn undeploy_workflow(
    app: AppHandle,
    state: State<'_, DesktopState>,
) -> Result<UndeployResponse, String> {
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

    let _ = app.emit("workflow://undeployed", response.clone());
    Ok(response)
}

#[tauri::command]
async fn list_connections(state: State<'_, DesktopState>) -> Result<Vec<ConnectionRecord>, String> {
    let connections = state.connection_manager.list().await;
    Ok(connections)
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

fn collect_serial_root_specs(graph: &WorkflowGraph) -> Result<Vec<SerialRootSpec>, EngineError> {
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
        let connection = graph
            .connections
            .iter()
            .find(|connection| connection.id == connection_id)
            .ok_or_else(|| {
                EngineError::node_config(
                    node_id.clone(),
                    format!("串口连接资源 `{connection_id}` 未注册"),
                )
            })?;

        if !is_serial_connection_type(&connection.kind) {
            return Err(EngineError::node_config(
                node_id.clone(),
                format!("连接资源 `{connection_id}` 不是串口类型"),
            ));
        }

        let mut config: SerialTriggerNodeConfig =
            serde_json::from_value(connection.metadata.clone()).map_err(|error| {
                EngineError::node_config(node_definition.id.clone(), error.to_string())
            })?;
        if let Some(inject) = node_definition.config.get("inject").and_then(Value::as_object) {
            config.inject.clone_from(inject);
        }
        config.port_path = config.port_path.trim().to_owned();

        if config.port_path.is_empty() {
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

                let mut interval =
                    tokio::time::interval(std::time::Duration::from_millis(timer_root.interval_ms));
                let _ = interval.tick().await;

                loop {
                    let _ = interval.tick().await;
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

            DesktopTriggerTask { cancel, join }
        })
        .collect()
}

fn spawn_serial_root_tasks(
    app: AppHandle,
    ingress: WorkflowIngress,
    serial_roots: Vec<SerialRootSpec>,
) -> Vec<DesktopTriggerTask> {
    serial_roots
        .into_iter()
        .map(|serial_root| {
            let app = app.clone();
            let ingress = ingress.clone();
            let cancel = Arc::new(AtomicBool::new(false));
            let task_cancel = Arc::clone(&cancel);
            let join = tauri::async_runtime::spawn_blocking(move || {
                run_serial_root_reader(app, ingress, serial_root, task_cancel);
            });

            DesktopTriggerTask { cancel, join }
        })
        .collect()
}

fn run_serial_root_reader(
    app: AppHandle,
    ingress: WorkflowIngress,
    serial_root: SerialRootSpec,
    cancel: Arc<AtomicBool>,
) {
    let config = serial_root.config.clone();
    let read_timeout = Duration::from_millis(config.read_timeout_ms.clamp(10, 2_000));
    let idle_gap = Duration::from_millis(config.idle_gap_ms.clamp(1, 10_000));
    let max_frame_bytes = config.max_frame_bytes.clamp(1, 8_192);
    let delimiter = decode_serial_delimiter(&config.delimiter);
    let port_result = serialport::new(config.port_path.clone(), config.baud_rate.max(1))
        .timeout(read_timeout)
        .data_bits(serial_data_bits(config.data_bits))
        .parity(serial_parity(&config.parity))
        .stop_bits(serial_stop_bits(config.stop_bits))
        .flow_control(serial_flow_control(&config.flow_control))
        .open();
    let mut port = match port_result {
        Ok(port) => port,
        Err(error) => {
            emit_serial_trigger_failure(
                &app,
                &serial_root.node_id,
                format!("串口打开失败: {error}"),
            );
            return;
        }
    };

    let mut buffer = Vec::with_capacity(max_frame_bytes.min(512));
    let mut scratch = [0_u8; 64];
    let mut last_byte_at: Option<Instant> = None;

    while !cancel.load(Ordering::Relaxed) {
        match port.read(&mut scratch) {
            Ok(0) => flush_idle_serial_frame(
                &app,
                &ingress,
                &serial_root,
                &mut buffer,
                last_byte_at,
                idle_gap,
            ),
            Ok(bytes_read) => {
                buffer.extend_from_slice(&scratch[..bytes_read]);
                last_byte_at = Some(Instant::now());

                while let Some(frame) = drain_serial_delimited_frame(&mut buffer, &delimiter) {
                    submit_serial_frame(&app, &ingress, &serial_root, &frame);
                }

                if buffer.len() >= max_frame_bytes {
                    let frame = buffer.drain(..max_frame_bytes).collect::<Vec<_>>();
                    submit_serial_frame(&app, &ingress, &serial_root, &frame);
                }
            }
            Err(error) if error.kind() == std::io::ErrorKind::TimedOut => flush_idle_serial_frame(
                &app,
                &ingress,
                &serial_root,
                &mut buffer,
                last_byte_at,
                idle_gap,
            ),
            Err(error) => {
                emit_serial_trigger_failure(
                    &app,
                    &serial_root.node_id,
                    format!("串口读取失败: {error}"),
                );
                break;
            }
        }
    }

    if !cancel.load(Ordering::Relaxed) && !buffer.is_empty() {
        submit_serial_frame(&app, &ingress, &serial_root, &buffer);
    }
}

fn flush_idle_serial_frame(
    app: &AppHandle,
    ingress: &WorkflowIngress,
    serial_root: &SerialRootSpec,
    buffer: &mut Vec<u8>,
    last_byte_at: Option<Instant>,
    idle_gap: Duration,
) {
    if buffer.is_empty() {
        return;
    }

    if last_byte_at.is_some_and(|instant| instant.elapsed() >= idle_gap) {
        let frame = std::mem::take(buffer);
        submit_serial_frame(app, ingress, serial_root, &frame);
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
        emit_serial_trigger_failure(app, &serial_root.node_id, error.to_string());
    }
}

fn emit_serial_trigger_failure(app: &AppHandle, node_id: &str, message: String) {
    let context = WorkflowContext::new(Value::Object(Default::default()));
    let _ = app.emit(
        "workflow://node-status",
        ExecutionEvent::Failed {
            stage: node_id.to_owned(),
            trace_id: context.trace_id,
            error: message,
        },
    );
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
        .invoke_handler(tauri::generate_handler![
            deploy_workflow,
            dispatch_payload,
            undeploy_workflow,
            list_connections,
            load_project_library_file,
            save_project_library_file
        ]);

    if let Err(error) = builder.run(tauri::generate_context!()) {
        eprintln!("failed to run Nazh desktop shell: {error}");
    }
}
