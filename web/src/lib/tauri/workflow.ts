import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';

import type {
  DeadLetterRecord,
  DeployResponse,
  DescribeNodePinsRequest,
  DescribeNodePinsResponse,
  DispatchResponse,
  ListNodeTypesResponse,
  RuntimeWorkflowSummary,
  UndeployResponse,
  WorkflowRuntimePolicyInput,
  WorkflowResult,
} from '../../types';

export interface ScopedWorkflowEvent {
  workflowId: string;
  event: unknown;
}

export interface ScopedWorkflowResult {
  workflowId: string;
  result: WorkflowResult;
}

export async function deployWorkflow(
  ast: string,
  observabilityContext?: {
    workspacePath: string;
    projectId: string;
    projectName: string;
    environmentId: string;
    environmentName: string;
    deploymentSource?: string;
  },
  runtimeOptions?: {
    workflowId?: string;
    runtimePolicy?: WorkflowRuntimePolicyInput;
  },
): Promise<DeployResponse> {
  return invoke<DeployResponse>('deploy_workflow', {
    ast,
    observabilityContext: observabilityContext
      ? {
          workspacePath: observabilityContext.workspacePath.trim(),
          projectId: observabilityContext.projectId,
          projectName: observabilityContext.projectName,
          environmentId: observabilityContext.environmentId,
          environmentName: observabilityContext.environmentName,
          deploymentSource: observabilityContext.deploymentSource ?? 'manual',
        }
      : null,
    workflowId: runtimeOptions?.workflowId?.trim() ? runtimeOptions.workflowId.trim() : null,
    runtimePolicy: runtimeOptions?.runtimePolicy ?? null,
  });
}

export async function dispatchPayload(
  payload: unknown,
  workflowId?: string | null,
): Promise<DispatchResponse> {
  return invoke<DispatchResponse>('dispatch_payload', {
    payload,
    workflowId: workflowId?.trim() ? workflowId.trim() : null,
  });
}

export async function undeployWorkflow(workflowId?: string | null): Promise<UndeployResponse> {
  return invoke<UndeployResponse>('undeploy_workflow', {
    workflowId: workflowId?.trim() ? workflowId.trim() : null,
  });
}

export async function listNodeTypes(): Promise<ListNodeTypesResponse> {
  return invoke<ListNodeTypesResponse>('list_node_types');
}

/**
 * 给定节点类型 + config，返回该节点实例的 input/output pin schema。
 *
 * 用于前端连接期校验——FlowGram `canAddLine` 钩子通过缓存的 pin schema
 * 即时判断"上游产出 → 下游期望"是否兼容，错连立刻拒绝并给视觉反馈。
 *
 * 实例化无副作用（节点构造器只读 config + 资源句柄克隆）。失败时调用
 * 方应 fallback 到 `Any/Any`，部署期校验作为 backstop 兜底。
 */
export async function describeNodePins(
  nodeType: string,
  config: Record<string, unknown>,
): Promise<DescribeNodePinsResponse> {
  const request: DescribeNodePinsRequest = {
    nodeType,
    config: config as DescribeNodePinsRequest['config'],
  };
  return invoke<DescribeNodePinsResponse>('describe_node_pins', { request });
}

export async function listRuntimeWorkflows(): Promise<RuntimeWorkflowSummary[]> {
  return invoke<RuntimeWorkflowSummary[]>('list_runtime_workflows');
}

export async function setActiveRuntimeWorkflow(
  workflowId: string,
): Promise<RuntimeWorkflowSummary> {
  return invoke<RuntimeWorkflowSummary>('set_active_runtime_workflow', {
    workflowId: workflowId.trim(),
  });
}

export async function listDeadLetters(
  workspacePath: string,
  workflowId?: string | null,
  limit = 120,
): Promise<DeadLetterRecord[]> {
  return invoke<DeadLetterRecord[]>('list_dead_letters', {
    workspacePath: workspacePath.trim() || null,
    workflowId: workflowId?.trim() ? workflowId.trim() : null,
    limit,
  });
}

export async function onWorkflowEvent(
  handler: (payload: ScopedWorkflowEvent) => void,
): Promise<() => void> {
  const unlisten = await listen<ScopedWorkflowEvent>('workflow://node-status', (event) => {
    handler(event.payload);
  });

  return () => {
    unlisten();
  };
}

export async function onWorkflowResult(
  handler: (payload: ScopedWorkflowResult) => void,
): Promise<() => void> {
  const unlisten = await listen<ScopedWorkflowResult>('workflow://result', (event) => {
    handler(event.payload);
  });

  return () => {
    unlisten();
  };
}

export async function onWorkflowDeployed(
  handler: (payload: DeployResponse) => void,
): Promise<() => void> {
  const unlisten = await listen<DeployResponse>('workflow://deployed', (event) => {
    handler(event.payload);
  });

  return () => {
    unlisten();
  };
}

export async function onWorkflowUndeployed(
  handler: (payload: UndeployResponse) => void,
): Promise<() => void> {
  const unlisten = await listen<UndeployResponse>('workflow://undeployed', (event) => {
    handler(event.payload);
  });

  return () => {
    unlisten();
  };
}

export async function onRuntimeWorkflowFocus(
  handler: (payload: RuntimeWorkflowSummary) => void,
): Promise<() => void> {
  const unlisten = await listen<RuntimeWorkflowSummary>('workflow://runtime-focus', (event) => {
    handler(event.payload);
  });

  return () => {
    unlisten();
  };
}

export async function respondHumanLoop(params: {
  approvalId: string;
  action: 'approved' | 'rejected';
  formData: Record<string, unknown>;
  comment?: string | null;
  respondedBy?: string | null;
}): Promise<void> {
  return invoke<void>('respond_human_loop', {
    approvalId: params.approvalId,
    action: params.action,
    formData: params.formData,
    comment: params.comment ?? null,
    respondedBy: params.respondedBy ?? null,
  });
}

export async function listPendingApprovals(
  workflowId?: string | null,
): Promise<unknown[]> {
  return invoke<unknown[]>('list_pending_approvals', {
    workflowId: workflowId?.trim() ? workflowId.trim() : null,
  });
}
