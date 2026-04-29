use std::{
    path::{Component, Path, PathBuf},
    sync::Arc,
};

use nazh_engine::{
    AiService, ConnectionDefinition, EngineError, WorkflowContext, WorkflowGraph,
    deploy_workflow_with_ai as deploy_workflow_graph,
};
use serde_json::{Value, json};
use tauri::{AppHandle, Emitter, State};
use tauri_bindings::{DeployResponse, DispatchResponse, UndeployResponse};

use crate::{
    observability::{ObservabilityContextInput, ObservabilityStore},
    registry::shared_node_registry,
    runtime::{
        DeadLetterSink, DesktopWorkflow, RuntimeWorkflowMetadata, WorkflowRuntimePolicy,
        WorkflowRuntimePolicyInput, create_dispatch_router,
    },
    state::DesktopState,
    util::stringify_error,
    workspace::resolve_project_workspace_dir,
};

const MAX_IPC_INPUT_BYTES: usize = 10 * 1024 * 1024;
const SQL_WRITER_DEFAULT_DATABASE_PATH: &str = "./nazh-local.sqlite3";

#[tauri::command]
#[allow(clippy::too_many_lines)]
pub(crate) async fn deploy_workflow(
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
    let registry = shared_node_registry();
    let ai_service: Arc<dyn AiService> = state.ai_service.clone();
    let deployment = match deploy_workflow_graph(
        graph,
        state.connection_manager.clone(),
        Some(ai_service),
        registry,
        Some(workflow_id.clone()),
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
        shared_resources,
        // ADR-0014 引脚求值语义二分：按节点 id 索引的 OutputCache 句柄。
        output_caches,
    } = deployment.into_parts();
    let root_nodes = ingress.root_nodes().to_vec();
    let (event_rx, result_rx, result_store_ref) = streams.into_receivers();
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

    runtime_tasks.push(crate::events::spawn_execution_event_forwarder(
        app.clone(),
        workflow_id.clone(),
        event_rx,
        observability_store.clone(),
    ));

    runtime_tasks.push(crate::events::spawn_result_forwarder(
        app.clone(),
        workflow_id.clone(),
        result_rx,
        result_store_ref,
        observability_store.clone(),
    ));

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
                shared_resources,
                output_caches,
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
pub(crate) async fn dispatch_payload(
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
pub(crate) async fn undeploy_workflow(
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
