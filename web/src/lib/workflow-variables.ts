import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import type {
  DeleteGlobalVariableRequest,
  DeleteWorkflowVariableRequest,
  DeleteWorkflowVariableResponse,
  GetGlobalVariableRequest,
  GetGlobalVariableResponse,
  ListGlobalVariablesRequest,
  ListGlobalVariablesResponse,
  QueryVariableHistoryRequest,
  QueryVariableHistoryResponse,
  ResetWorkflowVariableRequest,
  ResetWorkflowVariableResponse,
  SetGlobalVariableRequest,
  SetGlobalVariableResponse,
  SetWorkflowVariableRequest,
  SetWorkflowVariableResponse,
  SnapshotWorkflowVariablesResponse,
  VariableChangedPayload,
  VariableDeletedPayload,
} from '../generated';

/**
 * 写入工作流变量（ADR-0012 Phase 2）。
 *
 * 类型不匹配 / 变量未声明 / 工作流未部署等错误以 Promise reject 抛出。
 * 写入成功后服务端立刻读回新快照并返回，调用方无需再调 `snapshotWorkflowVariables`。
 */
export async function setWorkflowVariable(
  request: SetWorkflowVariableRequest,
): Promise<SetWorkflowVariableResponse> {
  return invoke<SetWorkflowVariableResponse>('set_workflow_variable', {
    request,
  });
}

/**
 * 删除工作流变量（ADR-0012 Phase 3）。
 *
 * 变量不存在时为幂等操作（`removedSnapshot` 为 `undefined`）。
 */
export async function deleteWorkflowVariable(
  request: DeleteWorkflowVariableRequest,
): Promise<DeleteWorkflowVariableResponse> {
  return invoke<DeleteWorkflowVariableResponse>('delete_workflow_variable', {
    request,
  });
}

/**
 * 将变量重置为声明初值（ADR-0012 Phase 3）。
 *
 * 后端 `TypedVariable.initial` 保存了部署时的声明初值，调用方无需传入。
 */
export async function resetWorkflowVariable(
  request: ResetWorkflowVariableRequest,
): Promise<ResetWorkflowVariableResponse> {
  return invoke<ResetWorkflowVariableResponse>('reset_workflow_variable', {
    request,
  });
}

/**
 * 读取工作流变量当前快照（ADR-0012 Phase 1 命令）。
 *
 * 返回 `{ variables: Record<string, TypedVariableSnapshot> }`。
 * 工作流未部署时 Promise reject，调用方需处理异常。
 */
export async function snapshotWorkflowVariables(
  workflowId: string,
): Promise<SnapshotWorkflowVariablesResponse> {
  return invoke<SnapshotWorkflowVariablesResponse>(
    'snapshot_workflow_variables',
    { request: { workflowId } },
  );
}

/**
 * 订阅 `workflow://variable-changed` 事件（ADR-0012 Phase 2）。
 *
 * 每次工作流变量被节点脚本或 IPC 写入时触发，payload 携带最新值与来源。
 *
 * 返回 unlisten 函数；调用方负责在组件卸载 / hook cleanup 时调用以释放监听器。
 */
export async function onWorkflowVariableChanged(
  handler: (payload: VariableChangedPayload) => void,
): Promise<() => void> {
  return listen<VariableChangedPayload>(
    'workflow://variable-changed',
    (event) => handler(event.payload),
  );
}

/**
 * 订阅 `workflow://variable-deleted` 事件（ADR-0012 Phase 3）。
 */
export async function onWorkflowVariableDeleted(
  handler: (payload: VariableDeletedPayload) => void,
): Promise<() => void> {
  return listen<VariableDeletedPayload>(
    'workflow://variable-deleted',
    (event) => handler(event.payload),
  );
}

/**
 * 查询变量变更历史（ADR-0012 Phase 3）。
 *
 * 返回最近 N 条历史记录（默认 100 条），按时间倒序。
 */
export async function queryVariableHistory(
  request: QueryVariableHistoryRequest,
): Promise<QueryVariableHistoryResponse> {
  return invoke<QueryVariableHistoryResponse>('query_variable_history', {
    request,
  });
}

/**
 * 设置全局变量（ADR-0012 Phase 3）。
 *
 * 全局变量不属于任何工作流，按 namespace + key 唯一标识。
 * `varType` 缺省为 `"Any"`。
 */
export async function setGlobalVariable(
  request: SetGlobalVariableRequest,
): Promise<SetGlobalVariableResponse> {
  return invoke<SetGlobalVariableResponse>('set_global_variable', {
    request,
  });
}

/**
 * 获取单个全局变量（ADR-0012 Phase 3）。
 *
 * 变量不存在时 `snapshot` 为 `undefined`。
 */
export async function getGlobalVariable(
  request: GetGlobalVariableRequest,
): Promise<GetGlobalVariableResponse> {
  return invoke<GetGlobalVariableResponse>('get_global_variable', {
    request,
  });
}

/**
 * 列出全局变量（ADR-0012 Phase 3）。
 *
 * 可选按 namespace 过滤；缺省返回全部。
 */
export async function listGlobalVariables(
  request: ListGlobalVariablesRequest,
): Promise<ListGlobalVariablesResponse> {
  return invoke<ListGlobalVariablesResponse>('list_global_variables', {
    request,
  });
}

/**
 * 删除全局变量（ADR-0012 Phase 3）。
 *
 * 变量不存在时为幂等操作。
 */
export async function deleteGlobalVariable(
  request: DeleteGlobalVariableRequest,
): Promise<void> {
  return invoke<void>('delete_global_variable', { request });
}
