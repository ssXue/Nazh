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
  normalizeNodeConfig,
  type NazhNodeKind,
} from '../components/flowgram/flowgram-node-library';
import { toFlowgramWorkflowJson } from './flowgram';
import { copilotCompleteStream } from './tauri';
import { isUsableGlobalAiProvider, resolveGlobalAiProvider } from './workflow-ai';

const DEFAULT_WORKFLOW_TIMEOUT_MS = 90_000;
const DEFAULT_COPILOT_PARAMS: AiGenerationParams = {
  temperature: 0.45,
  maxTokens: 4096,
  topP: 1,
};

const ALLOWED_NODE_KINDS: NazhNodeKind[] = [
  'native',
  'code',
  'timer',
  'serialTrigger',
  'modbusRead',
  'if',
  'switch',
  'tryCatch',
  'loop',
  'httpClient',
  'barkPush',
  'sqlWriter',
  'debugConsole',
];

const NODE_GUIDE_TEXT = `可用节点类型与建议：
- timer: 定时触发。config 可含 interval_ms, immediate, inject
- native: 本地透传/注入。config 可含 message
- code: Rhai 脚本处理。config 必须含 script，脚本输入变量是 payload，常用能力有 ai_complete("prompt"), rand(min, max), now_ms(), from_json(text), to_json(value), is_blank(text)
- if: 条件分支。config 必须含 script；下游边 sourcePortId 只能是 true / false
- switch: 多路分支。config 必须含 script 与 branches，branches 形如 [{key, label}]；下游边 sourcePortId 必须对应 branch key 或 default
- tryCatch: 异常分支。config 必须含 script；下游边 sourcePortId 只能是 try / catch
- loop: 循环分发。config 必须含 script；下游边 sourcePortId 只能是 body / done
- serialTrigger: 串口触发。通常不填写 connectionId，等待用户后续绑定
- modbusRead: Modbus 读取。config 可含 unit_id, register, quantity, register_type, base_value, amplitude；通常不填写 connectionId
- httpClient: HTTP/Webhook 发送。config 可含 body_mode, title_template, body_template；通常不填写 connectionId
- barkPush: Bark 推送。config 可含 title_template, subtitle_template, body_template, level；通常不填写 connectionId
- sqlWriter: SQLite 写入。config 可含 database_path, table
- debugConsole: 调试输出。config 可含 label, pretty

协议要求：
- 只输出 JSON Lines，每行一个 JSON 对象
- 不要输出 Markdown、代码块、解释文字或序号
- 一旦确定一个节点或一条边，就立即输出，不要等全部设计完再统一输出
- 先输出 project，再输出 node，再输出 edge，最后输出 done
- 编辑已有工作流时，只输出必要修改，不要重复未改动的节点
- 节点 id 使用简短的 snake_case 英文
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

export interface UpsertNodeOperation {
  type: 'upsert_node';
  id: string;
  nodeType: NazhNodeKind;
  label?: string;
  connectionId?: string | null;
  timeoutMs?: number | null;
  config?: JsonValue;
}

export interface DeleteNodeOperation {
  type: 'delete_node';
  id: string;
}

export interface UpsertEdgeOperation {
  type: 'upsert_edge';
  from: string;
  to: string;
  sourcePortId?: string;
  targetPortId?: string;
}

export interface DeleteEdgeOperation {
  type: 'delete_edge';
  from: string;
  to: string;
  sourcePortId?: string;
  targetPortId?: string;
}

export interface DoneOperation {
  type: 'done';
  summary?: string;
}

export type WorkflowOrchestrationOperation =
  | ProjectMetadataOperation
  | UpsertNodeOperation
  | DeleteNodeOperation
  | UpsertEdgeOperation
  | DeleteEdgeOperation
  | DoneOperation;

export interface WorkflowOrchestrationSessionState {
  draft: WorkflowOrchestrationDraft;
  nodeLabels: Record<string, string>;
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
  ) => void;
}

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

function normalizeAllowedNodeKind(value: unknown): NazhNodeKind | null {
  switch (value) {
    case 'native':
    case 'code':
    case 'timer':
    case 'serialTrigger':
    case 'modbusRead':
    case 'if':
    case 'switch':
    case 'tryCatch':
    case 'loop':
    case 'httpClient':
    case 'barkPush':
    case 'sqlWriter':
    case 'debugConsole':
      return value;
    default:
      return null;
  }
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
    nodes: editorGraph.nodes.map((node) => ({
      ...node,
      data: {
        ...(isRecord(node.data) ? node.data : {}),
        label: nodeLabels[node.id] ?? node.id,
      },
    })),
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

export function createWorkflowOrchestrationState(
  baseDraft?: WorkflowOrchestrationDraft | null,
): WorkflowOrchestrationSessionState {
  const draft = baseDraft ?? createEmptyWorkflowDraft();
  const nodeLabels = Object.keys(draft.graph.nodes).reduce<Record<string, string>>((acc, nodeId) => {
    acc[nodeId] = nodeId;
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
    case 'upsert_node':
      return `编排节点 ${operation.id} · ${operation.nodeType}`;
    case 'delete_node':
      return `删除节点 ${operation.id}`;
    case 'upsert_edge':
      return `连接 ${operation.from} -> ${operation.to}`;
    case 'delete_edge':
      return `移除连线 ${operation.from} -> ${operation.to}`;
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

function buildExistingGraphSummary(draft: WorkflowOrchestrationDraft): string {
  return JSON.stringify(
    {
      project: {
        name: draft.name,
        description: draft.description,
        payloadText: normalizePayloadText(draft.payloadText),
      },
      graph: {
        name: draft.graph.name,
        nodes: Object.fromEntries(
          Object.entries(draft.graph.nodes).map(([nodeId, node]) => [
            nodeId,
            {
              type: node.type,
              connection_id: node.connection_id ?? null,
              timeout_ms: node.timeout_ms ?? null,
              config: node.config ?? {},
            },
          ]),
        ),
        edges: (draft.graph.edges ?? []).map((edge) => ({
          from: edge.from,
          to: edge.to,
          source_port_id: edge.source_port_id ?? null,
          target_port_id: edge.target_port_id ?? null,
        })),
      },
    },
    null,
    2,
  );
}

export function buildWorkflowOrchestrationPrompt(options: {
  mode: WorkflowOrchestrationMode;
  requirement: string;
  baseDraft?: WorkflowOrchestrationDraft | null;
}): AiMessage[] {
  const { mode, requirement, baseDraft } = options;
  const currentGraphText =
    mode === 'edit' && baseDraft
      ? buildExistingGraphSummary(baseDraft)
      : '当前从空白工作流开始。';

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

${NODE_GUIDE_TEXT}

操作格式：
{"type":"project","name":"工程名","description":"说明","payloadText":"{\\"manual\\":true}"}
{"type":"upsert_node","id":"timer_trigger","nodeType":"timer","label":"定时触发","timeoutMs":null,"config":{"interval_ms":5000,"immediate":true,"inject":{"source":"timer"}}}
{"type":"upsert_edge","from":"timer_trigger","to":"debug_console"}
{"type":"delete_node","id":"old_node"}
{"type":"delete_edge","from":"old_a","to":"old_b"}
{"type":"done","summary":"完成摘要"}

注意：
- nodeType 只能从 ${ALLOWED_NODE_KINDS.join(', ')} 中选择
- switch / if / tryCatch / loop 的 sourcePortId 要合法
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
    case 'node':
    case 'upsert_node': {
      const id = readString(input, ['id', 'nodeId', 'node_id']);
      const nodeType = normalizeAllowedNodeKind(
        readString(input, ['nodeType', 'node_type', 'kind']),
      );
      if (!id || !nodeType) {
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
        type: 'upsert_node',
        id,
        nodeType,
        label: readString(input, ['label', 'title']),
        connectionId: nextConnectionId,
        timeoutMs: toPositiveRoundedNumber(
          readFiniteNumber(input, ['timeoutMs', 'timeout_ms']),
        ),
        config,
      };
    }
    case 'remove_node':
    case 'delete_node': {
      const id = readString(input, ['id', 'nodeId', 'node_id']);
      if (!id) {
        return null;
      }
      return {
        type: 'delete_node',
        id,
      };
    }
    case 'edge':
    case 'upsert_edge': {
      const from = readString(input, ['from', 'source', 'sourceNodeId', 'source_node_id']);
      const to = readString(input, ['to', 'target', 'targetNodeId', 'target_node_id']);
      if (!from || !to) {
        return null;
      }

      return {
        type: 'upsert_edge',
        from,
        to,
        sourcePortId: readString(input, ['sourcePortId', 'source_port_id', 'sourcePort']),
        targetPortId: readString(input, ['targetPortId', 'target_port_id', 'targetPort']),
      };
    }
    case 'remove_edge':
    case 'delete_edge': {
      const from = readString(input, ['from', 'source', 'sourceNodeId', 'source_node_id']);
      const to = readString(input, ['to', 'target', 'targetNodeId', 'target_node_id']);
      if (!from || !to) {
        return null;
      }

      return {
        type: 'delete_edge',
        from,
        to,
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
  nextLabel: string | undefined,
  currentLabels: Record<string, string>,
): string {
  if (nextLabel && nextLabel.trim()) {
    return nextLabel.trim();
  }

  return currentLabels[nodeId] ?? nodeId;
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
    case 'upsert_node': {
      const defaultSeed = buildDefaultNodeSeed(operation.nodeType);
      const existingNode = nextNodes[operation.id];
      const mergedConfig = mergeJsonValue(
        (existingNode?.config ?? defaultSeed.config) as JsonValue,
        (operation.config ?? {}) as JsonValue,
      );
      const normalizedConfig = normalizeNodeConfig(operation.nodeType, mergedConfig);

      nextNodeLabels[operation.id] = resolveNodeLabel(
        operation.id,
        operation.label,
        nextNodeLabels,
      );

      nextNodes[operation.id] = {
        id: operation.id,
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
    case 'delete_node':
      delete nextNodes[operation.id];
      delete nextNodeLabels[operation.id];
      nextEdges = nextEdges.filter((edge) => edge.from !== operation.id && edge.to !== operation.id);
      break;
    case 'upsert_edge': {
      const nextEdge: WorkflowEdge = {
        from: operation.from,
        to: operation.to,
        source_port_id: normalizeEdgePortId(operation.sourcePortId),
        target_port_id: normalizeEdgePortId(operation.targetPortId),
      };
      const nextEdgeKey = buildEdgeKey(nextEdge);
      nextEdges = nextEdges.filter((edge) => buildEdgeKey(edge) !== nextEdgeKey);
      nextEdges.push(nextEdge);
      break;
    }
    case 'delete_edge': {
      const nextEdgeKey = buildEdgeKey({
        from: operation.from,
        to: operation.to,
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
    operations: [...state.operations, operation],
    summary: nextSummary,
  };
}

function resolveGenerationParams(params?: AiGenerationParams): AiGenerationParams {
  return {
    temperature: params?.temperature ?? DEFAULT_COPILOT_PARAMS.temperature,
    maxTokens: params?.maxTokens ?? DEFAULT_COPILOT_PARAMS.maxTokens,
    topP: params?.topP ?? DEFAULT_COPILOT_PARAMS.topP,
  };
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

export async function streamWorkflowOrchestration(
  options: StreamWorkflowOrchestrationOptions,
): Promise<WorkflowOrchestrationSessionState> {
  const messages = buildWorkflowOrchestrationPrompt({
    mode: options.mode,
    requirement: options.requirement,
    baseDraft: options.baseDraft,
  });
  const request: AiCompletionRequest = {
    providerId: options.providerId,
    model: options.model ?? undefined,
    messages,
    params: resolveGenerationParams(options.params),
    timeoutMs: options.timeoutMs ?? DEFAULT_WORKFLOW_TIMEOUT_MS,
  };

  let nextState = createWorkflowOrchestrationState(options.baseDraft);
  let processedLength = 0;

  const streamResult = await copilotCompleteStream(
    request,
    (accumulatedText) => {
      options.onRawText?.(accumulatedText);
      const parsed = consumeOperationLines(accumulatedText, processedLength);
      processedLength = parsed.nextProcessedLength;

      for (const operation of parsed.operations) {
        nextState = applyWorkflowOrchestrationOperation(nextState, operation);
        options.onOperation?.(operation, nextState);
      }
    },
    options.onThinking,
    {
      maxRetries: 1,
      onRetryStart: async (attempt, error) => {
        processedLength = 0;
        nextState = createWorkflowOrchestrationState(options.baseDraft);
        options.onRetry?.(attempt, error, nextState);
      },
    },
  );
  const rawText = streamResult.text;

  const trailingOperation = extractTrailingOperation(rawText);
  const lastAppliedOperation = nextState.operations[nextState.operations.length - 1] ?? null;
  const shouldApplyTrailingOperation =
    trailingOperation !== null &&
    (!lastAppliedOperation ||
      JSON.stringify(lastAppliedOperation) !== JSON.stringify(trailingOperation));

  if (shouldApplyTrailingOperation) {
    nextState = applyWorkflowOrchestrationOperation(nextState, trailingOperation);
    options.onOperation?.(trailingOperation, nextState);
  }

  const hasCompletionOperation = nextState.operations.some((operation) => operation.type === 'done');
  if (!hasCompletionOperation) {
    throw buildIncompleteProtocolError(rawText, nextState, streamResult.finishReason);
  }

  return nextState;
}
