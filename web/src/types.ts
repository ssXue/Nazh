import type { WorkflowJSON as FlowgramWorkflowJSON } from '@flowgram.ai/free-layout-editor';

// ── 从 Rust 引擎自动生成的 IPC 契约类型（ts-rs） ────────────

export type {
  AiAgentSettings,
  AiCompletionRequest,
  AiCompletionResponse,
  AiConfigUpdate,
  AiConfigView,
  AiGenerationParams,
  AiMessage,
  AiMessageRole,
  AiProviderDraft,
  AiProviderUpsert,
  AiProviderView,
  AiSecretInput,
  AiTestResult,
  AiTokenUsage,
  ConnectionDefinition,
  DeployResponse,
  DispatchResponse,
  ExecutionEvent,
  JsonValue,
  ListNodeTypesResponse,
  NodeTypeEntry,
  UndeployResponse,
  WorkflowContext,
  WorkflowEdge,
} from './generated';

export type { WorkflowContext as WorkflowResult } from './generated';

// ── 前端扩展类型（补充 Rust 侧不需要的画布/UI 字段） ───────

import type {
  ConnectionRecord as GeneratedConnectionRecord,
  JsonValue,
  WorkflowGraph as WorkflowGraphBase,
  WorkflowNodeDefinition as WorkflowNodeDefinitionBase,
} from './generated';

export type ConnectionHealthState =
  | 'idle'
  | 'connecting'
  | 'healthy'
  | 'degraded'
  | 'invalid'
  | 'reconnecting'
  | 'rateLimited'
  | 'circuitOpen'
  | 'timeout'
  | 'disconnected';

export interface ConnectionHealthSnapshot {
  phase: ConnectionHealthState;
  diagnosis?: string | null;
  recommendedAction?: string | null;
  lastStateChangedAt?: string | null;
  lastConnectedAt?: string | null;
  lastHeartbeatAt?: string | null;
  lastCheckedAt?: string | null;
  lastReleasedAt?: string | null;
  lastFailureAt?: string | null;
  lastFailureReason?: string | null;
  nextRetryAt?: string | null;
  circuitOpenUntil?: string | null;
  rateLimitedUntil?: string | null;
  consecutiveFailures: number;
  totalFailures: number;
  timeoutCount: number;
  rateLimitHits: number;
  reconnectAttempts: number;
  lastLatencyMs?: number | null;
}

export type ConnectionRecord = GeneratedConnectionRecord & {
  health?: ConnectionHealthSnapshot;
};

/**
 * 带有画布布局元数据的节点定义。
 *
 * 在 Rust 生成的基础类型上：
 * - `id` / `config` / `buffer` 改为可选（Rust 侧 `#[serde(default)]`，前端构造时可省略）
 * - 追加前端独有的 `meta.position` 画布坐标
 */
export type WorkflowNodeDefinition =
  Omit<WorkflowNodeDefinitionBase, 'id' | 'config' | 'buffer'> & {
    id?: string;
    config?: JsonValue;
    buffer?: number;
    meta?: {
      position?: {
        x: number;
        y: number;
      };
    };
  };

/**
 * 带有前端画布编辑器状态的工作流图。
 *
 * - `connections` 改为可选（Rust 侧 `#[serde(default)]`）
 * - `nodes` 使用扩展后的 `WorkflowNodeDefinition`
 * - 追加前端独有的 `editor_graph`（FlowGram 画布状态）
 */
export type WorkflowGraph = Omit<WorkflowGraphBase, 'connections' | 'nodes'> & {
  connections?: WorkflowGraphBase['connections'];
  nodes: Record<string, WorkflowNodeDefinition>;
  editor_graph?: FlowgramWorkflowJSON;
};

// ── 纯前端类型（不跨 IPC 边界） ────────────────────────────

export interface WorkflowLogicBranch {
  key: string;
  label?: string;
}

export interface WorkflowRuntimeState {
  traceId: string | null;
  lastEventType: 'started' | 'completed' | 'failed' | 'output' | null;
  lastNodeId: string | null;
  lastError: string | null;
  lastUpdatedAt: number | null;
  activeNodeIds: string[];
  completedNodeIds: string[];
  failedNodeIds: string[];
  outputNodeIds: string[];
}

export type WorkflowWindowStatus =
  | 'idle'
  | 'deployed'
  | 'running'
  | 'completed'
  | 'failed'
  | 'preview';

export interface RuntimeLogEntry {
  id: string;
  timestamp: number;
  level: 'info' | 'success' | 'warn' | 'error';
  source: string;
  message: string;
  detail?: string | null;
}

export interface AppErrorRecord {
  id: string;
  timestamp: number;
  scope: 'workflow' | 'command' | 'frontend' | 'runtime';
  title: string;
  detail?: string | null;
}

export interface ObservabilityEntry {
  id: string;
  timestamp: string;
  level: 'info' | 'success' | 'warn' | 'error';
  category: 'execution' | 'result' | 'audit' | 'alert' | string;
  source: string;
  message: string;
  detail?: string | null;
  traceId?: string | null;
  nodeId?: string | null;
  durationMs?: number | null;
  projectId?: string | null;
  projectName?: string | null;
  environmentId?: string | null;
  environmentName?: string | null;
  data?: JsonValue;
}

export interface AlertDeliveryRecord {
  id: string;
  timestamp: string;
  traceId: string;
  nodeId: string;
  projectId: string;
  projectName: string;
  environmentId: string;
  environmentName: string;
  url: string;
  method: string;
  status: number;
  success: boolean;
  webhookKind?: string | null;
  bodyMode?: string | null;
  requestTimeoutMs?: number | null;
  requestedAt?: string | null;
  requestBodyPreview?: string | null;
}

export interface ObservabilityTraceSummary {
  traceId: string;
  status: string;
  startedAt?: string | null;
  lastSeenAt?: string | null;
  totalEvents: number;
  nodeCount: number;
  outputCount: number;
  failureCount: number;
  totalDurationMs?: number | null;
  lastNodeId?: string | null;
  projectName?: string | null;
  environmentName?: string | null;
}

export interface ObservabilityQueryResult {
  entries: ObservabilityEntry[];
  traces: ObservabilityTraceSummary[];
  alerts: AlertDeliveryRecord[];
  audits: ObservabilityEntry[];
}

export type RuntimeBackpressureStrategy = 'block' | 'rejectNewest';

export interface WorkflowRuntimePolicy {
  manualQueueCapacity: number;
  triggerQueueCapacity: number;
  manualBackpressureStrategy: RuntimeBackpressureStrategy;
  triggerBackpressureStrategy: RuntimeBackpressureStrategy;
  maxRetryAttempts: number;
  initialRetryBackoffMs: number;
  maxRetryBackoffMs: number;
}

export interface DispatchLaneSnapshot {
  depth: number;
  accepted: number;
  retried: number;
  deadLettered: number;
}

export interface RuntimeWorkflowSummary {
  workflowId: string;
  projectId?: string | null;
  projectName?: string | null;
  environmentId?: string | null;
  environmentName?: string | null;
  deployedAt: string;
  nodeCount: number;
  edgeCount: number;
  rootNodes: string[];
  active: boolean;
  policy: WorkflowRuntimePolicy;
  manualLane: DispatchLaneSnapshot;
  triggerLane: DispatchLaneSnapshot;
}

export interface DeadLetterRecord {
  id: string;
  timestamp: string;
  workflowId: string;
  lane: 'manual' | 'trigger' | string;
  source: string;
  targetNodeId?: string | null;
  traceId: string;
  attempts: number;
  reason: string;
  projectId?: string | null;
  projectName?: string | null;
  environmentId?: string | null;
  environmentName?: string | null;
  payload: JsonValue;
}
