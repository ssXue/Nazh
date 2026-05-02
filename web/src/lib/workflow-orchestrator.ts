import type {
  AiCompletionRequest,
  AiConfigView,
  AiGenerationParams,
  AiMessage,
  AiProviderView,
  JsonValue,
  WorkflowEdge,
  WorkflowGraph,
  WorkflowNodeDefinition,
} from '../types';

import {
  buildDefaultNodeSeed,
  createFlowgramNodeRegistries,
  getAllNodeDefinitions,
  normalizeNodeConfig,
  resolveNodeDisplayLabel,
  type NazhNodeKind,
} from '../components/flowgram/flowgram-node-library';
import { toFlowgramWorkflowJson } from './flowgram';
import { copilotCompleteStream } from './tauri';
import { isUsableGlobalAiProvider, resolveGlobalAiProvider } from './workflow-ai';
import { allocateNodeId } from './workflow-node-id';
import {
  buildWorkflowAiNodeGuideText,
  getWorkflowAiAllowedNodeKinds,
  normalizeWorkflowAiNodeKind,
} from './workflow-node-capabilities';

const DEFAULT_WORKFLOW_TIMEOUT_MS = 90_000;
const MAX_WORKFLOW_TRANSPORT_RETRY_ATTEMPTS = 2;
const MAX_WORKFLOW_RESUME_ATTEMPTS = 2;
const MAX_WORKFLOW_RESTART_ATTEMPTS = 1;
const DEFAULT_COPILOT_PARAMS: AiGenerationParams = {
  temperature: 0.45,
  maxTokens: 4096,
  topP: 1,
};
const EMPTY_FLOWGRAM_CONNECTION_DEFAULTS = {
  any: null,
  modbus: null,
  serial: null,
  mqtt: null,
  http: null,
  bark: null,
};
const FLOWGRAM_NODE_REGISTRY_MAP = new Map(
  createFlowgramNodeRegistries(EMPTY_FLOWGRAM_CONNECTION_DEFAULTS).map((registry) => [
    registry.type,
    registry,
  ]),
);

const PROTOCOL_REQUIREMENTS_TEXT = `协议要求：
- 只输出 JSON Lines，每行一个 JSON 对象
- 不要输出 Markdown、代码块、解释文字或序号
- 一旦确定一个节点或一条边，就立即输出，不要等全部设计完再统一输出
- 先输出 project，再输出 node，再输出 edge，最后输出 done
- 编辑已有工作流时，只输出必要修改，不要重复未改动的节点
- 节点 ref 只用于本次编排会话内引用，不是系统 node id；真实 node id 由 Nazh 创建
- 不要编造不存在的节点类型
- connectionId 只有在用户明确给出可复用的连接 id 时才填写，否则留空或省略
- 输出的 payloadText 必须是合法 JSON 字符串`;

export type WorkflowOrchestrationMode = 'create' | 'edit';

export interface WorkflowOrchestrationDraft {
  name: string;
  description: string;
  payloadText: string;
  graph: WorkflowGraph;
}

export interface ProjectMetadataOperation {
  type: 'project';
  name?: string;
  description?: string;
  payloadText?: string;
}

export interface CreateNodeOperation {
  type: 'create_node';
  ref: string;
  nodeType: NazhNodeKind;
  label?: string;
  connectionId?: string | null;
  timeoutMs?: number | null;
  config?: JsonValue;
}

export interface UpdateNodeOperation {
  type: 'update_node';
  ref: string;
  nodeType?: NazhNodeKind;
  label?: string;
  connectionId?: string | null;
  timeoutMs?: number | null;
  config?: JsonValue;
}

export interface DeleteNodeOperation {
  type: 'delete_node';
  ref: string;
}

export interface CreateEdgeOperation {
  type: 'create_edge';
  fromRef: string;
  toRef: string;
  sourcePortId?: string;
  targetPortId?: string;
}

export interface DeleteEdgeOperation {
  type: 'delete_edge';
  fromRef: string;
  toRef: string;
  sourcePortId?: string;
  targetPortId?: string;
}

export interface DoneOperation {
  type: 'done';
  summary?: string;
}

export type WorkflowOrchestrationOperation =
  | ProjectMetadataOperation
  | CreateNodeOperation
  | UpdateNodeOperation
  | DeleteNodeOperation
  | CreateEdgeOperation
  | DeleteEdgeOperation
  | DoneOperation;

export interface WorkflowOrchestrationSessionState {
  draft: WorkflowOrchestrationDraft;
  nodeLabels: Record<string, string>;
  nodeRefs: Record<string, string>;
  operations: WorkflowOrchestrationOperation[];
  summary: string | null;
}

export interface StreamWorkflowOrchestrationOptions {
  mode: WorkflowOrchestrationMode;
  requirement: string;
  providerId: string;
  model?: string | null;
  baseDraft?: WorkflowOrchestrationDraft | null;
  params?: AiGenerationParams;
  timeoutMs?: number | null;
  onRawText?: (rawText: string) => void;
  onThinking?: (thinkingText: string) => void;
  onOperation?: (
    operation: WorkflowOrchestrationOperation,
    nextState: WorkflowOrchestrationSessionState,
  ) => void;
  onRetry?: (
    attempt: number,
    error: Error,
    nextState: WorkflowOrchestrationSessionState,
    strategy: WorkflowOrchestrationRetryStrategy,
  ) => void;
}

export type WorkflowOrchestrationRetryStrategy = 'retry' | 'resume' | 'restart';

function buildIncompleteProtocolError(
  rawText: string,
  state: WorkflowOrchestrationSessionState,
  finishReason?: string,
): Error {
  const hasOperations = state.operations.length > 0;
  const hasRawOutput = rawText.trim().length > 0;
  const normalizedFinishReason = finishReason?.trim().toLowerCase();

  if (normalizedFinishReason === 'length') {
    return new Error(
      hasOperations
        ? 'AI 流式输出因 token 上限提前结束，工作流协议未完成：缺少 done 操作。请提高 maxTokens，或缩小本次编排范围。'
        : 'AI 流式输出因 token 上限提前结束，且没有返回可解析的工作流操作。请提高 maxTokens，或缩小本次编排范围。',
    );
  }

  if (!hasOperations) {
    return new Error(
      hasRawOutput
        ? `AI 已结束输出，但没有返回可解析的工作流操作。请要求它严格只输出 JSON Lines。${normalizedFinishReason ? ` 结束原因：${normalizedFinishReason}。` : ''}`
        : 'AI 已结束输出，但没有返回任何工作流操作。',
    );
  }

  return new Error(
    `AI 流式输出已结束，但工作流协议未完成：缺少 done 操作。${normalizedFinishReason ? ` 结束原因：${normalizedFinishReason}。` : ''}`,
  );
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null && !Array.isArray(value);
}

function hasOwnKey<T extends object>(value: T, key: string): boolean {
  return Object.prototype.hasOwnProperty.call(value, key);
}

/** 动态校验 AI 输出的 nodeType 是否合法。 */
function normalizeAllowedNodeKind(value: unknown): NazhNodeKind | null {
  return normalizeWorkflowAiNodeKind(value);
}

function isRecoverableWorkflowOrchestrationError(error: Error): boolean {
  const message = error.message.trim().toLowerCase();
  return [
    'error decoding response body',
    '未收到结束信号',
    'connection reset',
    'broken pipe',
    'unexpected eof',
    'unexpected end of file',
    'token 上限提前结束',
    '缺少 done 操作',
    '没有返回可解析的工作流操作',
    '没有返回任何工作流操作',
    'stream interrupted',
  ].some((pattern) => message.includes(pattern));
}

function isRecoverableWorkflowTransportError(error: Error): boolean {
  const message = error.message.trim().toLowerCase();
  return [
    'error decoding response body',
    '未收到结束信号',
    'connection reset',
    'broken pipe',
    'unexpected eof',
    'unexpected end of file',
    'stream interrupted',
  ].some((pattern) => message.includes(pattern));
}

function hasWorkflowProtocolFragment(rawText: string): boolean {
  const trimmed = rawText.trim();
  if (!trimmed) {
    return false;
  }

  if (parseOperationLine(trimmed) !== null) {
    return true;
  }

  return trimmed
    .split(/\r?\n/)
    .some((line) => {
      const normalizedLine = line.trim().replace(/^data:\s*/i, '');
      return normalizedLine.startsWith('{') || normalizedLine.includes('"type"');
    });
}

function canResumeWorkflowOrchestration(
  state: WorkflowOrchestrationSessionState,
  rawText: string,
): boolean {
  return state.operations.length > 0 || hasWorkflowProtocolFragment(rawText);
}

async function waitForWorkflowRetry(attempt: number): Promise<void> {
  const delayMs = Math.min(400 * 2 ** Math.max(0, attempt - 1), 1_500);
  await new Promise<void>((resolve) => {
    globalThis.setTimeout(resolve, delayMs);
  });
}

function readString(record: Record<string, unknown>, keys: string[]): string | undefined {
  for (const key of keys) {
    const value = record[key];
    if (typeof value === 'string' && value.trim()) {
      return value.trim();
    }
  }

  return undefined;
}

function readFiniteNumber(
  record: Record<string, unknown>,
  keys: string[],
): number | undefined {
  for (const key of keys) {
    const value = record[key];
    if (typeof value === 'number' && Number.isFinite(value)) {
      return value;
    }
  }

  return undefined;
}

function mergeJsonValue(baseValue: JsonValue, patchValue: JsonValue): JsonValue {
  if (Array.isArray(baseValue) || Array.isArray(patchValue)) {
    return patchValue;
  }

  if (isRecord(baseValue) && isRecord(patchValue)) {
    return Object.entries(patchValue).reduce<Record<string, JsonValue>>(
      (acc, [key, value]) => {
        const currentValue = acc[key] ?? null;
        acc[key] =
          isRecord(currentValue) && isRecord(value)
            ? mergeJsonValue(currentValue as JsonValue, value as JsonValue)
            : (value as JsonValue);
        return acc;
      },
      { ...(baseValue as Record<string, JsonValue>) },
    );
  }

  return patchValue;
}

function normalizeTimeoutMs(value: number | null | undefined): number | undefined {
  if (typeof value !== 'number' || !Number.isFinite(value) || value <= 0) {
    return undefined;
  }

  return Math.round(value);
}

function normalizePayloadText(value: unknown): string {
  if (typeof value === 'string' && value.trim()) {
    const trimmed = value.trim();
    try {
      return JSON.stringify(JSON.parse(trimmed), null, 2);
    } catch {
      return JSON.stringify({ request: trimmed }, null, 2);
    }
  }

  if (value === null || value === undefined) {
    return '{}';
  }

  try {
    return JSON.stringify(value, null, 2);
  } catch {
    return '{}';
  }
}

function normalizeEdgePortId(value: unknown): string | undefined {
  return typeof value === 'string' && value.trim() ? value.trim() : undefined;
}

function buildEdgeKey(edge: Pick<WorkflowEdge, 'from' | 'to' | 'source_port_id' | 'target_port_id'>): string {
  return `${edge.from}:${edge.source_port_id ?? ''}->${edge.to}:${edge.target_port_id ?? ''}`;
}

function buildGraphWithLabels(
  graph: WorkflowGraph,
  nodeLabels: Record<string, string>,
): WorkflowGraph {
  const editorGraph = toFlowgramWorkflowJson({
    ...graph,
    editor_graph: undefined,
  });

  const nextEditorGraph = {
    ...editorGraph,
    nodes: editorGraph.nodes.map((node) => {
      const defaults = FLOWGRAM_NODE_REGISTRY_MAP.get(String(node.type))?.onAdd?.();
      return {
        ...node,
        blocks: node.blocks ?? defaults?.blocks,
        edges: node.edges ?? defaults?.edges,
        data: {
          ...(isRecord(node.data) ? node.data : {}),
          label: resolveNodeDisplayLabel(node.type, nodeLabels[node.id]),
        },
      };
    }),
  };

  return {
    ...graph,
    editor_graph: nextEditorGraph,
  };
}

function buildStateDraft(
  name: string,
  description: string,
  payloadText: string,
  nodes: Record<string, WorkflowNodeDefinition>,
  edges: WorkflowEdge[],
  nodeLabels: Record<string, string>,
): WorkflowOrchestrationDraft {
  const baseGraph: WorkflowGraph = {
    name,
    connections: [],
    nodes,
    edges,
  };

  return {
    name,
    description,
    payloadText,
    graph: buildGraphWithLabels(baseGraph, nodeLabels),
  };
}

export function createEmptyWorkflowDraft(name = 'AI 编排草稿'): WorkflowOrchestrationDraft {
  return {
    name,
    description: 'AI 正在编排工作流。',
    payloadText: '{}',
    graph: buildGraphWithLabels(
      {
        name,
        connections: [],
        nodes: {},
        edges: [],
      },
      {},
    ),
  };
}

function toWorkflowNodeRefPrefix(value: string): string {
  return (
    value
      .trim()
      .replace(/([a-z0-9])([A-Z])/g, '$1_$2')
      .toLowerCase()
      .replace(/[^a-z0-9_]+/g, '_')
      .replace(/^_+|_+$/g, '') || 'node'
  );
}

function buildInitialNodeRefs(graph: WorkflowGraph): Record<string, string> {
  const usedRefs = new Set<string>();
  return Object.entries(graph.nodes).reduce<Record<string, string>>((acc, [nodeId, node]) => {
    const prefix = toWorkflowNodeRefPrefix(node.type);
    const ref = allocateNodeId(prefix, usedRefs);
    usedRefs.add(ref);
    acc[ref] = nodeId;
    return acc;
  }, {});
}

function invertNodeRefs(nodeRefs: Record<string, string>): Record<string, string> {
  return Object.entries(nodeRefs).reduce<Record<string, string>>((acc, [ref, nodeId]) => {
    acc[nodeId] = ref;
    return acc;
  }, {});
}

export function createWorkflowOrchestrationState(
  baseDraft?: WorkflowOrchestrationDraft | null,
): WorkflowOrchestrationSessionState {
  const draft = baseDraft ?? createEmptyWorkflowDraft();
  const nodeRefs = buildInitialNodeRefs(draft.graph);
  const nodeLabels = Object.keys(draft.graph.nodes).reduce<Record<string, string>>((acc, nodeId) => {
    const node = draft.graph.nodes[nodeId];
    const editorNode = draft.graph.editor_graph?.nodes.find((item) => item.id === nodeId);
    const editorData = isRecord(editorNode?.data) ? editorNode.data : {};
    acc[nodeId] = resolveNodeDisplayLabel(node?.type, editorData.label);
    return acc;
  }, {});

  return {
    draft: buildStateDraft(
      draft.name,
      draft.description,
      normalizePayloadText(draft.payloadText),
      { ...draft.graph.nodes },
      [...(draft.graph.edges ?? [])],
      nodeLabels,
    ),
    nodeLabels,
    nodeRefs,
    operations: [],
    summary: null,
  };
}

export function describeWorkflowOrchestrationOperation(
  operation: WorkflowOrchestrationOperation,
): string {
  switch (operation.type) {
    case 'project':
      return `更新工程信息${operation.name ? `：${operation.name}` : ''}`;
    case 'create_node':
      return `创建节点 ${operation.ref} · ${operation.nodeType}`;
    case 'update_node':
      return `更新节点 ${operation.ref}${operation.nodeType ? ` · ${operation.nodeType}` : ''}`;
    case 'delete_node':
      return `删除节点 ${operation.ref}`;
    case 'create_edge':
      return `连接 ${operation.fromRef} -> ${operation.toRef}`;
    case 'delete_edge':
      return `移除连线 ${operation.fromRef} -> ${operation.toRef}`;
    case 'done':
      return operation.summary?.trim() ? operation.summary.trim() : 'AI 编排完成';
  }
}

export function resolvePreferredWorkflowAiProvider(
  aiConfig: AiConfigView | null,
): AiProviderView | null {
  const globalProvider = resolveGlobalAiProvider(aiConfig);
  if (isUsableGlobalAiProvider(globalProvider)) {
    return globalProvider;
  }

  return aiConfig?.providers.find((provider) => isUsableGlobalAiProvider(provider)) ?? null;
}

export function getWorkflowAiUnavailableReason(aiConfig: AiConfigView | null): string {
  const preferredProvider = resolvePreferredWorkflowAiProvider(aiConfig);
  if (preferredProvider) {
    return `使用 ${preferredProvider.name} 进行 AI 编排`;
  }

  if (!aiConfig || aiConfig.providers.length === 0) {
    return '请先在 AI 配置中添加可用提供商';
  }

  const activeProvider = aiConfig.activeProviderId
    ? aiConfig.providers.find((provider) => provider.id === aiConfig.activeProviderId) ?? null
    : null;

  if (activeProvider && !activeProvider.enabled) {
    return `全局 AI ${activeProvider.name} 已被禁用`;
  }

  if (activeProvider && !activeProvider.hasApiKey) {
    return `请先为全局 AI ${activeProvider.name} 配置 API Key`;
  }

  if (aiConfig.providers.some((provider) => provider.enabled)) {
    return '请先为已启用的 AI 提供商配置 API Key';
  }

  return '请先启用一个 AI 提供商';
}

function buildExistingGraphSummary(state: WorkflowOrchestrationSessionState): string {
  const draft = state.draft;
  const refByNodeId = invertNodeRefs(state.nodeRefs);

  return JSON.stringify(
    {
      project: {
        name: draft.name,
        description: draft.description,
        payloadText: normalizePayloadText(draft.payloadText),
      },
      graph: {
        name: draft.graph.name,
        nodes: Object.entries(draft.graph.nodes).map(([nodeId, node]) => ({
          ref: refByNodeId[nodeId] ?? null,
          type: node.type,
          connection_id: node.connection_id ?? null,
          timeout_ms: node.timeout_ms ?? null,
          config: node.config ?? {},
        })),
        edges: (draft.graph.edges ?? []).map((edge) => ({
          fromRef: refByNodeId[edge.from] ?? null,
          toRef: refByNodeId[edge.to] ?? null,
          source_port_id: edge.source_port_id ?? null,
          target_port_id: edge.target_port_id ?? null,
        })),
      },
    },
    null,
    2,
  );
}

function buildWorkflowAiSourcePortGuideText(): string {
  return getAllNodeDefinitions()
    .map((definition) => {
      const seed = definition.buildDefaultSeed();
      const branches = definition.getRoutingBranches?.(seed.config) ?? [];
      if (branches.length === 0) {
        return null;
      }

      if (definition.kind === 'switch') {
        return '- switch: sourcePortId 使用该节点 config.branches[].key；兜底分支使用 default。';
      }

      return `- ${definition.kind}: sourcePortId 只能是 ${branches.map((branch) => branch.key).join(' / ')}。`;
    })
    .filter((line): line is string => line !== null)
    .join('\n');
}

export function buildWorkflowOrchestrationPrompt(options: {
  mode: WorkflowOrchestrationMode;
  requirement: string;
  baseDraft?: WorkflowOrchestrationDraft | null;
  baseState?: WorkflowOrchestrationSessionState | null;
}): AiMessage[] {
  const { mode, requirement, baseDraft, baseState } = options;
  const promptState =
    baseState ?? (baseDraft ? createWorkflowOrchestrationState(baseDraft) : null);
  const currentGraphText =
    mode === 'edit' && promptState
      ? buildExistingGraphSummary(promptState)
      : '当前从空白工作流开始。';
  const sourcePortGuideText = buildWorkflowAiSourcePortGuideText();

  const userPrompt = `任务模式：${mode === 'create' ? 'create（从空白开始编排新工作流）' : 'edit（基于当前工作流流式修改）'}

用户需求：
${requirement.trim()}

当前工作流上下文：
${currentGraphText}

请严格输出 JSON Lines 操作流，逐步编排。`;

  return [
    {
      role: 'system',
      content: `你是 Nazh 的工业工作流 AI 编排助手。你必须使用可流式消费的 JSON Lines 操作协议完成工作流创建或编辑。

可用节点类型与建议：
${buildWorkflowAiNodeGuideText()}

${PROTOCOL_REQUIREMENTS_TEXT}

操作格式：
{"type":"project","name":"工程名","description":"说明","payloadText":"{\\"manual\\":true}"}
{"type":"create_node","ref":"timer","nodeType":"timer","label":"定时触发","timeoutMs":null,"config":{"interval_ms":5000,"immediate":true,"inject":{"source":"timer"}}}
{"type":"update_node","ref":"timer","config":{"interval_ms":10000}}
{"type":"create_edge","fromRef":"timer","toRef":"debug"}
{"type":"delete_node","ref":"old"}
{"type":"delete_edge","fromRef":"old_a","toRef":"old_b"}
{"type":"done","summary":"完成摘要"}

注意：
- nodeType 只能从 ${getWorkflowAiAllowedNodeKinds().join(', ')} 中选择
${sourcePortGuideText}
- ref 必须是本次会话内稳定、简短的英文别名；引用已有节点时只能使用“当前工作流上下文”里给出的 ref
- 新建节点时自己选择 ref，但不要输出或猜测系统 node id
- code 节点脚本只输出 Rhai 可执行逻辑，不要使用未声明 API
- 对于工业场景，优先给出可以直接继续编辑和绑定连接的稳定草图
- 如果需求不清晰，优先从最小可运行链路开始，再补上分支和输出
- 保持输出紧凑；每个 JSON 对象单独一行`,
    },
    {
      role: 'user',
      content: userPrompt,
    },
  ];
}

function buildWorkflowOrchestrationContinuationPrompt(options: {
  mode: WorkflowOrchestrationMode;
  requirement: string;
  acceptedState: WorkflowOrchestrationSessionState;
  rawText: string;
  error: Error;
  attempt: number;
}): AiMessage[] {
  const baseMessages = buildWorkflowOrchestrationPrompt({
    mode: options.mode,
    requirement: options.requirement,
    baseState: options.acceptedState,
  });
  const acceptedOperationsText = options.acceptedState.operations
    .map((operation) => JSON.stringify(operation))
    .join('\n');
  const normalizedRawText = options.rawText.trim();
  const rawLines = normalizedRawText ? normalizedRawText.split(/\r?\n/) : [];
  const lastRawLine = rawLines[rawLines.length - 1] ?? '';
  const isLastLineComplete = parseOperationLine(lastRawLine) !== null;
  const assistantTranscript = [
    acceptedOperationsText,
    normalizedRawText && !isLastLineComplete ? lastRawLine : '',
  ]
    .filter((segment) => segment.trim().length > 0)
    .join('\n');

  return [
    baseMessages[0],
    baseMessages[1],
    {
      role: 'assistant',
      content: assistantTranscript,
    },
    {
      role: 'user',
      content: `上一条 assistant 输出在流式传输中断了，这是第 ${options.attempt} 次续传。

中断原因：
${options.error.message}

继续规则：
- 把上面的 assistant 内容视为你已经成功输出并已被系统应用
- 不要重复任何已经完整输出过的 JSON Lines 操作
- 如果最后一行是不完整片段，直接从下一条完整操作继续，不要回头解释
- 从现在开始直接输出剩余 JSON Lines；不要补写思考说明、不要输出 Markdown
- 只在确实完成后输出 {"type":"done", ...}`,
    },
  ];
}

function toPositiveRoundedNumber(value: number | undefined): number | null | undefined {
  if (value === undefined) {
    return undefined;
  }

  if (!Number.isFinite(value) || value <= 0) {
    return null;
  }

  return Math.round(value);
}

function normalizeOperation(input: unknown): WorkflowOrchestrationOperation | null {
  if (!isRecord(input)) {
    return null;
  }

  const rawType = readString(input, ['type', 'op', 'action', 'kind']);
  if (!rawType) {
    return null;
  }

  switch (rawType) {
    case 'project':
    case 'project_meta':
      return {
        type: 'project',
        name: readString(input, ['name']),
        description: readString(input, ['description', 'desc']),
        payloadText: readString(input, ['payloadText', 'payload_text']),
      };
    case 'create_node': {
      const ref = readString(input, ['ref']);
      const nodeType = normalizeAllowedNodeKind(
        readString(input, ['nodeType', 'node_type', 'kind']),
      );
      if (!ref || !nodeType) {
        return null;
      }

      const config = isRecord(input.config) || Array.isArray(input.config)
        ? (input.config as JsonValue)
        : undefined;
      const hasConnectionId =
        hasOwnKey(input, 'connectionId') || hasOwnKey(input, 'connection_id');
      const nextConnectionId = hasConnectionId
        ? readString(input, ['connectionId', 'connection_id']) ?? null
        : undefined;

      return {
        type: 'create_node',
        ref,
        nodeType,
        label: readString(input, ['label', 'title']),
        connectionId: nextConnectionId,
        timeoutMs: toPositiveRoundedNumber(
          readFiniteNumber(input, ['timeoutMs', 'timeout_ms']),
        ),
        config,
      };
    }
    case 'update_node': {
      const ref = readString(input, ['ref']);
      if (!ref) {
        return null;
      }

      const rawNodeType = readString(input, ['nodeType', 'node_type', 'kind']);
      let nodeType: NazhNodeKind | undefined;
      if (rawNodeType) {
        const normalizedNodeType = normalizeAllowedNodeKind(rawNodeType);
        if (!normalizedNodeType) {
          return null;
        }
        nodeType = normalizedNodeType;
      }
      const config = isRecord(input.config) || Array.isArray(input.config)
        ? (input.config as JsonValue)
        : undefined;
      const hasConnectionId =
        hasOwnKey(input, 'connectionId') || hasOwnKey(input, 'connection_id');
      const nextConnectionId = hasConnectionId
        ? readString(input, ['connectionId', 'connection_id']) ?? null
        : undefined;

      return {
        type: 'update_node',
        ref,
        nodeType,
        label: readString(input, ['label', 'title']),
        connectionId: nextConnectionId,
        timeoutMs: toPositiveRoundedNumber(
          readFiniteNumber(input, ['timeoutMs', 'timeout_ms']),
        ),
        config,
      };
    }
    case 'delete_node': {
      const ref = readString(input, ['ref']);
      if (!ref) {
        return null;
      }
      return {
        type: 'delete_node',
        ref,
      };
    }
    case 'create_edge': {
      const fromRef = readString(input, ['fromRef']);
      const toRef = readString(input, ['toRef']);
      if (!fromRef || !toRef) {
        return null;
      }

      return {
        type: 'create_edge',
        fromRef,
        toRef,
        sourcePortId: readString(input, ['sourcePortId', 'source_port_id', 'sourcePort']),
        targetPortId: readString(input, ['targetPortId', 'target_port_id', 'targetPort']),
      };
    }
    case 'delete_edge': {
      const fromRef = readString(input, ['fromRef']);
      const toRef = readString(input, ['toRef']);
      if (!fromRef || !toRef) {
        return null;
      }

      return {
        type: 'delete_edge',
        fromRef,
        toRef,
        sourcePortId: readString(input, ['sourcePortId', 'source_port_id', 'sourcePort']),
        targetPortId: readString(input, ['targetPortId', 'target_port_id', 'targetPort']),
      };
    }
    case 'done':
    case 'complete':
      return {
        type: 'done',
        summary: readString(input, ['summary', 'message']),
      };
    default:
      return null;
  }
}

function parseOperationLine(line: string): WorkflowOrchestrationOperation | null {
  const trimmed = line.trim();
  if (!trimmed || trimmed === '```') {
    return null;
  }

  const normalizedLine = trimmed.replace(/^data:\s*/i, '');
  const startIndex = normalizedLine.indexOf('{');
  const endIndex = normalizedLine.lastIndexOf('}');
  if (startIndex === -1 || endIndex === -1 || endIndex <= startIndex) {
    return null;
  }

  try {
    return normalizeOperation(JSON.parse(normalizedLine.slice(startIndex, endIndex + 1)));
  } catch {
    return null;
  }
}

function resolveNodeLabel(
  nodeId: string,
  nodeType: NazhNodeKind,
  nextLabel: string | undefined,
  currentLabels: Record<string, string>,
): string {
  if (nextLabel && nextLabel.trim()) {
    return nextLabel.trim();
  }

  const currentLabel = currentLabels[nodeId];
  return resolveNodeDisplayLabel(
    nodeType,
    currentLabel && currentLabel !== nodeId ? currentLabel : undefined,
  );
}

function resolveRequiredNodeId(
  nodeRefs: Record<string, string>,
  ref: string,
): string {
  const nodeId = nodeRefs[ref];
  if (!nodeId) {
    throw new Error(`AI 编排引用了未知节点 ref: ${ref}`);
  }

  return nodeId;
}

function removeNodeRefs(
  nodeRefs: Record<string, string>,
  nodeId: string,
): Record<string, string> {
  return Object.fromEntries(
    Object.entries(nodeRefs).filter(([, currentNodeId]) => currentNodeId !== nodeId),
  );
}

export function applyWorkflowOrchestrationOperation(
  state: WorkflowOrchestrationSessionState,
  operation: WorkflowOrchestrationOperation,
): WorkflowOrchestrationSessionState {
  let nextName = state.draft.name;
  let nextDescription = state.draft.description;
  let nextPayloadText = state.draft.payloadText;
  let nextNodes = { ...state.draft.graph.nodes };
  let nextNodeLabels = { ...state.nodeLabels };
  let nextNodeRefs = { ...state.nodeRefs };
  let nextEdges = [...(state.draft.graph.edges ?? [])];
  let nextSummary = state.summary;

  switch (operation.type) {
    case 'project':
      nextName = operation.name?.trim() || nextName;
      nextDescription = operation.description?.trim() || nextDescription;
      if (operation.payloadText !== undefined) {
        nextPayloadText = normalizePayloadText(operation.payloadText);
      }
      break;
    case 'create_node': {
      const defaultSeed = buildDefaultNodeSeed(operation.nodeType);
      const existingNodeId = nextNodeRefs[operation.ref];
      const usedNodeIds = new Set([
        ...Object.keys(nextNodes),
        ...Object.values(nextNodeRefs),
      ]);
      const nodeId = existingNodeId ?? allocateNodeId(defaultSeed.idPrefix, usedNodeIds);
      const existingNode = nextNodes[nodeId];
      const mergedConfig = mergeJsonValue(
        (existingNode?.config ?? defaultSeed.config) as JsonValue,
        (operation.config ?? {}) as JsonValue,
      );
      const normalizedConfig = normalizeNodeConfig(operation.nodeType, mergedConfig);

      nextNodeRefs[operation.ref] = nodeId;
      nextNodeLabels[nodeId] = resolveNodeLabel(
        nodeId,
        operation.nodeType,
        operation.label,
        nextNodeLabels,
      );

      nextNodes[nodeId] = {
        id: nodeId,
        type: operation.nodeType,
        connection_id:
          operation.connectionId !== undefined
            ? operation.connectionId?.trim() || undefined
            : existingNode?.connection_id,
        timeout_ms:
          operation.timeoutMs !== undefined
            ? normalizeTimeoutMs(operation.timeoutMs)
            : existingNode?.timeout_ms ?? normalizeTimeoutMs(defaultSeed.timeoutMs ?? undefined),
        config: normalizedConfig as JsonValue,
        meta: existingNode?.meta,
      };
      break;
    }
    case 'update_node': {
      const nodeId = resolveRequiredNodeId(nextNodeRefs, operation.ref);
      const existingNode = nextNodes[nodeId];
      if (!existingNode) {
        throw new Error(`AI 编排引用的节点 ref 已不存在: ${operation.ref}`);
      }
      const nodeType =
        operation.nodeType ?? normalizeAllowedNodeKind(existingNode.type) ?? 'native';
      const defaultSeed = buildDefaultNodeSeed(nodeType);
      const baseConfig =
        operation.nodeType && operation.nodeType !== existingNode.type
          ? defaultSeed.config
          : existingNode.config ?? defaultSeed.config;
      const mergedConfig = mergeJsonValue(
        baseConfig as JsonValue,
        (operation.config ?? {}) as JsonValue,
      );
      const normalizedConfig = normalizeNodeConfig(nodeType, mergedConfig);

      nextNodeLabels[nodeId] = resolveNodeLabel(
        nodeId,
        nodeType,
        operation.label,
        nextNodeLabels,
      );

      nextNodes[nodeId] = {
        ...existingNode,
        id: nodeId,
        type: nodeType,
        connection_id:
          operation.connectionId !== undefined
            ? operation.connectionId?.trim() || undefined
            : existingNode.connection_id,
        timeout_ms:
          operation.timeoutMs !== undefined
            ? normalizeTimeoutMs(operation.timeoutMs)
            : existingNode.timeout_ms,
        config: normalizedConfig as JsonValue,
      };
      break;
    }
    case 'delete_node': {
      const nodeId = resolveRequiredNodeId(nextNodeRefs, operation.ref);
      delete nextNodes[nodeId];
      delete nextNodeLabels[nodeId];
      nextNodeRefs = removeNodeRefs(nextNodeRefs, nodeId);
      nextEdges = nextEdges.filter((edge) => edge.from !== nodeId && edge.to !== nodeId);
      break;
    }
    case 'create_edge': {
      const fromNodeId = resolveRequiredNodeId(nextNodeRefs, operation.fromRef);
      const toNodeId = resolveRequiredNodeId(nextNodeRefs, operation.toRef);
      const nextEdge: WorkflowEdge = {
        from: fromNodeId,
        to: toNodeId,
        source_port_id: normalizeEdgePortId(operation.sourcePortId),
        target_port_id: normalizeEdgePortId(operation.targetPortId),
      };
      const nextEdgeKey = buildEdgeKey(nextEdge);
      nextEdges = nextEdges.filter((edge) => buildEdgeKey(edge) !== nextEdgeKey);
      nextEdges.push(nextEdge);
      break;
    }
    case 'delete_edge': {
      const fromNodeId = resolveRequiredNodeId(nextNodeRefs, operation.fromRef);
      const toNodeId = resolveRequiredNodeId(nextNodeRefs, operation.toRef);
      const nextEdgeKey = buildEdgeKey({
        from: fromNodeId,
        to: toNodeId,
        source_port_id: normalizeEdgePortId(operation.sourcePortId),
        target_port_id: normalizeEdgePortId(operation.targetPortId),
      });
      nextEdges = nextEdges.filter((edge) => buildEdgeKey(edge) !== nextEdgeKey);
      break;
    }
    case 'done':
      nextSummary = operation.summary?.trim() || nextSummary;
      break;
  }

  return {
    draft: buildStateDraft(
      nextName,
      nextDescription,
      nextPayloadText,
      nextNodes,
      nextEdges,
      nextNodeLabels,
    ),
    nodeLabels: nextNodeLabels,
    nodeRefs: nextNodeRefs,
    operations: [...state.operations, operation],
    summary: nextSummary,
  };
}

function resolveGenerationParams(params?: AiGenerationParams): AiGenerationParams {
  const resolved: AiGenerationParams = {
    temperature: params?.temperature ?? DEFAULT_COPILOT_PARAMS.temperature,
    maxTokens: params?.maxTokens ?? DEFAULT_COPILOT_PARAMS.maxTokens,
    topP: params?.topP ?? DEFAULT_COPILOT_PARAMS.topP,
  };
  if (params?.thinking) {
    resolved.thinking = params.thinking;
  }
  if (params?.reasoningEffort) {
    resolved.reasoningEffort = params.reasoningEffort;
  }
  return resolved;
}

function consumeOperationLines(
  rawText: string,
  processedLength: number,
): {
  nextProcessedLength: number;
  operations: WorkflowOrchestrationOperation[];
} {
  const unprocessedText = rawText.slice(processedLength);
  if (!unprocessedText) {
    return {
      nextProcessedLength: processedLength,
      operations: [],
    };
  }

  const newlineMatches = [...unprocessedText.matchAll(/\r?\n/g)];
  const lastNewline = newlineMatches[newlineMatches.length - 1];
  if (!lastNewline || lastNewline.index === undefined) {
    return {
      nextProcessedLength: processedLength,
      operations: [],
    };
  }

  const consumedText = unprocessedText.slice(0, lastNewline.index + lastNewline[0].length);
  const completeLines = consumedText
    .split(/\r?\n/)
    .filter((line) => line.trim().length > 0);

  return {
    nextProcessedLength: processedLength + consumedText.length,
    operations: completeLines
      .map((line) => parseOperationLine(line))
      .filter((operation): operation is WorkflowOrchestrationOperation => Boolean(operation)),
  };
}

function extractTrailingOperation(rawText: string): WorkflowOrchestrationOperation | null {
  const lastLine = rawText.split(/\r?\n/).pop() ?? '';
  return parseOperationLine(lastLine);
}

function applyTrailingOperationIfNeeded(
  state: WorkflowOrchestrationSessionState,
  rawText: string,
  onOperation?: StreamWorkflowOrchestrationOptions['onOperation'],
): WorkflowOrchestrationSessionState {
  const trailingOperation = extractTrailingOperation(rawText);
  const lastAppliedOperation = state.operations[state.operations.length - 1] ?? null;
  const shouldApplyTrailingOperation =
    trailingOperation !== null &&
    (!lastAppliedOperation ||
      JSON.stringify(lastAppliedOperation) !== JSON.stringify(trailingOperation));

  if (!shouldApplyTrailingOperation) {
    return state;
  }

  const nextState = applyWorkflowOrchestrationOperation(state, trailingOperation);
  onOperation?.(trailingOperation, nextState);
  return nextState;
}

export async function streamWorkflowOrchestration(
  options: StreamWorkflowOrchestrationOptions,
): Promise<WorkflowOrchestrationSessionState> {
  let nextState = createWorkflowOrchestrationState(options.baseDraft);
  let transportRetryCount = 0;
  let resumeAttemptCount = 0;
  let restartAttemptCount = 0;
  let nextAttemptMode: 'initial' | 'resume' | 'restart' = 'initial';
  let lastInterruptedRawText = '';
  let lastInterruptedError: Error | null = null;
  let combinedThinkingText = '';
  let lastDeliveredThinkingText = '';
  let preserveThinkingOnNextResume = false;
  let combinedRawText = '';
  let lastDeliveredRawText = '';
  let preserveRawTextOnNextResume = false;

  while (true) {
    const messages =
      nextAttemptMode === 'resume' && lastInterruptedError
        ? buildWorkflowOrchestrationContinuationPrompt({
            mode: options.mode,
            requirement: options.requirement,
            acceptedState: nextState,
            rawText: lastInterruptedRawText,
            error: lastInterruptedError,
            attempt: resumeAttemptCount,
          })
        : buildWorkflowOrchestrationPrompt({
            mode: options.mode,
            requirement: options.requirement,
            baseState: nextState,
          });

    const request: AiCompletionRequest = {
      providerId: options.providerId,
      model: options.model ?? undefined,
      messages,
      params: resolveGenerationParams(options.params),
      timeoutMs: options.timeoutMs ?? DEFAULT_WORKFLOW_TIMEOUT_MS,
    };

    const attemptStartOperationCount = nextState.operations.length;
    const attemptBaseRawText = combinedRawText;
    const attemptBaseThinkingText = combinedThinkingText;
    let processedLength = 0;
    let currentAttemptRawText = '';

    try {
      const streamResult = await copilotCompleteStream(
        request,
        (accumulatedText) => {
          currentAttemptRawText = accumulatedText;
          const visibleRawText =
            preserveRawTextOnNextResume && combinedRawText.trim().length > 0
              ? `${combinedRawText}\n${accumulatedText}`
              : accumulatedText;
          lastDeliveredRawText = visibleRawText;
          options.onRawText?.(visibleRawText);
          const parsed = consumeOperationLines(accumulatedText, processedLength);
          processedLength = parsed.nextProcessedLength;

          for (const operation of parsed.operations) {
            nextState = applyWorkflowOrchestrationOperation(nextState, operation);
            options.onOperation?.(operation, nextState);
          }
        },
        (thinkingText) => {
          if (preserveThinkingOnNextResume && nextState.operations.length > 0) {
            return;
          }
          const visibleThinkingText =
            preserveThinkingOnNextResume && combinedThinkingText.trim().length > 0
              ? `${combinedThinkingText}\n${thinkingText}`
              : thinkingText;
          lastDeliveredThinkingText = visibleThinkingText;
          options.onThinking?.(visibleThinkingText);
        },
        {
          maxRetries: 0,
        },
      );
      const rawText = streamResult.text;
      combinedRawText = lastDeliveredRawText || rawText;
      combinedThinkingText = lastDeliveredThinkingText;
      preserveThinkingOnNextResume = false;
      preserveRawTextOnNextResume = false;
      transportRetryCount = 0;
      nextState = applyTrailingOperationIfNeeded(nextState, rawText, options.onOperation);

      const hasCompletionOperation = nextState.operations.some((operation) => operation.type === 'done');
      if (!hasCompletionOperation) {
        throw buildIncompleteProtocolError(rawText, nextState, streamResult.finishReason);
      }

      return nextState;
    } catch (error) {
      const normalizedError =
        error instanceof Error ? error : new Error(String(error));
      nextState = applyTrailingOperationIfNeeded(nextState, currentAttemptRawText, options.onOperation);
      combinedRawText = lastDeliveredRawText || combinedRawText;
      combinedThinkingText = lastDeliveredThinkingText || combinedThinkingText;
      const attemptProducedProtocolProgress =
        nextState.operations.length > attemptStartOperationCount ||
        hasWorkflowProtocolFragment(currentAttemptRawText);
      const shouldResumeCurrentAttempt = canResumeWorkflowOrchestration(
        nextState,
        currentAttemptRawText,
      );

      if (
        isRecoverableWorkflowTransportError(normalizedError) &&
        !attemptProducedProtocolProgress &&
        transportRetryCount < MAX_WORKFLOW_TRANSPORT_RETRY_ATTEMPTS
      ) {
        transportRetryCount += 1;
        combinedRawText = attemptBaseRawText;
        combinedThinkingText = attemptBaseThinkingText;
        lastDeliveredRawText = attemptBaseRawText;
        lastDeliveredThinkingText = attemptBaseThinkingText;
        options.onRetry?.(transportRetryCount, normalizedError, nextState, 'retry');
        options.onRawText?.(attemptBaseRawText);
        options.onThinking?.(attemptBaseThinkingText);
        await waitForWorkflowRetry(transportRetryCount);
        continue;
      }

      if (!isRecoverableWorkflowOrchestrationError(normalizedError)) {
        throw normalizedError;
      }

      transportRetryCount = 0;
      const shouldResume =
        shouldResumeCurrentAttempt &&
        resumeAttemptCount < MAX_WORKFLOW_RESUME_ATTEMPTS &&
        (attemptProducedProtocolProgress || nextAttemptMode !== 'resume');
      if (shouldResume) {
        resumeAttemptCount += 1;
        lastInterruptedRawText = currentAttemptRawText;
        lastInterruptedError = normalizedError;
        nextAttemptMode = 'resume';
        preserveThinkingOnNextResume = nextState.operations.length > 0;
        preserveRawTextOnNextResume = true;
        if (!preserveRawTextOnNextResume) {
          combinedRawText = '';
        }
        if (!preserveThinkingOnNextResume) {
          combinedThinkingText = '';
        }
        options.onRetry?.(resumeAttemptCount, normalizedError, nextState, 'resume');
        if (!preserveRawTextOnNextResume) {
          lastDeliveredRawText = '';
          options.onRawText?.('');
        }
        if (!preserveThinkingOnNextResume) {
          lastDeliveredThinkingText = '';
          options.onThinking?.('');
        }
        continue;
      }

      const shouldRestart =
        nextState.operations.length === 0 && restartAttemptCount < MAX_WORKFLOW_RESTART_ATTEMPTS;
      if (shouldRestart) {
        restartAttemptCount += 1;
        nextState = createWorkflowOrchestrationState(options.baseDraft);
        lastInterruptedRawText = '';
        lastInterruptedError = normalizedError;
        nextAttemptMode = 'restart';
        combinedThinkingText = '';
        combinedRawText = '';
        lastDeliveredThinkingText = '';
        lastDeliveredRawText = '';
        preserveThinkingOnNextResume = false;
        preserveRawTextOnNextResume = false;
        options.onRetry?.(restartAttemptCount, normalizedError, nextState, 'restart');
        options.onRawText?.('');
        options.onThinking?.('');
        continue;
      }

      throw normalizedError;
    }
  }
}
