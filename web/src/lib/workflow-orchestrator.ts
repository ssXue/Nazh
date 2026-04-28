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
import type { WorkflowJSON as FlowgramWorkflowJSON } from '@flowgram.ai/free-layout-editor';

import {
  buildDefaultNodeSeed,
  normalizeNodeConfig,
  type NazhNodeKind,
} from '../components/flowgram/flowgram-node-library';
import { toFlowgramWorkflowJson, toNazhWorkflowGraph } from './flowgram';
import { copilotCompleteStream } from './tauri';
import { isUsableGlobalAiProvider, resolveGlobalAiProvider } from './workflow-ai';
import {
  buildWorkflowAiNodeGuideText,
  getLocalWorkflowAiNodeCatalog,
  getWorkflowAiAllowedNodeKinds,
  loadWorkflowAiNodeCatalog,
  normalizeWorkflowAiNodeKind,
  type WorkflowAiNodeCatalog,
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

const PROTOCOL_GUIDE_TEXT = `协议要求：
- 只输出 JSON Lines，每行一个 JSON 对象
- 不要输出 Markdown、代码块、解释文字或序号
- 一旦确定一个节点或一条边，就立即输出，不要等全部设计完再统一输出
- 先输出 project，再输出 node 或 upsert_subgraph，再输出 edge，最后输出 done
- 编辑已有工作流时，只输出必要修改，不要重复未改动的节点
- 节点 id 使用简短的 snake_case 英文
- 不要编造不存在的节点类型
- connectionId 只有在用户明确给出可复用的连接 id 时才填写，否则留空或省略
- 输出的 payloadText 必须是合法 JSON 字符串
- 需要把一段可复用拓扑封装成单节点时，用 upsert_subgraph；不要直接输出 subgraphInput / subgraphOutput，它们由系统自动生成`;

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

export interface SubgraphBlockOperation {
  id: string;
  nodeType: NazhNodeKind;
  label?: string;
  connectionId?: string | null;
  timeoutMs?: number | null;
  config?: JsonValue;
}

export interface SubgraphEdgeOperation {
  from: string;
  to: string;
  sourcePortId?: string;
  targetPortId?: string;
}

export interface UpsertSubgraphOperation {
  type: 'upsert_subgraph';
  id: string;
  label?: string;
  parameterBindings?: Record<string, string | number | boolean>;
  blocks: SubgraphBlockOperation[];
  edges: SubgraphEdgeOperation[];
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
  | UpsertSubgraphOperation
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

function normalizeAllowedNodeKind(value: unknown): NazhNodeKind | null {
  return normalizeWorkflowAiNodeKind(value, getLocalWorkflowAiNodeCatalog());
}

function normalizeSubgraphBlockNodeKind(value: unknown): NazhNodeKind | null {
  const kind = normalizeAllowedNodeKind(value);
  if (kind === 'subgraph') {
    return null;
  }
  return kind;
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

function normalizeParameterBindings(
  value: unknown,
): Record<string, string | number | boolean> | undefined {
  if (!isRecord(value)) {
    return undefined;
  }

  const bindings = Object.entries(value).reduce<Record<string, string | number | boolean>>(
    (acc, [key, rawValue]) => {
      if (
        typeof rawValue === 'string' ||
        typeof rawValue === 'number' ||
        typeof rawValue === 'boolean'
      ) {
        acc[key] = rawValue;
      }
      return acc;
    },
    {},
  );

  return Object.keys(bindings).length > 0 ? bindings : undefined;
}

function normalizeSubgraphBlocks(value: unknown): SubgraphBlockOperation[] {
  if (!Array.isArray(value)) {
    return [];
  }

  return value
    .map((item): SubgraphBlockOperation | null => {
      if (!isRecord(item)) {
        return null;
      }

      const id = readString(item, ['id', 'nodeId', 'node_id']);
      const nodeType = normalizeSubgraphBlockNodeKind(
        readString(item, ['nodeType', 'node_type', 'kind']),
      );
      if (!id || !nodeType) {
        return null;
      }

      const hasConnectionId =
        hasOwnKey(item, 'connectionId') || hasOwnKey(item, 'connection_id');
      const connectionId = hasConnectionId
        ? readString(item, ['connectionId', 'connection_id']) ?? null
        : undefined;
      const config = isRecord(item.config) || Array.isArray(item.config)
        ? (item.config as JsonValue)
        : undefined;

      return {
        id,
        nodeType,
        label: readString(item, ['label', 'title']),
        connectionId,
        timeoutMs: toPositiveRoundedNumber(
          readFiniteNumber(item, ['timeoutMs', 'timeout_ms']),
        ),
        config,
      };
    })
    .filter((block): block is SubgraphBlockOperation => Boolean(block));
}

function normalizeSubgraphEdges(value: unknown): SubgraphEdgeOperation[] {
  if (!Array.isArray(value)) {
    return [];
  }

  return value
    .map((item): SubgraphEdgeOperation | null => {
      if (!isRecord(item)) {
        return null;
      }

      const from = readString(item, ['from', 'source', 'sourceNodeId', 'source_node_id']);
      const to = readString(item, ['to', 'target', 'targetNodeId', 'target_node_id']);
      if (!from || !to) {
        return null;
      }

      return {
        from,
        to,
        sourcePortId: readString(item, ['sourcePortId', 'source_port_id', 'sourcePort']),
        targetPortId: readString(item, ['targetPortId', 'target_port_id', 'targetPort']),
      };
    })
    .filter((edge): edge is SubgraphEdgeOperation => Boolean(edge));
}

function buildEdgeKey(edge: Pick<WorkflowEdge, 'from' | 'to' | 'source_port_id' | 'target_port_id'>): string {
  return `${edge.from}:${edge.source_port_id ?? ''}->${edge.to}:${edge.target_port_id ?? ''}`;
}

function buildFlowgramEdgeKey(
  edge: Pick<
    FlowgramWorkflowJSON['edges'][number],
    'sourceNodeID' | 'targetNodeID' | 'sourcePortID' | 'targetPortID'
  >,
): string {
  return `${edge.sourceNodeID}:${edge.sourcePortID ?? ''}->${edge.targetNodeID}:${edge.targetPortID ?? ''}`;
}

function buildGraphWithLabels(
  graph: WorkflowGraph,
  nodeLabels: Record<string, string>,
): WorkflowGraph {
  const editorGraph = toFlowgramWorkflowJson(graph);

  const nextEditorGraph = {
    ...editorGraph,
    nodes: editorGraph.nodes.map((node) => ({
      ...node,
      data: {
        ...(isRecord(node.data) ? node.data : {}),
        label:
          nodeLabels[node.id] ??
          (isRecord(node.data) && typeof node.data.label === 'string'
            ? node.data.label
            : node.id),
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
  editorGraph?: WorkflowGraph['editor_graph'],
): WorkflowOrchestrationDraft {
  const baseGraph: WorkflowGraph = {
    name,
    connections: [],
    nodes,
    edges,
    editor_graph: editorGraph,
  };

  return {
    name,
    description,
    payloadText,
    graph: buildGraphWithLabels(baseGraph, nodeLabels),
  };
}

function buildFlowgramNodeData(
  nodeType: NazhNodeKind,
  label: string,
  config: JsonValue | undefined,
  connectionId?: string | null,
  timeoutMs?: number | null,
) {
  const defaultSeed = buildDefaultNodeSeed(nodeType);
  const mergedConfig = mergeJsonValue(
    defaultSeed.config as JsonValue,
    (config ?? {}) as JsonValue,
  );

  return {
    label,
    nodeType,
    displayType: nodeType,
    connectionId: connectionId ?? defaultSeed.connectionId ?? null,
    timeoutMs: timeoutMs ?? defaultSeed.timeoutMs ?? null,
    config: normalizeNodeConfig(nodeType, mergedConfig),
  };
}

function buildSubgraphFlowgramNode(
  operation: UpsertSubgraphOperation,
  currentLabels: Record<string, string>,
): FlowgramWorkflowJSON['nodes'][number] {
  const blocks: FlowgramWorkflowJSON['nodes'] = [
    {
      id: 'sg-in',
      type: 'subgraphInput',
      meta: { position: { x: 0, y: 0 } },
      data: buildFlowgramNodeData('subgraphInput', 'Input', {}),
    },
    ...operation.blocks.map((block, index) => ({
      id: block.id,
      type: block.nodeType,
      meta: { position: { x: 160 + index * 220, y: 96 } },
      data: buildFlowgramNodeData(
        block.nodeType,
        resolveNodeLabel(block.id, block.label, currentLabels),
        block.config,
        block.connectionId,
        block.timeoutMs,
      ),
    })),
    {
      id: 'sg-out',
      type: 'subgraphOutput',
      meta: { position: { x: 160 + operation.blocks.length * 220, y: 0 } },
      data: buildFlowgramNodeData('subgraphOutput', 'Output', {}),
    },
  ];

  const innerEdges =
    operation.edges.length > 0
      ? operation.edges
      : operation.blocks.reduce<SubgraphEdgeOperation[]>((acc, block, index, allBlocks) => {
          if (index === 0) {
            acc.push({ from: 'sg-in', to: block.id });
          }
          const nextBlock = allBlocks[index + 1];
          acc.push({ from: block.id, to: nextBlock?.id ?? 'sg-out' });
          return acc;
        }, []);

  return {
    id: operation.id,
    type: 'subgraph',
    meta: { position: { x: 80, y: 80 } },
    data: {
      label: resolveNodeLabel(operation.id, operation.label, currentLabels),
      nodeType: 'subgraph',
      displayType: 'subgraph',
      connectionId: null,
      timeoutMs: null,
      config: {
        parameterBindings: operation.parameterBindings ?? {},
      },
    },
    blocks,
    edges: innerEdges.map((edge) => ({
      sourceNodeID: edge.from,
      targetNodeID: edge.to,
      sourcePortID: normalizeEdgePortId(edge.sourcePortId),
      targetPortID: normalizeEdgePortId(edge.targetPortId),
    })),
  };
}

function removeEditorNode(
  editorGraph: FlowgramWorkflowJSON,
  nodeId: string,
): FlowgramWorkflowJSON {
  return {
    nodes: editorGraph.nodes.filter((node) => node.id !== nodeId),
    edges: editorGraph.edges.filter(
      (edge) => edge.sourceNodeID !== nodeId && edge.targetNodeID !== nodeId,
    ),
  };
}

function rebuildGraphFromEditorGraph(
  name: string,
  nodes: Record<string, WorkflowNodeDefinition>,
  edges: WorkflowEdge[],
  editorGraph: FlowgramWorkflowJSON,
): WorkflowGraph {
  return toNazhWorkflowGraph(editorGraph, {
    name,
    connections: [],
    nodes,
    edges,
  });
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
  const editorGraph = toFlowgramWorkflowJson(draft.graph);
  const nodeLabels = editorGraph.nodes.reduce<Record<string, string>>((acc, node) => {
    const data = isRecord(node.data) ? node.data : {};
    acc[node.id] = typeof data.label === 'string' && data.label.trim()
      ? data.label.trim()
      : node.id;
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
      editorGraph,
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
    case 'upsert_subgraph':
      return `编排子图 ${operation.id} · ${operation.blocks.length} 个内部节点`;
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
  nodeCatalog?: WorkflowAiNodeCatalog;
}): AiMessage[] {
  const { mode, requirement, baseDraft } = options;
  const nodeCatalog = options.nodeCatalog ?? getLocalWorkflowAiNodeCatalog();
  const allowedNodeKinds = getWorkflowAiAllowedNodeKinds(nodeCatalog);
  const nodeGuideText = buildWorkflowAiNodeGuideText(nodeCatalog);
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

可用节点类型与能力（由当前节点定义 / 运行时注册表自动生成）：
${nodeGuideText}

${PROTOCOL_GUIDE_TEXT}

操作格式：
{"type":"project","name":"工程名","description":"说明","payloadText":"{\\"manual\\":true}"}
{"type":"upsert_node","id":"timer_trigger","nodeType":"timer","label":"定时触发","timeoutMs":null,"config":{"interval_ms":5000,"immediate":true,"inject":{"source":"timer"}}}
{"type":"upsert_subgraph","id":"cleaning_group","label":"数据清洗子图","parameterBindings":{"threshold":88},"blocks":[{"id":"clean_code","nodeType":"code","label":"清洗脚本","config":{"script":"payload"}}],"edges":[{"from":"sg-in","to":"clean_code"},{"from":"clean_code","to":"sg-out"}]}
{"type":"upsert_edge","from":"timer_trigger","to":"debug_console"}
{"type":"delete_node","id":"old_node"}
{"type":"delete_edge","from":"old_a","to":"old_b"}
{"type":"done","summary":"完成摘要"}

注意：
- nodeType 只能从 ${allowedNodeKinds.join(', ')} 中选择
- switch / if / tryCatch / loop 的 sourcePortId 要合法
- code 节点脚本只输出 Rhai 可执行逻辑，不要使用未声明 API
- subgraph 是编辑器容器；需要封装拓扑时用 upsert_subgraph，外部 edge 可连接到 subgraph id，部署前系统会自动展平为 subgraphInput/subgraphOutput 桥接链路
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
  nodeCatalog?: WorkflowAiNodeCatalog;
}): AiMessage[] {
  const baseMessages = buildWorkflowOrchestrationPrompt({
    mode: options.mode,
    requirement: options.requirement,
    baseDraft: options.acceptedState.draft,
    nodeCatalog: options.nodeCatalog,
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
    case 'subgraph':
    case 'upsert_subgraph': {
      const id = readString(input, ['id', 'nodeId', 'node_id']);
      if (!id) {
        return null;
      }

      return {
        type: 'upsert_subgraph',
        id,
        label: readString(input, ['label', 'title']),
        parameterBindings: normalizeParameterBindings(
          input.parameterBindings ?? input.parameter_bindings,
        ),
        blocks: normalizeSubgraphBlocks(input.blocks),
        edges: normalizeSubgraphEdges(input.edges),
      };
    }
    case 'node':
    case 'upsert_node': {
      const id = readString(input, ['id', 'nodeId', 'node_id']);
      const nodeType = normalizeAllowedNodeKind(
        readString(input, ['nodeType', 'node_type', 'kind']),
      );
      if (!id || !nodeType) {
        return null;
      }

      if (nodeType === 'subgraph') {
        return {
          type: 'upsert_subgraph',
          id,
          label: readString(input, ['label', 'title']),
          parameterBindings: normalizeParameterBindings(
            input.parameterBindings ??
              input.parameter_bindings ??
              (isRecord(input.config) ? input.config.parameterBindings : undefined),
          ),
          blocks: normalizeSubgraphBlocks(input.blocks),
          edges: normalizeSubgraphEdges(input.edges),
        };
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
  let nextEditorGraph = state.draft.graph.editor_graph;
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
    case 'upsert_subgraph': {
      nextNodeLabels[operation.id] = resolveNodeLabel(
        operation.id,
        operation.label,
        nextNodeLabels,
      );
      const currentGraph: WorkflowGraph = {
        name: nextName,
        connections: [],
        nodes: nextNodes,
        edges: nextEdges,
        editor_graph: nextEditorGraph,
      };
      const editorGraph = removeEditorNode(
        toFlowgramWorkflowJson(currentGraph),
        operation.id,
      );
      const nextSubgraph = buildSubgraphFlowgramNode(operation, nextNodeLabels);
      const rebuiltGraph = rebuildGraphFromEditorGraph(
        nextName,
        nextNodes,
        nextEdges,
        {
          nodes: [...editorGraph.nodes, nextSubgraph],
          edges: editorGraph.edges,
        },
      );
      nextNodes = rebuiltGraph.nodes;
      nextEdges = rebuiltGraph.edges ?? [];
      nextEditorGraph = rebuiltGraph.editor_graph;
      break;
    }
    case 'delete_node':
      delete nextNodes[operation.id];
      delete nextNodeLabels[operation.id];
      nextEdges = nextEdges.filter(
        (edge) => edge.from !== operation.id && edge.to !== operation.id,
      );
      if (nextEditorGraph) {
        const rebuiltGraph = rebuildGraphFromEditorGraph(
          nextName,
          nextNodes,
          nextEdges,
          removeEditorNode(
            toFlowgramWorkflowJson({
              name: nextName,
              connections: [],
              nodes: nextNodes,
              edges: nextEdges,
              editor_graph: nextEditorGraph,
            }),
            operation.id,
          ),
        );
        nextNodes = rebuiltGraph.nodes;
        nextEdges = rebuiltGraph.edges ?? [];
        nextEditorGraph = rebuiltGraph.editor_graph;
      }
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
      if (nextEditorGraph) {
        const currentGraph: WorkflowGraph = {
          name: nextName,
          connections: [],
          nodes: nextNodes,
          edges: nextEdges,
          editor_graph: nextEditorGraph,
        };
        const editorGraph = toFlowgramWorkflowJson(currentGraph);
        const nextFlowEdge = {
          sourceNodeID: nextEdge.from,
          targetNodeID: nextEdge.to,
          sourcePortID: nextEdge.source_port_id,
          targetPortID: nextEdge.target_port_id,
        };
        const edgeKey = buildFlowgramEdgeKey(nextFlowEdge);
        const rebuiltGraph = rebuildGraphFromEditorGraph(
          nextName,
          nextNodes,
          nextEdges,
          {
            nodes: editorGraph.nodes,
            edges: [
              ...editorGraph.edges.filter(
                (edge) => buildFlowgramEdgeKey(edge) !== edgeKey,
              ),
              nextFlowEdge,
            ],
          },
        );
        nextNodes = rebuiltGraph.nodes;
        nextEdges = rebuiltGraph.edges ?? [];
        nextEditorGraph = rebuiltGraph.editor_graph;
      }
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
      if (nextEditorGraph) {
        const currentGraph: WorkflowGraph = {
          name: nextName,
          connections: [],
          nodes: nextNodes,
          edges: nextEdges,
          editor_graph: nextEditorGraph,
        };
        const editorGraph = toFlowgramWorkflowJson(currentGraph);
        const flowEdgeKey = buildFlowgramEdgeKey({
          sourceNodeID: operation.from,
          targetNodeID: operation.to,
          sourcePortID: operation.sourcePortId,
          targetPortID: operation.targetPortId,
        });
        const rebuiltGraph = rebuildGraphFromEditorGraph(
          nextName,
          nextNodes,
          nextEdges,
          {
            nodes: editorGraph.nodes,
            edges: editorGraph.edges.filter(
              (edge) => buildFlowgramEdgeKey(edge) !== flowEdgeKey,
            ),
          },
        );
        nextNodes = rebuiltGraph.nodes;
        nextEdges = rebuiltGraph.edges ?? [];
        nextEditorGraph = rebuiltGraph.editor_graph;
      }
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
      nextEditorGraph,
    ),
    nodeLabels: nextNodeLabels,
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
  const nodeCatalog = await loadWorkflowAiNodeCatalog();
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
            nodeCatalog,
          })
        : buildWorkflowOrchestrationPrompt({
            mode: options.mode,
            requirement: options.requirement,
            baseDraft: options.baseDraft,
            nodeCatalog,
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
