//! 工作流状态派生与展示。

import type { DeployResponse, WorkflowRuntimeState, WorkflowWindowStatus } from '../types';

/** 根据运行时信息推导当前工作流窗口状态。 */
export function deriveWorkflowStatus(
  tauriRuntime: boolean,
  hasActiveBoard: boolean,
  deployInfo: DeployResponse | null,
  runtimeState: WorkflowRuntimeState,
): WorkflowWindowStatus {
  if (!tauriRuntime) {
    return 'preview';
  }

  if (!hasActiveBoard || !deployInfo) {
    return 'idle';
  }

  if (runtimeState.lastEventType === 'failed' || runtimeState.failedNodeIds.length > 0) {
    return 'failed';
  }

  if (runtimeState.lastEventType === 'started' || runtimeState.activeNodeIds.length > 0) {
    return 'running';
  }

  if (
    runtimeState.traceId &&
    (runtimeState.lastEventType === 'output' ||
      runtimeState.outputNodeIds.length > 0 ||
      (runtimeState.lastEventType === 'completed' &&
        runtimeState.completedNodeIds.length > 0 &&
        runtimeState.activeNodeIds.length === 0))
  ) {
    return 'completed';
  }

  return 'deployed';
}

/** 返回工作流状态对应的中文标签。 */
export function getWorkflowStatusLabel(status: WorkflowWindowStatus): string {
  switch (status) {
    case 'preview':
      return '浏览器预览';
    case 'idle':
      return '未部署';
    case 'deployed':
      return '已部署待运行';
    case 'running':
      return '运行中';
    case 'completed':
      return '执行完成';
    case 'failed':
      return '执行失败';
  }
}

/** 返回工作流状态对应的状态胶囊 CSS class。 */
export function getWorkflowStatusPillClass(status: WorkflowWindowStatus): string {
  switch (status) {
    case 'running':
      return 'runtime-pill--running';
    case 'failed':
      return 'runtime-pill--failed';
    case 'completed':
    case 'deployed':
      return 'runtime-pill--ready';
    case 'idle':
    case 'preview':
      return 'runtime-pill--idle';
  }
}
