use std::{collections::HashMap, sync::Arc};

#[path = "workflow_deploy_helpers.rs"]
mod workflow_deploy_helpers;
use workflow_deploy_helpers::{derive_workflow_id, normalize_sql_writer_paths};

use nazh_engine::{
    ConnectionDefinition, RuntimeResources, WorkflowGraph, WorkflowId,
    deploy_workflow_and_restore_variables as deploy_workflow_graph,
};
use serde_json::{Value, json};
use store::DeploymentAuditRecord;
use tauri::{AppHandle, Emitter, State};
use tauri_bindings::{
    DeployResponse, ObservabilityContextInput, WorkflowRuntimePolicy, WorkflowRuntimePolicyInput,
};

use crate::{
    observability::ObservabilityStore,
    registry::shared_node_registry,
    runtime::{DeadLetterSink, DesktopWorkflow, RuntimeWorkflowMetadata, create_dispatch_router},
    state::DesktopState,
    util::stringify_error,
    workspace::resolve_project_workspace_dir,
};

const MAX_IPC_INPUT_BYTES: usize = 10 * 1024 * 1024;

/// 计算 AST 字符串的 SHA-256 哈希，用于部署版本变更检测（RFC-0003 Phase 3）。
fn compute_ast_hash(ast: &str) -> String {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(ast.as_bytes());
    format!("{:x}", hasher.finalize())
}

async fn record_deployment_audit(
    state: &DesktopState,
    metadata: &RuntimeWorkflowMetadata,
    action: &str,
    level: &str,
    message: &str,
    detail: Option<String>,
    data: Option<Value>,
) {
    let Ok(store) = state.store_handle() else {
        return;
    };
    let now = chrono::Utc::now();
    let record = DeploymentAuditRecord {
        id: format!(
            "deployment-audit-{}-{}",
            now.timestamp_millis(),
            now.timestamp_subsec_nanos()
        ),
        workflow_id: metadata.workflow_id.clone(),
        action: action.to_owned(),
        level: level.to_owned(),
        timestamp: now.to_rfc3339(),
        project_id: metadata.project_id.clone(),
        project_name: metadata.project_name.clone(),
        environment_id: metadata.environment_id.clone(),
        environment_name: metadata.environment_name.clone(),
        message: message.to_owned(),
        detail,
        data,
    };
    if let Err(error) = store.insert_deployment_audit(record).await {
        tracing::warn!(?error, workflow_id = %metadata.workflow_id, action, "部署审计写入失败");
    }
}

async fn load_persisted_variable_overrides(
    state: &DesktopState,
    workflow_id: &str,
) -> HashMap<String, Value> {
    let store = match state.store_handle() {
        Ok(store) => store,
        Err(error) => {
            tracing::warn!(?error, "Store 未就绪，跳过变量恢复");
            return HashMap::new();
        }
    };

    match store.load_variables(workflow_id).await {
        Ok(persisted) => {
            if !persisted.is_empty() {
                tracing::info!(
                    workflow_id = %workflow_id,
                    count = persisted.len(),
                    "已读取持久化变量覆盖值，准备在 on_deploy 前恢复"
                );
            }
            persisted
                .into_iter()
                .map(|var| (var.key, var.value))
                .collect()
        }
        Err(error) => {
            tracing::debug!(?error, "从 Store 加载变量失败，跳过恢复");
            HashMap::new()
        }
    }
}

/// 从 JSON 字符串部署工作流（供 Tauri IPC 和 copilot 工具共用）。
#[allow(dead_code, clippy::too_many_lines)]
pub(crate) async fn deploy_workflow_from_json(
    app: &AppHandle,
    state: &DesktopState,
    ast: &str,
    workflow_id: Option<String>,
) -> Result<DeployResponse, String> {
    if ast.len() > MAX_IPC_INPUT_BYTES {
        return Err("AST 超过最大允许大小（10 MB）".to_owned());
    }
    let mut graph = WorkflowGraph::from_json(ast).map_err(|e| stringify_error(&e))?;
    let (workspace_dir, _) = resolve_project_workspace_dir(app, None)?;
    normalize_sql_writer_paths(&mut graph, &workspace_dir).map_err(|e| stringify_error(&e))?;
    let workflow_id = derive_workflow_id(workflow_id.as_deref(), graph.name.as_deref(), None);
    let policy = WorkflowRuntimePolicy::default();
    let deployed_at = chrono::Utc::now().to_rfc3339();
    let ast_hash = compute_ast_hash(ast);
    let metadata = RuntimeWorkflowMetadata {
        workflow_id: workflow_id.clone(),
        project_id: None,
        project_name: None,
        environment_id: None,
        environment_name: None,
        deployed_at,
    };
    record_deployment_audit(
        state,
        &metadata,
        "deploy_requested",
        "info",
        "收到部署请求",
        None,
        Some(json!({
            "workflow_id": workflow_id.clone(),
            "source": "json",
            "ast_hash": ast_hash,
        })),
    )
    .await;

    let node_count = graph.nodes.len();
    let edge_count = graph.edges.len();
    let registry = shared_node_registry();
    let extra_resources = RuntimeResources::new()
        .with_resource(Arc::clone(&state.approval_registry))
        .with_resource(WorkflowId(Arc::new(workflow_id.clone())));
    let variable_overrides = load_persisted_variable_overrides(state, &workflow_id).await;

    let deployment = match deploy_workflow_graph(
        graph,
        state.connection_manager.clone(),
        registry,
        Some(workflow_id.clone()),
        extra_resources,
        variable_overrides,
    )
    .await
    {
        Ok(deployment) => deployment,
        Err(error) => {
            record_deployment_audit(
                state,
                &metadata,
                "deploy_failed",
                "error",
                "部署失败",
                Some(error.to_string()),
                None,
            )
            .await;
            return Err(stringify_error(&error));
        }
    };

    let nazh_engine::WorkflowDeploymentParts {
        ingress,
        streams,
        lifecycle_guards,
        shutdown_token,
        shared_resources,
        output_caches,
    } = deployment.into_parts();
    let root_nodes = ingress.root_nodes().to_vec();
    let (event_rx, var_event_rx, result_rx, result_store_ref) = streams.into_receivers();
    let dead_letters = DeadLetterSink::new(workspace_dir, metadata.clone()).await?;
    let (dispatch_router, mut runtime_tasks) = create_dispatch_router(
        ingress,
        workflow_id.clone(),
        policy.clone(),
        None,
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
        None,
    ));

    runtime_tasks.push(crate::events::spawn_variable_event_forwarder(
        app.clone(),
        workflow_id.clone(),
        var_event_rx,
        state.store_handle()?,
    ));

    runtime_tasks.push(crate::events::spawn_result_forwarder(
        app.clone(),
        workflow_id.clone(),
        result_rx,
        result_store_ref,
        None,
    ));

    {
        let mut workflows = state.workflows.lock().await;
        workflows.insert(
            workflow_id.clone(),
            DesktopWorkflow {
                workflow_id: workflow_id.clone(),
                metadata: metadata.clone(),
                policy,
                dispatch_router: dispatch_router.clone(),
                observability: None,
                node_count,
                edge_count,
                root_nodes: root_nodes.clone(),
                lifecycle_guards,
                shutdown_token,
                shared_resources: shared_resources.clone(),
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
        project_id: None,
        workflow_id: Some(workflow_id.clone()),
        replaced_existing: Some(replaced_existing),
    };

    let _ = app.emit("workflow://deployed", deploy_payload.clone());
    record_deployment_audit(
        state,
        &metadata,
        "deploy_success",
        "success",
        "部署完成",
        Some(format!("节点 {node_count} / 连线 {edge_count}")),
        Some(json!({
            "node_count": node_count,
            "edge_count": edge_count,
            "root_nodes": deploy_payload.root_nodes.clone(),
            "replaced_existing": replaced_existing,
            "ast_hash": ast_hash,
        })),
    )
    .await;
    Ok(deploy_payload)
}

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
    let connection_definitions_supplied = connection_definitions.is_some();
    if let Some(definitions) = connection_definitions {
        let connection_result = if state.workflows.lock().await.is_empty() {
            state
                .connection_manager
                .replace_connections(definitions)
                .await
        } else {
            state
                .connection_manager
                .upsert_connections(definitions)
                .await
        };
        if let Err(error) = connection_result {
            let message = format!("连接定义校验失败: {error}");
            record_deployment_audit(
                &state,
                &metadata,
                "deploy_rejected",
                "error",
                "部署前连接定义校验失败",
                Some(message.clone()),
                None,
            )
            .await;
            return Err(message);
        }
    }
    let store_handle = state.store_handle().ok();
    let ast_hash = compute_ast_hash(&ast);
    record_deployment_audit(
        &state,
        &metadata,
        "deploy_requested",
        "info",
        "收到部署请求",
        None,
        Some(json!({
            "workflow_id": workflow_id.clone(),
            "project_name": metadata.project_name.clone(),
            "environment_name": metadata.environment_name.clone(),
            "connection_definitions_supplied": connection_definitions_supplied,
            "ast_hash": ast_hash,
        })),
    )
    .await;
    let observability_store = if let Some(context) = observability_context.clone() {
        let store =
            ObservabilityStore::new(workspace_dir.clone(), context, store_handle.clone()).await?;
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
    let extra_resources = RuntimeResources::new()
        .with_resource(Arc::clone(&state.approval_registry))
        .with_resource(WorkflowId(Arc::new(workflow_id.clone())));
    let variable_overrides = load_persisted_variable_overrides(&state, &workflow_id).await;
    let deployment = match deploy_workflow_graph(
        graph,
        state.connection_manager.clone(),
        registry,
        Some(workflow_id.clone()),
        extra_resources,
        variable_overrides,
    )
    .await
    {
        Ok(deployment) => deployment,
        Err(error) => {
            record_deployment_audit(
                &state,
                &metadata,
                "deploy_failed",
                "error",
                "部署失败",
                Some(error.to_string()),
                None,
            )
            .await;
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
    let (event_rx, var_event_rx, result_rx, result_store_ref) = streams.into_receivers();
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

    runtime_tasks.push(crate::events::spawn_variable_event_forwarder(
        app.clone(),
        workflow_id.clone(),
        var_event_rx,
        state.store_handle()?,
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
                shared_resources: shared_resources.clone(),
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
    record_deployment_audit(
        &state,
        &metadata,
        "deploy_success",
        "success",
        "部署完成",
        Some(format!(
            "workflow_id={workflow_id} · 节点 {node_count} / 连线 {edge_count}"
        )),
        Some(json!({
            "workflow_id": workflow_id.clone(),
            "node_count": node_count,
            "edge_count": edge_count,
            "root_nodes": deploy_payload.root_nodes.clone(),
            "replaced_existing": replaced_existing,
            "ast_hash": ast_hash,
        })),
    )
    .await;
    let _ = app.emit("workflow://deployed", deploy_payload.clone());
    Ok(deploy_payload)
}
