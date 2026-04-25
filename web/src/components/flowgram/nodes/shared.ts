import type {
  FlowNodeJSON,
  WorkflowNodeJSON,
  WorkflowNodeRegistry,
} from '@flowgram.ai/free-layout-editor';

export type NazhNodeKind =
  | 'native'
  | 'code'
  | 'timer'
  | 'serialTrigger'
  | 'modbusRead'
  | 'mqttClient'
  | 'if'
  | 'switch'
  | 'tryCatch'
  | 'loop'
  | 'httpClient'
  | 'barkPush'
  | 'sqlWriter'
  | 'debugConsole';
export type NazhNodeDisplayType = NazhNodeKind;

export interface FlowgramLogicBranch {
  key: string;
  label: string;
  fixed?: boolean;
}

export interface FlowgramScriptAiConfig {
  providerId: string;
  model?: string;
  systemPrompt?: string;
  temperature?: number;
  maxTokens?: number;
  topP?: number;
  thinking?: {
    type: 'enabled' | 'disabled';
  };
  reasoningEffort?: 'high' | 'max';
  timeoutMs?: number;
}

export interface NodeSeed {
  idPrefix: string;
  kind: NazhNodeKind;
  displayType?: NazhNodeDisplayType;
  label: string;
  connectionId?: string | null;
  timeoutMs?: number | null;
  config: {
    message?: string;
    script?: string;
    ai?: FlowgramScriptAiConfig;
    branches?: FlowgramLogicBranch[];
    interval_ms?: number;
    immediate?: boolean;
    unit_id?: number;
    register?: number;
    quantity?: number;
    base_value?: number;
    amplitude?: number;
    body_mode?: string;
    body_template?: string;
    title_template?: string;
    content_mode?: string;
    level?: string;
    badge?: string | number;
    sound?: string;
    icon?: string;
    group?: string;
    url?: string;
    copy?: string;
    image?: string;
    auto_copy?: boolean;
    call?: boolean;
    archive_mode?: string;
    database_path?: string;
    table?: string;
    pretty?: boolean;
    label?: string;
    inject?: Record<string, unknown>;
    [key: string]: unknown;
  };
}

export interface FlowgramPaletteItem {
  key: string;
  title: string;
  description: string;
  badge: string;
  seed: NodeSeed;
}

export interface FlowgramPaletteSection {
  key: string;
  title: string;
  items: FlowgramPaletteItem[];
}

export interface FlowgramConnectionDefaults {
  any: string | null;
  modbus: string | null;
  serial: string | null;
  mqtt: string | null;
  http: string | null;
  bark: string | null;
}

export interface NodeCatalogInfo {
  category: string;
  description: string;
}

interface FlowgramNodeData {
  label?: string;
  nodeType?: NazhNodeKind;
  displayType?: NazhNodeDisplayType;
  connectionId?: string | null;
  timeoutMs?: number | null;
  config?: {
    message?: string;
    script?: string;
    ai?: FlowgramScriptAiConfig;
    branches?: FlowgramLogicBranch[];
    body_mode?: string;
    body_template?: string;
    title_template?: string;
    content_mode?: string;
    level?: string;
    badge?: string | number;
    sound?: string;
    icon?: string;
    group?: string;
    url?: string;
    copy?: string;
    image?: string;
    auto_copy?: boolean;
    call?: boolean;
    archive_mode?: string;
    [key: string]: unknown;
  };
}

export const STANDARD_NODE_SIZE = {
  width: 214,
  height: 132,
} as const;

export const LOGIC_NODE_SIZE = {
  width: 240,
  height: 168,
} as const;

export const SWITCH_NODE_SIZE = {
  width: 252,
  height: 188,
} as const;

export const LOOP_NODE_SIZE = {
  width: 244,
  height: 176,
} as const;

export const IF_BRANCHES: FlowgramLogicBranch[] = [
  { key: 'true', label: 'True', fixed: true },
  { key: 'false', label: 'False', fixed: true },
];

export const TRYCATCH_BRANCHES: FlowgramLogicBranch[] = [
  { key: 'try', label: 'Try', fixed: true },
  { key: 'catch', label: 'Catch', fixed: true },
];

export const LOOP_BRANCHES: FlowgramLogicBranch[] = [
  { key: 'body', label: 'Body', fixed: true },
  { key: 'done', label: 'Done', fixed: true },
];

export const DEFAULT_SWITCH_BRANCHES: FlowgramLogicBranch[] = [
  { key: 'default', label: 'Default', fixed: true },
];

export const DEFAULT_HTTP_ALARM_TITLE_TEMPLATE =
  'Nazh 工业告警 · {{payload.tag}} · {{payload.severity}}';
export const DEFAULT_HTTP_ALARM_BODY_TEMPLATE =
  '### Nazh 工业告警\n- 设备：{{payload.tag}}\n- 温度：{{payload.temperature_c}} °C\n- 严重级别：{{payload.severity}}\n- Trace：{{trace_id}}\n- 事件时间：{{timestamp}}';
export const DEFAULT_BARK_TITLE_TEMPLATE = 'Nazh 告警 · {{payload.tag}}';
export const DEFAULT_BARK_BODY_TEMPLATE =
  '设备：{{payload.tag}}\n严重级别：{{payload.severity}}\n时间：{{timestamp}}\n摘要：{{payload}}';

export function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null && !Array.isArray(value);
}

export function normalizeFiniteValue(value: unknown): number | undefined {
  return typeof value === 'number' && Number.isFinite(value) ? value : undefined;
}

export function normalizePositiveIntegerValue(value: unknown): number | undefined {
  if (typeof value !== 'number' || !Number.isFinite(value) || value <= 0) {
    return undefined;
  }

  return Math.round(value);
}

export function normalizeNodeKind(value: unknown): NazhNodeKind {
  switch (value) {
    case 'code':
      return 'code';
    case 'timer':
    case 'serialTrigger':
    case 'modbusRead':
    case 'mqttClient':
    case 'httpClient':
    case 'barkPush':
    case 'sqlWriter':
    case 'debugConsole':
    case 'if':
    case 'switch':
    case 'tryCatch':
    case 'loop':
      return value;
    case 'native':
    default:
      return 'native';
  }
}

function normalizeDisplayType(value: unknown, fallback: NazhNodeKind): NazhNodeDisplayType {
  return normalizeNodeKind(value ?? fallback);
}

export function normalizeTimeoutValue(value: unknown): number | null {
  if (typeof value !== 'number' || !Number.isFinite(value) || value <= 0) {
    return null;
  }

  return value;
}

export function sanitizeBranchKey(input: string): string {
  return input
    .trim()
    .toLowerCase()
    .replace(/[^a-z0-9_-]+/g, '_')
    .replace(/^_+|_+$/g, '');
}

export function uniqueBranchKey(base: string, usedKeys: Set<string>): string {
  const normalizedBase = sanitizeBranchKey(base) || 'branch';
  let candidate = normalizedBase;
  let index = 1;

  while (usedKeys.has(candidate)) {
    candidate = `${normalizedBase}_${index}`;
    index += 1;
  }

  usedKeys.add(candidate);
  return candidate;
}

function normalizeBranchLabel(value: unknown, fallbackKey: string): string {
  if (typeof value === 'string' && value.trim()) {
    return value.trim();
  }

  return fallbackKey;
}

export function normalizeSwitchBranches(value: unknown): FlowgramLogicBranch[] {
  if (!Array.isArray(value)) {
    return [];
  }

  const usedKeys = new Set<string>();

  return value.reduce<FlowgramLogicBranch[]>((acc, item) => {
    if (!isRecord(item)) {
      return acc;
    }

    const sourceKey = typeof item.key === 'string' ? item.key : '';
    const nextKey = uniqueBranchKey(sourceKey || 'branch', usedKeys);
    acc.push({
      key: nextKey,
      label: normalizeBranchLabel(item.label, nextKey),
    });
    return acc;
  }, []);
}

export function normalizeScriptAiConfig(value: unknown): FlowgramScriptAiConfig | undefined {
  if (!isRecord(value)) {
    return undefined;
  }

  const hasKnownField = (
    [
      'providerId',
      'model',
      'systemPrompt',
      'temperature',
      'maxTokens',
      'topP',
      'thinking',
      'reasoningEffort',
      'timeoutMs',
    ] as const
  ).some((key) => hasOwnKey(value, key));

  if (!hasKnownField) {
    return undefined;
  }

  const normalized: FlowgramScriptAiConfig = {
    providerId: typeof value.providerId === 'string' ? value.providerId : '',
  };

  if (typeof value.model === 'string' && value.model.trim()) {
    normalized.model = value.model;
  }

  if (typeof value.systemPrompt === 'string' && value.systemPrompt.trim()) {
    normalized.systemPrompt = value.systemPrompt;
  }

  const temperature = normalizeFiniteValue(value.temperature);
  if (temperature !== undefined) {
    normalized.temperature = temperature;
  }

  const maxTokens = normalizePositiveIntegerValue(value.maxTokens);
  if (maxTokens !== undefined) {
    normalized.maxTokens = maxTokens;
  }

  const topP = normalizeFiniteValue(value.topP);
  if (topP !== undefined) {
    normalized.topP = topP;
  }

  if (
    isRecord(value.thinking) &&
    (value.thinking.type === 'enabled' || value.thinking.type === 'disabled')
  ) {
    normalized.thinking = {
      type: value.thinking.type,
    };
  }

  if (value.reasoningEffort === 'high' || value.reasoningEffort === 'max') {
    normalized.reasoningEffort = value.reasoningEffort;
  }

  const timeoutMs = normalizePositiveIntegerValue(value.timeoutMs);
  if (timeoutMs !== undefined) {
    normalized.timeoutMs = timeoutMs;
  }

  return normalized;
}

export function hasOwnKey<T extends object>(value: T, key: string): boolean {
  return Object.prototype.hasOwnProperty.call(value, key);
}

export function inferHttpWebhookKind(url: string): 'generic' | 'dingtalk' {
  return /dingtalk\.com|dingtalk\.cn|oapi\.dingtalk/i.test(url) ? 'dingtalk' : 'generic';
}

export function normalizeHttpBodyMode(
  value: unknown,
  webhookKind: string,
): 'json' | 'template' | 'dingtalk_markdown' {
  switch (value) {
    case 'template':
      return 'template';
    case 'dingtalk_markdown':
    case 'alarm-template':
      return 'dingtalk_markdown';
    case 'json':
    default:
      return webhookKind === 'dingtalk' ? 'dingtalk_markdown' : 'json';
  }
}

export function resolveDefaultConnectionId(
  nodeType: NazhNodeKind,
  connectionDefaults: FlowgramConnectionDefaults,
): string | null {
  switch (nodeType) {
    case 'native':
      return connectionDefaults.any;
    case 'modbusRead':
      return connectionDefaults.modbus;
    case 'serialTrigger':
      return connectionDefaults.serial;
    case 'mqttClient':
      return connectionDefaults.mqtt;
    case 'httpClient':
      return connectionDefaults.http;
    case 'barkPush':
      return connectionDefaults.bark;
    default:
      return null;
  }
}

export function resolveNodeData(
  seed: NodeSeed,
  fallbackLabel: string,
  connectionDefaults: FlowgramConnectionDefaults,
): Required<FlowgramNodeData> {
  const connectionId =
    seed.connectionId === undefined
      ? resolveDefaultConnectionId(seed.kind, connectionDefaults)
      : seed.connectionId;
  const label = seed.label.trim() || fallbackLabel;

  return {
    label,
    nodeType: seed.kind,
    displayType: normalizeDisplayType(seed.displayType, seed.kind),
    connectionId,
    timeoutMs: seed.timeoutMs ?? null,
    config: normalizeNodeConfig(seed.kind, seed.config),
  };
}

export function buildPaletteNodeJson(
  seed: NodeSeed,
  connectionDefaults: FlowgramConnectionDefaults,
  baseJson?: Partial<WorkflowNodeJSON>,
): Partial<WorkflowNodeJSON> {
  const fallbackLabel = seed.label.trim() || getFallbackNodeLabel(seed.kind);
  const baseData = isRecord(baseJson?.data) ? (baseJson.data as Record<string, unknown>) : {};
  const nextData = resolveNodeData(seed, fallbackLabel, connectionDefaults);

  return {
    ...baseJson,
    type: seed.kind,
    data: {
      ...baseData,
      ...nextData,
      config: {
        ...(isRecord(baseData.config) ? baseData.config : {}),
        ...nextData.config,
      },
    },
  };
}

export function normalizeFlowgramNodeJson(
  json: FlowNodeJSON,
  connectionDefaults: FlowgramConnectionDefaults,
): FlowNodeJSON {
  const rawData = isRecord(json.data) ? (json.data as FlowgramNodeData) : {};
  const nodeType = normalizeNodeKind(rawData.nodeType ?? json.type);
  const fallbackLabel = json.id || getFallbackNodeLabel(nodeType);
  const rawConfig = rawData.config;
  const normalizedConfig = normalizeNodeConfig(nodeType, rawConfig);

  return {
    ...json,
    type: nodeType,
    data: {
      ...rawData,
      label:
        typeof rawData.label === 'string' && rawData.label.trim()
          ? rawData.label
          : fallbackLabel,
      nodeType,
      displayType: normalizeDisplayType(rawData.displayType, nodeType),
      connectionId:
        rawData.connectionId === undefined
          ? resolveDefaultConnectionId(nodeType, connectionDefaults)
          : rawData.connectionId ?? null,
      timeoutMs: normalizeTimeoutValue(rawData.timeoutMs),
      config: normalizedConfig,
    },
  };
}

export function parseTimeoutMs(value: string): number | null {
  const normalized = value.trim();
  if (!normalized) {
    return null;
  }

  const numeric = Number(normalized);
  return normalizeTimeoutValue(numeric);
}

export function getLogicNodeBranchDefinitions(
  nodeType: unknown,
  config: unknown,
): FlowgramLogicBranch[] {
  switch (nodeType) {
    case 'if':
      return IF_BRANCHES;
    case 'tryCatch':
      return TRYCATCH_BRANCHES;
    case 'loop':
      return LOOP_BRANCHES;
    case 'switch': {
      const normalizedConfig = isRecord(config) ? config : {};
      const branches = normalizeSwitchBranches(normalizedConfig.branches);
      return [...branches, ...DEFAULT_SWITCH_BRANCHES];
    }
    default:
      return [];
  }
}

export function getFallbackNodeLabel(kind: NazhNodeKind): string {
  switch (kind) {
    case 'timer':
      return 'Timer Node';
    case 'serialTrigger':
      return 'Serial Trigger';
    case 'modbusRead':
      return 'Modbus Read';
    case 'mqttClient':
      return 'MQTT Client';
    case 'code':
      return 'Code Node';
    case 'if':
      return 'IF Node';
    case 'switch':
      return 'Switch Node';
    case 'tryCatch':
      return 'TryCatch Node';
    case 'loop':
      return 'Loop Node';
    case 'httpClient':
      return 'HTTP Client';
    case 'barkPush':
      return 'Bark Push';
    case 'sqlWriter':
      return 'SQL Writer';
    case 'debugConsole':
      return 'Debug Console';
    case 'native':
    default:
      return 'Native Node';
  }
}

export interface NodeValidation {
  tone: 'info' | 'warning' | 'danger';
  message: string;
}

export interface NodeValidationContext {
  draft: import('./settings-shared').SelectedNodeDraft;
  selectedConnection: import('../../../types').ConnectionDefinition | null;
  compatibleConnections: import('../../../types').ConnectionDefinition[];
  connections: import('../../../types').ConnectionDefinition[];
  resolvedHttpWebhookKind: string;
  resolvedHttpBodyMode: string;
  aiProviders: import('../../../types').AiProviderView[];
  activeAiProviderId: string | null;
  resolvedGlobalAiProvider: import('../../../types').AiProviderView | null;
  preferredCopilotProvider: import('../../../types').AiProviderView | null;
  usesManagedConnection: boolean;
}

export interface NodeDefinition {
  kind: NazhNodeKind;
  catalog: NodeCatalogInfo;
  fallbackLabel: string;
  requiresConnection?: boolean;
  fieldValidators?: Partial<Record<keyof import('./settings-shared').SelectedNodeDraft, import('./settings-shared').FieldValidator>>;
  buildDefaultSeed(): NodeSeed;
  normalizeConfig(config: unknown): NodeSeed['config'];
  getNodeSize(): { width: number; height: number };
  buildRegistryMeta(): {
    defaultExpanded: boolean;
    size: { width: number; height: number };
    defaultPorts?: Array<{ type: 'input' | 'output' }>;
    useDynamicPort?: boolean;
  };
  validate(ctx: NodeValidationContext): NodeValidation[];
}

export function normalizeNodeConfig(
  nodeType: NazhNodeKind,
  config: unknown,
): NodeSeed['config'] {
  const rawConfig = isRecord(config) ? config : {};

  if (nodeType === 'native') {
    return {
      ...rawConfig,
      message: typeof rawConfig.message === 'string' ? rawConfig.message : '',
    };
  }

  if (nodeType === 'timer') {
    return {
      ...rawConfig,
      interval_ms:
        typeof rawConfig.interval_ms === 'number' && Number.isFinite(rawConfig.interval_ms)
          ? Math.max(1, Math.round(rawConfig.interval_ms))
          : 5000,
      immediate: rawConfig.immediate !== false,
      inject: isRecord(rawConfig.inject) ? rawConfig.inject : {},
    };
  }

  if (nodeType === 'serialTrigger') {
    return {
      inject: isRecord(rawConfig.inject) ? rawConfig.inject : {},
    };
  }

  if (nodeType === 'modbusRead') {
    return {
      ...rawConfig,
      unit_id:
        typeof rawConfig.unit_id === 'number' && Number.isFinite(rawConfig.unit_id)
          ? Math.max(1, Math.round(rawConfig.unit_id))
          : 1,
      register:
        typeof rawConfig.register === 'number' && Number.isFinite(rawConfig.register)
          ? Math.max(1, Math.round(rawConfig.register))
          : 40001,
      quantity:
        typeof rawConfig.quantity === 'number' && Number.isFinite(rawConfig.quantity)
          ? Math.max(1, Math.round(rawConfig.quantity))
          : 1,
      register_type:
        typeof rawConfig.register_type === 'string' ? rawConfig.register_type : 'holding',
      base_value:
        typeof rawConfig.base_value === 'number' && Number.isFinite(rawConfig.base_value)
          ? rawConfig.base_value
          : 64,
      amplitude:
        typeof rawConfig.amplitude === 'number' && Number.isFinite(rawConfig.amplitude)
          ? rawConfig.amplitude
          : 6,
    };
  }

  if (nodeType === 'mqttClient') {
    return {
      ...rawConfig,
      mode: rawConfig.mode === 'subscribe' ? 'subscribe' : 'publish',
      topic: typeof rawConfig.topic === 'string' ? rawConfig.topic : '',
      qos: [0, 1, 2].includes(rawConfig.qos as number) ? rawConfig.qos : 0,
      payload_template: typeof rawConfig.payload_template === 'string' ? rawConfig.payload_template : '',
    };
  }

  if (nodeType === 'switch') {
    return {
      ...rawConfig,
      script: typeof rawConfig.script === 'string' ? rawConfig.script : 'payload["route"]',
      branches: normalizeSwitchBranches(rawConfig.branches),
    };
  }

  if (nodeType === 'loop') {
    return {
      ...rawConfig,
      script: typeof rawConfig.script === 'string' ? rawConfig.script : '[payload]',
    };
  }

  if (nodeType === 'httpClient') {
    const legacyUrl = typeof rawConfig.url === 'string' ? rawConfig.url : '';
    const {
      url: _unusedUrl,
      method: _unusedMethod,
      headers: _unusedHeaders,
      webhook_kind: rawWebhookKind,
      content_type: _unusedContentType,
      request_timeout_ms: _unusedRequestTimeoutMs,
      at_mobiles: _unusedAtMobiles,
      at_all: _unusedAtAll,
      ...restConfig
    } = rawConfig;
    const webhookKind =
      typeof rawWebhookKind === 'string' && rawWebhookKind.trim()
        ? rawWebhookKind
        : inferHttpWebhookKind(legacyUrl);
    const bodyMode = normalizeHttpBodyMode(rawConfig.body_mode, webhookKind);

    return {
      ...restConfig,
      body_mode: bodyMode,
      title_template:
        typeof rawConfig.title_template === 'string'
          ? rawConfig.title_template
          : DEFAULT_HTTP_ALARM_TITLE_TEMPLATE,
      body_template:
        typeof rawConfig.body_template === 'string'
          ? rawConfig.body_template
          : bodyMode === 'dingtalk_markdown'
            ? DEFAULT_HTTP_ALARM_BODY_TEMPLATE
            : '',
    };
  }

  if (nodeType === 'barkPush') {
    const {
      server_url: _unusedServerUrl,
      device_key: _unusedDeviceKey,
      request_timeout_ms: _unusedRequestTimeoutMs,
      ...restConfig
    } = rawConfig;

    return {
      ...restConfig,
      content_mode: rawConfig.content_mode === 'markdown' ? 'markdown' : 'body',
      title_template:
        typeof rawConfig.title_template === 'string'
          ? rawConfig.title_template
          : DEFAULT_BARK_TITLE_TEMPLATE,
      subtitle_template:
        typeof rawConfig.subtitle_template === 'string' ? rawConfig.subtitle_template : '',
      body_template:
        typeof rawConfig.body_template === 'string'
          ? rawConfig.body_template
          : DEFAULT_BARK_BODY_TEMPLATE,
      level:
        rawConfig.level === 'critical' ||
        rawConfig.level === 'timeSensitive' ||
        rawConfig.level === 'passive'
          ? rawConfig.level
          : 'active',
      badge:
        typeof rawConfig.badge === 'number'
          ? String(rawConfig.badge)
          : typeof rawConfig.badge === 'string'
            ? rawConfig.badge
            : '',
      sound: typeof rawConfig.sound === 'string' ? rawConfig.sound : '',
      icon: typeof rawConfig.icon === 'string' ? rawConfig.icon : '',
      group: typeof rawConfig.group === 'string' ? rawConfig.group : '',
      url: typeof rawConfig.url === 'string' ? rawConfig.url : '',
      copy: typeof rawConfig.copy === 'string' ? rawConfig.copy : '',
      image: typeof rawConfig.image === 'string' ? rawConfig.image : '',
      auto_copy: rawConfig.auto_copy === true,
      call: rawConfig.call === true,
      archive_mode:
        rawConfig.archive_mode === 'archive' || rawConfig.archive_mode === 'skip'
          ? rawConfig.archive_mode
          : 'inherit',
    };
  }

  if (nodeType === 'sqlWriter') {
    return {
      ...rawConfig,
      database_path:
        typeof rawConfig.database_path === 'string' ? rawConfig.database_path : './nazh-local.sqlite3',
      table: typeof rawConfig.table === 'string' ? rawConfig.table : 'workflow_logs',
    };
  }

  if (nodeType === 'debugConsole') {
    return {
      ...rawConfig,
      label: typeof rawConfig.label === 'string' ? rawConfig.label : '',
      pretty: rawConfig.pretty !== false,
    };
  }

  if (nodeType === 'code') {
    const { ai: _unusedAi, ...restConfig } = rawConfig;
    const ai = normalizeScriptAiConfig(rawConfig.ai);

    return {
      ...restConfig,
      script: typeof rawConfig.script === 'string' ? rawConfig.script : 'payload',
      ...(ai ? { ai } : {}),
    };
  }

  return {
    ...rawConfig,
    script: typeof rawConfig.script === 'string' ? rawConfig.script : 'payload',
  };
}
