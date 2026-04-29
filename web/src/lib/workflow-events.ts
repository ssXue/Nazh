//! 工作流运行时事件解析与状态机。
//!
//! 从 Tauri 事件 payload 解析 ExecutionEvent 联合类型，
//! 通过 reducer 模式维护工作流运行时状态。

import type {
  AppErrorRecord,
  EdgeTransmitSummary,
  BackpressureDetected,
  ExecutionEvent,
  RuntimeLogEntry,
  WorkflowRuntimeState,
} from '../types';

export interface ParsedWorkflowEvent {
  kind: 'started' | 'completed' | 'failed' | 'output' | 'finished'
    | 'edge-transmit-summary' | 'backpressure-detected';
  nodeId: string;
  traceId: string;
  error?: string;
  /** ADR-0016：边传输汇总载荷（kind = 'edge-transmit-summary'）。 */
  edgeTransmitSummary?: EdgeTransmitSummary;
  /** ADR-0016：背压告警载荷（kind = 'backpressure-detected'）。 */
  backpressureDetected?: BackpressureDetected;
}

export const EMPTY_RUNTIME_STATE: WorkflowRuntimeState = {
  traceId: null,
  lastEventType: null,
  lastNodeId: null,
  lastError: null,
  lastUpdatedAt: null,
  activeNodeIds: [],
  completedNodeIds: [],
  failedNodeIds: [],
  outputNodeIds: [],
};

export function pushUnique(items: string[], item: string): string[] {
  return items.includes(item) ? items : [...items, item];
}

export function removeItem(items: string[], item: string): string[] {
  return items.filter((current) => current !== item);
}

export function createClientEntryId(prefix: string): string {
  return `${prefix}-${Date.now()}-${Math.random().toString(16).slice(2, 8)}`;
}

export function describeUnknownError(error: unknown): { message: string; detail?: string | null } {
  if (error instanceof Error) {
    return {
      message: error.message || '未知错误',
      detail: error.stack ?? null,
    };
  }

  if (typeof error === 'string') {
    return { message: error };
  }

  if (error && typeof error === 'object') {
    try {
      return {
        message: JSON.stringify(error),
      };
    } catch {
      return {
        message: '发生了无法序列化的异常对象',
      };
    }
  }

  return { message: '未知错误' };
}

export function buildRuntimeLogEntry(
  source: string,
  level: RuntimeLogEntry['level'],
  message: string,
  detail?: string | null,
): RuntimeLogEntry {
  return {
    id: createClientEntryId('log'),
    timestamp: Date.now(),
    level,
    source,
    message,
    detail: detail ?? null,
  };
}

export function buildAppErrorRecord(
  scope: AppErrorRecord['scope'],
  title: string,
  detail?: string | null,
): AppErrorRecord {
  return {
    id: createClientEntryId('error'),
    timestamp: Date.now(),
    scope,
    title,
    detail: detail ?? null,
  };
}

export function parseWorkflowEventPayload(payload: unknown): ParsedWorkflowEvent | null {
  if (!payload || typeof payload !== 'object') {
    return null;
  }

  const event = payload as ExecutionEvent;

  if ('Started' in event) {
    return {
      kind: 'started',
      nodeId: event.Started.stage,
      traceId: event.Started.trace_id,
    };
  }

  if ('Completed' in event) {
    return {
      kind: 'completed',
      nodeId: event.Completed.stage,
      traceId: event.Completed.trace_id,
    };
  }

  if ('Failed' in event) {
    return {
      kind: 'failed',
      nodeId: event.Failed.stage,
      traceId: event.Failed.trace_id,
      error: event.Failed.error,
    };
  }

  if ('Output' in event) {
    return {
      kind: 'output',
      nodeId: event.Output.stage,
      traceId: event.Output.trace_id,
    };
  }

  if ('Finished' in event) {
    return {
      kind: 'finished',
      nodeId: '',
      traceId: event.Finished.trace_id,
    };
  }

  // ADR-0016：边级观测事件——纯可观测数据，不影响运行时状态机。
  if ('EdgeTransmitSummary' in event) {
    return {
      kind: 'edge-transmit-summary',
      nodeId: event.EdgeTransmitSummary.from_node,
      traceId: '',
      edgeTransmitSummary: event.EdgeTransmitSummary,
    };
  }

  if ('BackpressureDetected' in event) {
    return {
      kind: 'backpressure-detected',
      nodeId: event.BackpressureDetected.at_node,
      traceId: '',
      backpressureDetected: event.BackpressureDetected,
    };
  }

  return null;
}

export function reduceRuntimeState(
  current: WorkflowRuntimeState,
  event: ParsedWorkflowEvent,
): WorkflowRuntimeState {
  const baseState =
    current.traceId === event.traceId
      ? current
      : {
          ...EMPTY_RUNTIME_STATE,
          traceId: event.traceId,
        };

  const nextState: WorkflowRuntimeState = {
    ...baseState,
    traceId: event.traceId,
    lastEventType: event.kind,
    lastNodeId: event.nodeId,
    lastError: event.kind === 'failed' ? event.error ?? null : null,
    lastUpdatedAt: Date.now(),
  };

  switch (event.kind) {
    case 'started':
      nextState.activeNodeIds = pushUnique(baseState.activeNodeIds, event.nodeId);
      nextState.completedNodeIds = removeItem(baseState.completedNodeIds, event.nodeId);
      nextState.failedNodeIds = removeItem(baseState.failedNodeIds, event.nodeId);
      nextState.outputNodeIds = removeItem(baseState.outputNodeIds, event.nodeId);
      return nextState;
    case 'completed':
      nextState.activeNodeIds = removeItem(baseState.activeNodeIds, event.nodeId);
      nextState.completedNodeIds = pushUnique(baseState.completedNodeIds, event.nodeId);
      nextState.failedNodeIds = removeItem(baseState.failedNodeIds, event.nodeId);
      nextState.outputNodeIds = baseState.outputNodeIds;
      return nextState;
    case 'failed':
      nextState.activeNodeIds = removeItem(baseState.activeNodeIds, event.nodeId);
      nextState.completedNodeIds = removeItem(baseState.completedNodeIds, event.nodeId);
      nextState.failedNodeIds = pushUnique(baseState.failedNodeIds, event.nodeId);
      nextState.outputNodeIds = removeItem(baseState.outputNodeIds, event.nodeId);
      return nextState;
    case 'output':
      nextState.activeNodeIds = removeItem(baseState.activeNodeIds, event.nodeId);
      nextState.completedNodeIds = pushUnique(baseState.completedNodeIds, event.nodeId);
      nextState.failedNodeIds = removeItem(baseState.failedNodeIds, event.nodeId);
      nextState.outputNodeIds = pushUnique(baseState.outputNodeIds, event.nodeId);
      return nextState;
    case 'finished':
      nextState.activeNodeIds = [];
      nextState.completedNodeIds = baseState.completedNodeIds;
      nextState.failedNodeIds = baseState.failedNodeIds;
      nextState.outputNodeIds = baseState.outputNodeIds;
      return nextState;
    // ADR-0016：边级观测事件不影响运行时状态机——透传。
    case 'edge-transmit-summary':
    case 'backpressure-detected':
      return baseState;
  }
}

/** Reactive 引脚值变更事件（ADR-0015 Phase 2，独立事件 channel workflow://reactive-update/*）。 */
export interface ReactiveUpdateEvent {
  workflowId: string;
  nodeId: string;
  pinId: string;
  value: unknown;
  updatedAt: string;
}

/** 从 Tauri 事件 payload 解析 ReactiveUpdate。 */
export function parseReactiveUpdate(payload: unknown): ReactiveUpdateEvent | null {
  if (!payload || typeof payload !== 'object') return null;
  const p = payload as Record<string, unknown>;
  if (
    typeof p.workflowId === 'string' &&
    typeof p.nodeId === 'string' &&
    typeof p.pinId === 'string' &&
    'value' in p &&
    typeof p.updatedAt === 'string'
  ) {
    return {
      workflowId: p.workflowId,
      nodeId: p.nodeId,
      pinId: p.pinId,
      value: p.value,
      updatedAt: p.updatedAt,
    };
  }
  return null;
}
