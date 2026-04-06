use nazh_engine::{
    deploy_workflow as deploy_workflow_graph, shared_connection_manager, ConnectionRecord,
    EngineError, WorkflowContext, WorkflowGraph, WorkflowIngress,
};
use serde::Serialize;
use serde_json::Value;
use tauri::{AppHandle, Emitter, State};
use tokio::sync::Mutex;

struct DesktopState {
    connection_manager: nazh_engine::SharedConnectionManager,
    workflow: Mutex<Option<WorkflowIngress>>,
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

#[tauri::command]
async fn deploy_workflow(
    app: AppHandle,
    state: State<'_, DesktopState>,
    ast: String,
) -> Result<DeployResponse, String> {
    let graph = WorkflowGraph::from_json(&ast).map_err(stringify_error)?;
    let node_count = graph.nodes.len();
    let edge_count = graph.edges.len();
    let deployment = deploy_workflow_graph(graph, state.connection_manager.clone())
        .await
        .map_err(stringify_error)?;
    let (ingress, streams) = deployment.into_parts();
    let root_nodes = ingress.root_nodes().to_vec();
    let (mut event_rx, mut result_rx) = streams.into_receivers();

    {
        let mut workflow_guard = state.workflow.lock().await;
        *workflow_guard = Some(ingress);
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
            .clone()
            .ok_or_else(|| stringify_error(EngineError::WorkflowUnavailable))?
    };

    let ctx = WorkflowContext::new(payload);
    let trace_id = ctx.trace_id.to_string();
    ingress.submit(ctx).await.map_err(stringify_error)?;
    Ok(DispatchResponse { trace_id })
}

#[tauri::command]
async fn list_connections(
    state: State<'_, DesktopState>,
) -> Result<Vec<ConnectionRecord>, String> {
    let connections = state.connection_manager.read().await.list();
    Ok(connections)
}

fn stringify_error(error: EngineError) -> String {
    error.to_string()
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let builder = tauri::Builder::default()
        .manage(DesktopState::default())
        .invoke_handler(tauri::generate_handler![
            deploy_workflow,
            dispatch_payload,
            list_connections
        ]);

    if let Err(error) = builder.run(tauri::generate_context!()) {
        eprintln!("failed to run Nazh desktop shell: {error}");
    }
}
