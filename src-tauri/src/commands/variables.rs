use std::sync::Arc;

use tauri::State;
use tauri_bindings::{
    DeleteWorkflowVariableRequest, DeleteWorkflowVariableResponse, SetWorkflowVariableRequest,
    SetWorkflowVariableResponse, SnapshotWorkflowVariablesRequest,
    SnapshotWorkflowVariablesResponse,
};

use crate::state::DesktopState;

/// 从已部署工作流中取出 `Arc<WorkflowVariables>` 并释放 `workflows` Mutex。
///
/// 四个 IPC 命令共享同一套
/// "取 Arc → 块作用域 drop `MutexGuard`" 模式；提取为 helper 消除重复。
async fn resolve_workflow_variables(
    state: &DesktopState,
    workflow_id: &str,
) -> Result<Arc<nazh_engine::WorkflowVariables>, String> {
    let workflows = state.workflows.lock().await;
    let workflow = workflows
        .get(workflow_id)
        .ok_or_else(|| format!("工作流 `{workflow_id}` 未部署或已撤销"))?;
    workflow
        .shared_resources
        .get::<Arc<nazh_engine::WorkflowVariables>>()
        .ok_or_else(|| {
            // 走到这里说明 deploy_workflow_with_ai 漏注入了 WorkflowVariables——引擎层 bug
            tracing::error!(
                workflow_id = %workflow_id,
                "WorkflowVariables 缺失：deploy_workflow_with_ai 应无条件注入"
            );
            format!("内部错误：工作流 `{workflow_id}` 无 WorkflowVariables 资源")
        })
    // workflows MutexGuard 在此 drop，Arc<WorkflowVariables> 为 owned clone（refcount bump）
}

/// 返回指定已部署工作流的变量快照。
///
/// 若工作流不存在或部署中未注入 [`WorkflowVariables`]，返回错误。
/// 调用方（前端）应以此作为轻量级运行时状态探针——变量值在节点执行中动态更新，
/// 快照为调用瞬间的一致性读（`DashMap` 逐桶读，非全局锁）。
#[tauri::command]
pub(crate) async fn snapshot_workflow_variables(
    state: State<'_, DesktopState>,
    request: SnapshotWorkflowVariablesRequest,
) -> Result<SnapshotWorkflowVariablesResponse, String> {
    let vars = resolve_workflow_variables(&state, &request.workflow_id).await?;

    let variables = vars
        .snapshot()
        .into_iter()
        .map(|(k, v)| (k, v.into()))
        .collect();

    Ok(SnapshotWorkflowVariablesResponse { variables })
}

/// IPC 写命令：前端或外部工具直接覆写单个工作流变量。
///
/// 取 [`WorkflowVariables`] Arc 后释放 `workflows` Mutex，避免在 `DashMap` 写操作期间
/// 持有全局锁。`updated_by = "ipc"` 哨兵用于区分节点写路径（`node_id`）。
/// 写入后立刻读回快照返回，让前端无需额外 `snapshot_workflow_variables` 调用即可看到
/// 新的 `updated_at`。
#[tauri::command]
pub(crate) async fn set_workflow_variable(
    state: State<'_, DesktopState>,
    request: SetWorkflowVariableRequest,
) -> Result<SetWorkflowVariableResponse, String> {
    // 取 Arc<WorkflowVariables>，MutexGuard 在 resolve_workflow_variables 内 drop
    let vars = resolve_workflow_variables(&state, &request.workflow_id).await?;

    // 写入：updated_by = "ipc" 哨兵，与 node_id 路径区分
    vars.set(&request.name, request.value, Some("ipc"))
        .map_err(|err| err.to_string())?;

    // 写入后读回快照返回（让前端立即看到新 updated_at / updated_by）
    // 类型由 SetWorkflowVariableResponse::snapshot 字段推断（TypedVariableSnapshot from nazh_core）
    let snapshot = vars
        .get(&request.name)
        // 理论上不可达：写入成功意味着变量存在。但并发 delete_workflow_variable 可能在
        // set 与 get 之间移除该变量——保留作为安全网。
        .ok_or_else(|| format!("变量 `{}` 写入后未能读回", request.name))?
        .into();

    Ok(SetWorkflowVariableResponse { snapshot })
}

/// IPC 删除命令：移除指定工作流变量（ADR-0012 Phase 3）。
///
/// 变量不存在时为幂等操作（返回 `removed_snapshot: None`）。
/// 成功移除时引擎侧发 `VariableDeleted` 事件，前端通过
/// `workflow://variable-deleted` 通道接收。
#[tauri::command]
pub(crate) async fn delete_workflow_variable(
    state: State<'_, DesktopState>,
    request: DeleteWorkflowVariableRequest,
) -> Result<DeleteWorkflowVariableResponse, String> {
    let vars = resolve_workflow_variables(&state, &request.workflow_id).await?;
    let removed = vars.remove(&request.name);
    let removed_snapshot = removed.map(Into::into);
    Ok(DeleteWorkflowVariableResponse { removed_snapshot })
}
