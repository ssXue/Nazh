import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import type {
  DeleteWorkflowVariableRequest,
  DeleteWorkflowVariableResponse,
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
