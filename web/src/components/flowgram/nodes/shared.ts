export type NazhNodeDisplayType = string;

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

export interface NodeSeed<K extends string = string> {
  idPrefix: string;
  kind: K;
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
  can: string | null;
  ethercat: string | null;
}

export interface NodeCatalogInfo {
  category: string;
  description: string;
}

export interface NodePaletteMetadata {
  visible?: boolean;
  title?: string;
  badge?: string;
}

export interface NodeAiMetadata {
  visible?: boolean;
  editorOnly?: boolean;
  hint?: string;
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

export function parseTimeoutMs(value: string): number | null {
  const normalized = value.trim();
  if (!normalized) {
    return null;
  }

  const numeric = Number(normalized);
  return normalizeTimeoutValue(numeric);
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

export interface NodeDefinition<K extends string = string> {
  kind: K;
  catalog: NodeCatalogInfo;
  fallbackLabel: string;
  palette?: NodePaletteMetadata;
  ai?: NodeAiMetadata;
  requiresConnection?: boolean;
  fieldValidators?: Partial<Record<keyof import('./settings-shared').SelectedNodeDraft, import('./settings-shared').FieldValidator>>;
  buildDefaultSeed(): NodeSeed<K>;
  normalizeConfig(config: unknown): NodeSeed['config'];
  getOutputPorts?(config: unknown): FlowgramLogicBranch[];
  getRoutingBranches?(config: unknown): FlowgramLogicBranch[];
  getNodeSize(): { width: number; height: number };
  buildRegistryMeta(): {
    defaultExpanded: boolean;
    isContainer?: boolean;
    size: { width: number; height: number };
    defaultPorts?: Array<{ type: 'input' | 'output' }>;
    useDynamicPort?: boolean;
    deleteDisable?: boolean;
    copyDisable?: boolean;
    padding?: (transform: unknown) => { top: number; bottom: number; left: number; right: number };
    selectable?: (node: unknown, mousePos?: unknown) => boolean;
    wrapperStyle?: Record<string, string>;
  };
  validate(ctx: NodeValidationContext): NodeValidation[];
}
