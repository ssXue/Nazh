//! Nazh 的 Tauri 桌面壳层。
//!
//! 向 React 前端暴露三个 IPC 命令：
//! - [`deploy_workflow`] — 解析并部署工作流 DAG。
//! - [`dispatch_payload`] — 向运行中的工作流提交测试载荷。
//! - [`list_connections`] — 获取连接池快照。
//!
//! 引擎事件通过 `Window::emit` 转发给前端。

use nazh_engine::{
    deploy_workflow as deploy_workflow_graph, shared_connection_manager, ConnectionRecord,
    EngineError, TimerNodeConfig, WorkflowContext, WorkflowGraph, WorkflowIngress,
};
use serde::Serialize;
use serde_json::Value;
use tauri::{AppHandle, Emitter, Manager, State};
use tokio::sync::Mutex;

use std::path::{Component, Path, PathBuf};

/// 已部署工作流的运行时包装，包含入口句柄和定时器任务。
struct DesktopWorkflow {
    ingress: WorkflowIngress,
    timer_tasks: Vec<tauri::async_runtime::JoinHandle<()>>,
}

impl DesktopWorkflow {
    /// 中止所有定时器任务，返回中止数量。
    fn abort_timers(&mut self) -> usize {
        let aborted = self.timer_tasks.len();
        for task in self.timer_tasks.drain(..) {
            task.abort();
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

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct DeployResponse {
    node_count: usize,
    edge_count: usize,
    root_nodes: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct DispatchResponse {
    trace_id: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct UndeployResponse {
    had_workflow: bool,
    aborted_timer_count: usize,
}

#[derive(Debug, Clone)]
struct TimerRootSpec {
    node_id: String,
    interval_ms: u64,
    immediate: bool,
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
    let node_count = graph.nodes.len();
    let edge_count = graph.edges.len();
    let deployment = deploy_workflow_graph(graph, state.connection_manager.clone())
        .await
        .map_err(stringify_error)?;
    let (ingress, streams) = deployment.into_parts();
    let root_nodes = ingress.root_nodes().to_vec();
    let (mut event_rx, mut result_rx) = streams.into_receivers();
    let timer_tasks = spawn_timer_root_tasks(ingress.clone(), timer_roots);

    {
        let mut workflow_guard = state.workflow.lock().await;
        if let Some(existing) = workflow_guard.as_mut() {
            existing.abort_timers();
        }
        *workflow_guard = Some(DesktopWorkflow {
            ingress,
            timer_tasks,
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
    let response = {
        let mut workflow_guard = state.workflow.lock().await;

        if let Some(mut workflow) = workflow_guard.take() {
            UndeployResponse {
                had_workflow: true,
                aborted_timer_count: workflow.abort_timers(),
            }
        } else {
            UndeployResponse {
                had_workflow: false,
                aborted_timer_count: 0,
            }
        }
    };

    let _ = app.emit("workflow://undeployed", response.clone());
    Ok(response)
}

#[tauri::command]
async fn list_connections(state: State<'_, DesktopState>) -> Result<Vec<ConnectionRecord>, String> {
    let connections = state.connection_manager.read().await.list();
    Ok(connections)
}

fn stringify_error(error: EngineError) -> String {
    error.to_string()
}

fn collect_timer_root_specs(graph: &WorkflowGraph) -> Result<Vec<TimerRootSpec>, EngineError> {
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

fn spawn_timer_root_tasks(
    ingress: WorkflowIngress,
    timer_roots: Vec<TimerRootSpec>,
) -> Vec<tauri::async_runtime::JoinHandle<()>> {
    timer_roots
        .into_iter()
        .map(|timer_root| {
            let ingress = ingress.clone();
            tauri::async_runtime::spawn(async move {
                if timer_root.immediate {
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
                    let _ = ingress
                        .submit_to(
                            &timer_root.node_id,
                            WorkflowContext::new(Value::Object(Default::default())),
                        )
                        .await;
                }
            })
        })
        .collect()
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
            list_connections
        ]);

    if let Err(error) = builder.run(tauri::generate_context!()) {
        eprintln!("failed to run Nazh desktop shell: {error}");
    }
}
