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
  | 'if'
  | 'switch'
  | 'tryCatch'
  | 'loop'
  | 'httpClient'
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
    url?: string;
    method?: string;
    headers?: Record<string, unknown>;
    webhook_kind?: string;
    body_mode?: string;
    content_type?: string;
    request_timeout_ms?: number;
    body_template?: string;
    title_template?: string;
    at_mobiles?: string[];
    at_all?: boolean;
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
    webhook_kind?: string;
    body_mode?: string;
    content_type?: string;
    request_timeout_ms?: number;
    body_template?: string;
    title_template?: string;
    at_mobiles?: string[];
    at_all?: boolean;
    [key: string]: unknown;
  };
}

const STANDARD_NODE_SIZE = {
  width: 214,
  height: 132,
} as const;

const LOGIC_NODE_SIZE = {
  width: 240,
  height: 168,
} as const;

const SWITCH_NODE_SIZE = {
  width: 252,
  height: 188,
} as const;

const LOOP_NODE_SIZE = {
  width: 244,
  height: 176,
} as const;

const IF_BRANCHES: FlowgramLogicBranch[] = [
  { key: 'true', label: 'True', fixed: true },
  { key: 'false', label: 'False', fixed: true },
];

const TRYCATCH_BRANCHES: FlowgramLogicBranch[] = [
  { key: 'try', label: 'Try', fixed: true },
  { key: 'catch', label: 'Catch', fixed: true },
];

const LOOP_BRANCHES: FlowgramLogicBranch[] = [
  { key: 'body', label: 'Body', fixed: true },
  { key: 'done', label: 'Done', fixed: true },
];

const DEFAULT_SWITCH_BRANCHES: FlowgramLogicBranch[] = [
  { key: 'default', label: 'Default', fixed: true },
];

const DEFAULT_HTTP_ALARM_TITLE_TEMPLATE =
  'Nazh 工业告警 · {{payload.tag}} · {{payload.severity}}';
const DEFAULT_HTTP_ALARM_BODY_TEMPLATE =
  '### Nazh 工业告警\n- 设备：{{payload.tag}}\n- 温度：{{payload.temperature_c}} °C\n- 严重级别：{{payload.severity}}\n- Trace：{{trace_id}}\n- 事件时间：{{timestamp}}';

const NODE_TEMPLATES: FlowgramPaletteItem[] = [
  {
    key: 'timer-trigger',
    title: '定时触发',
    description: '按固定间隔启动流程。',
    badge: 'Timer',
    seed: {
      idPrefix: 'timer_trigger',
      kind: 'timer',
      displayType: 'timer',
      label: 'Timer Trigger',
      timeoutMs: null,
      config: {
        interval_ms: 5000,
        immediate: true,
        inject: {
          source: 'timer',
        },
      },
    },
  },
  {
    key: 'serial-trigger',
    title: '串口触发',
    description: '监听扫码枪、RFID 等串口外设主动上报。',
    badge: 'Serial',
    seed: {
      idPrefix: 'serial_trigger',
      kind: 'serialTrigger',
      displayType: 'serialTrigger',
      label: 'Serial Trigger',
      timeoutMs: null,
      config: {
        inject: {
          source: 'serial',
        },
      },
    },
  },
  {
    key: 'modbus-temperature',
    title: 'Modbus 采集',
    description: '模拟读取物理寄存器。',
    badge: 'Modbus',
    seed: {
      idPrefix: 'modbus_read',
      kind: 'modbusRead',
      displayType: 'modbusRead',
      label: 'Modbus Read',
      timeoutMs: 1000,
      config: {
        unit_id: 1,
        register: 40001,
        quantity: 1,
        base_value: 64,
        amplitude: 6,
      },
    },
  },
  {
    key: 'switch-router',
    title: 'Switch 分流',
    description: '按 route 字段分支。',
    badge: 'Switch',
    seed: {
      idPrefix: 'switch_router',
      kind: 'switch',
      displayType: 'switch',
      label: 'Switch Router',
      connectionId: null,
      timeoutMs: 1000,
      config: {
        script: 'payload["status"]',
        branches: [
          { key: 'nominal', label: 'Nominal' },
          { key: 'alert', label: 'Alert' },
        ],
      },
    },
  },
  {
    key: 'payload-cleaner',
    title: '数据清洗',
    description: '脚本规范化数据结构。',
    badge: 'Code',
    seed: {
      idPrefix: 'code_clean',
      kind: 'code',
      displayType: 'code',
      label: 'Code Clean',
      timeoutMs: 1000,
      config: {
        script:
          'payload["temperature"] = payload["value"]; payload["severity"] = payload["value"] > 88 ? "high" : "normal"; payload',
      },
    },
  },
  {
    key: 'dingtalk-alarm',
    title: '钉钉报警',
    description: '通过 HTTP 发告警。',
    badge: 'HTTP',
    seed: {
      idPrefix: 'http_alarm',
      kind: 'httpClient',
      displayType: 'httpClient',
      label: 'HTTP Alert',
      timeoutMs: 1000,
      config: {
        method: 'POST',
        url: 'https://oapi.dingtalk.com/robot/send?access_token=demo',
        webhook_kind: 'dingtalk',
        body_mode: 'dingtalk_markdown',
        content_type: 'application/json',
        request_timeout_ms: 4000,
        title_template: DEFAULT_HTTP_ALARM_TITLE_TEMPLATE,
        body_template: DEFAULT_HTTP_ALARM_BODY_TEMPLATE,
        at_mobiles: [],
        at_all: false,
        headers: {
          'X-Alarm-Source': 'nazh',
        },
      },
    },
  },
  {
    key: 'sqlite-audit',
    title: 'SQLite 记录',
    description: '写入本地审计记录。',
    badge: 'SQL',
    seed: {
      idPrefix: 'sql_writer',
      kind: 'sqlWriter',
      displayType: 'sqlWriter',
      label: 'SQL Writer',
      timeoutMs: 1500,
      config: {
        database_path: './data/nazh.sqlite3',
        table: 'workflow_logs',
      },
    },
  },
  {
    key: 'debug-tap',
    title: '调试输出',
    description: '将数据打印到控制台。',
    badge: 'Debug',
    seed: {
      idPrefix: 'debug_console',
      kind: 'debugConsole',
      displayType: 'debugConsole',
      label: 'Debug Console',
      timeoutMs: 500,
      config: {
        label: 'runtime-tap',
        pretty: true,
      },
    },
  },
];

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null && !Array.isArray(value);
}

function hasOwnKey<T extends object>(value: T, key: string): boolean {
  return Object.prototype.hasOwnProperty.call(value, key);
}

function normalizeFiniteValue(value: unknown): number | undefined {
  return typeof value === 'number' && Number.isFinite(value) ? value : undefined;
}

function normalizePositiveIntegerValue(value: unknown): number | undefined {
  if (typeof value !== 'number' || !Number.isFinite(value) || value <= 0) {
    return undefined;
  }

  return Math.round(value);
}

function normalizeScriptAiConfig(value: unknown): FlowgramScriptAiConfig | undefined {
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

  const timeoutMs = normalizePositiveIntegerValue(value.timeoutMs);
  if (timeoutMs !== undefined) {
    normalized.timeoutMs = timeoutMs;
  }

  return normalized;
}

export function normalizeNodeKind(value: unknown): NazhNodeKind {
  switch (value) {
    case 'code':
      return 'code';
    case 'timer':
    case 'serialTrigger':
    case 'modbusRead':
    case 'httpClient':
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

function normalizeTimeoutValue(value: unknown): number | null {
  if (typeof value !== 'number' || !Number.isFinite(value) || value <= 0) {
    return null;
  }

  return value;
}

function sanitizeBranchKey(input: string): string {
  return input
    .trim()
    .toLowerCase()
    .replace(/[^a-z0-9_-]+/g, '_')
    .replace(/^_+|_+$/g, '');
}

function uniqueBranchKey(base: string, usedKeys: Set<string>): string {
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

function normalizeSwitchBranches(value: unknown): FlowgramLogicBranch[] {
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

function normalizeNodeConfig(
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
    const url = typeof rawConfig.url === 'string' ? rawConfig.url : '';
    const webhookKind =
      typeof rawConfig.webhook_kind === 'string' && rawConfig.webhook_kind.trim()
        ? rawConfig.webhook_kind
        : inferHttpWebhookKind(url);
    const bodyMode = normalizeHttpBodyMode(rawConfig.body_mode, webhookKind);

    return {
      ...rawConfig,
      url,
      method: typeof rawConfig.method === 'string' ? rawConfig.method : 'POST',
      headers: isRecord(rawConfig.headers) ? rawConfig.headers : {},
      webhook_kind: webhookKind,
      body_mode: bodyMode,
      content_type:
        typeof rawConfig.content_type === 'string' && rawConfig.content_type.trim()
          ? rawConfig.content_type
          : 'application/json',
      request_timeout_ms:
        typeof rawConfig.request_timeout_ms === 'number' && Number.isFinite(rawConfig.request_timeout_ms)
          ? Math.max(500, Math.round(rawConfig.request_timeout_ms))
          : 4000,
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
      at_mobiles: Array.isArray(rawConfig.at_mobiles)
        ? rawConfig.at_mobiles.filter((value): value is string => typeof value === 'string')
        : [],
      at_all: rawConfig.at_all === true,
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

function getNodeSize(kind: NazhNodeKind) {
  if (kind === 'switch') {
    return SWITCH_NODE_SIZE;
  }

  if (kind === 'loop') {
    return LOOP_NODE_SIZE;
  }

  if (kind === 'if' || kind === 'tryCatch') {
    return LOGIC_NODE_SIZE;
  }

  return STANDARD_NODE_SIZE;
}

function buildRegistryMeta(kind: NazhNodeKind): WorkflowNodeRegistry['meta'] {
  if (kind === 'if' || kind === 'switch' || kind === 'tryCatch' || kind === 'loop') {
    return {
      defaultExpanded: true,
      size: getNodeSize(kind),
      defaultPorts: [{ type: 'input' }],
      useDynamicPort: true,
    };
  }

  return {
    defaultExpanded: true,
    size: getNodeSize(kind),
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

export function getDefaultHttpAlarmTitleTemplate(): string {
  return DEFAULT_HTTP_ALARM_TITLE_TEMPLATE;
}

export function getDefaultHttpAlarmBodyTemplate(): string {
  return DEFAULT_HTTP_ALARM_BODY_TEMPLATE;
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

export function buildDefaultNodeSeed(kind: NazhNodeKind): NodeSeed {
  switch (kind) {
    case 'native':
      return {
        idPrefix: 'native_node',
        kind: 'native',
        displayType: 'native',
        label: '',
        timeoutMs: null,
        config: {
          message: 'New native node',
        },
      };
    case 'code':
      return {
        idPrefix: 'code_node',
        kind: 'code',
        displayType: 'code',
        label: '',
        timeoutMs: 1000,
        config: {
          script: 'payload',
        },
      };
    case 'timer':
      return {
        idPrefix: 'timer_node',
        kind: 'timer',
        displayType: 'timer',
        label: '',
        timeoutMs: null,
        config: {
          interval_ms: 5000,
          immediate: true,
          inject: {},
        },
      };
    case 'serialTrigger':
      return {
        idPrefix: 'serial_trigger',
        kind: 'serialTrigger',
        displayType: 'serialTrigger',
        label: '',
        timeoutMs: null,
        config: {
          inject: {},
        },
      };
    case 'modbusRead':
      return {
        idPrefix: 'modbus_read',
        kind: 'modbusRead',
        displayType: 'modbusRead',
        label: '',
        timeoutMs: 1000,
        config: {
          unit_id: 1,
          register: 40001,
          quantity: 1,
          base_value: 64,
          amplitude: 6,
        },
      };
    case 'if':
      return {
        idPrefix: 'if_node',
        kind: 'if',
        displayType: 'if',
        label: '',
        timeoutMs: 1000,
        config: {
          script: 'payload["value"] > 0',
        },
      };
    case 'switch':
      return {
        idPrefix: 'switch_node',
        kind: 'switch',
        displayType: 'switch',
        label: '',
        timeoutMs: 1000,
        config: {
          script: 'payload["route"]',
          branches: [
            { key: 'route_a', label: 'Route A' },
            { key: 'route_b', label: 'Route B' },
          ],
        },
      };
    case 'tryCatch':
      return {
        idPrefix: 'try_catch_node',
        kind: 'tryCatch',
        displayType: 'tryCatch',
        label: '',
        timeoutMs: 1000,
        config: {
          script: 'payload',
        },
      };
    case 'loop':
      return {
        idPrefix: 'loop_node',
        kind: 'loop',
        displayType: 'loop',
        label: '',
        timeoutMs: 1000,
        config: {
          script: '[payload]',
        },
      };
    case 'httpClient':
      return {
        idPrefix: 'http_client',
        kind: 'httpClient',
        displayType: 'httpClient',
        label: '',
        timeoutMs: 1000,
        config: {
          url: '',
          method: 'POST',
          headers: {},
          webhook_kind: 'generic',
          body_mode: 'json',
          content_type: 'application/json',
          request_timeout_ms: 4000,
          title_template: DEFAULT_HTTP_ALARM_TITLE_TEMPLATE,
          body_template: '',
          at_mobiles: [],
          at_all: false,
        },
      };
    case 'sqlWriter':
      return {
        idPrefix: 'sql_writer',
        kind: 'sqlWriter',
        displayType: 'sqlWriter',
        label: '',
        timeoutMs: 1500,
        config: {
          database_path: './nazh-local.sqlite3',
          table: 'workflow_logs',
        },
      };
    case 'debugConsole':
      return {
        idPrefix: 'debug_console',
        kind: 'debugConsole',
        displayType: 'debugConsole',
        label: '',
        timeoutMs: 500,
        config: {
          label: '',
          pretty: true,
        },
      };
  }
}

function getFallbackNodeLabel(nodeType: NazhNodeKind): string {
  switch (nodeType) {
    case 'timer':
      return 'Timer Node';
    case 'serialTrigger':
      return 'Serial Trigger';
    case 'modbusRead':
      return 'Modbus Read';
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
    case 'sqlWriter':
      return 'SQL Writer';
    case 'debugConsole':
      return 'Debug Console';
    case 'native':
    default:
      return 'Native Node';
  }
}

function normalizedConnectionType(connectionType: string): string {
  return connectionType.trim().toLowerCase();
}

function resolveDefaultConnectionId(
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
      config: normalizeNodeConfig(nodeType, rawData.config),
    },
  };
}

export function createFlowgramNodeRegistries(
  connectionDefaults: FlowgramConnectionDefaults,
): WorkflowNodeRegistry[] {
  const nodeKinds: NazhNodeKind[] = [
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
    'sqlWriter',
    'debugConsole',
  ];

  return nodeKinds.map((kind) => ({
    type: kind,
    meta: buildRegistryMeta(kind),
    onAdd: () => buildPaletteNodeJson(buildDefaultNodeSeed(kind), connectionDefaults),
  }));
}

export function getDefaultFlowgramNodeRegistry(type: string): WorkflowNodeRegistry {
  const kind = normalizeNodeKind(type);

  return {
    type: kind,
    meta: buildRegistryMeta(kind),
  };
}

export function getFlowgramPaletteSections(): FlowgramPaletteSection[] {
  return [
    {
      key: 'blank',
      title: '基础节点',
      items: [
        {
          key: 'blank-timer',
          title: 'Timer',
          description: '定时触发根节点。',
          badge: 'Timer',
          seed: buildDefaultNodeSeed('timer'),
        },
        {
          key: 'blank-serial',
          title: 'Serial Trigger',
          description: '被动接收扫码枪/RFID 串口数据。',
          badge: 'Serial',
          seed: buildDefaultNodeSeed('serialTrigger'),
        },
        {
          key: 'blank-modbus',
          title: 'Modbus Read',
          description: '模拟读物理数据。',
          badge: 'Modbus',
          seed: buildDefaultNodeSeed('modbusRead'),
        },
        {
          key: 'blank-code',
          title: 'Code Node',
          description: 'Rhai 脚本清洗数据。',
          badge: 'Code',
          seed: buildDefaultNodeSeed('code'),
        },
        {
          key: 'blank-http',
          title: 'HTTP Client',
          description: '发送钉钉或 Webhook 报警。',
          badge: 'HTTP',
          seed: buildDefaultNodeSeed('httpClient'),
        },
        {
          key: 'blank-sql',
          title: 'SQL Writer',
          description: '落本地 SQLite 记录。',
          badge: 'SQL',
          seed: buildDefaultNodeSeed('sqlWriter'),
        },
        {
          key: 'blank-debug',
          title: 'Debug Console',
          description: '可视化看当前数据。',
          badge: 'Debug',
          seed: buildDefaultNodeSeed('debugConsole'),
        },
        {
          key: 'blank-native',
          title: 'Native',
          description: '通用原生资源节点。',
          badge: 'Native',
          seed: buildDefaultNodeSeed('native'),
        },
      ],
    },
    {
      key: 'logic',
      title: '逻辑节点',
      items: [
        {
          key: 'blank-if',
          title: 'IF 条件',
          description: '按 true / false 分流。',
          badge: 'IF',
          seed: buildDefaultNodeSeed('if'),
        },
        {
          key: 'blank-switch',
          title: 'Switch 分流',
          description: '按分支键路由。',
          badge: 'Switch',
          seed: buildDefaultNodeSeed('switch'),
        },
        {
          key: 'blank-try-catch',
          title: 'Try 捕获',
          description: '按执行成功或异常分流。',
          badge: 'Try',
          seed: buildDefaultNodeSeed('tryCatch'),
        },
        {
          key: 'blank-loop',
          title: 'Loop 迭代',
          description: '按 body / done 分支迭代。',
          badge: 'Loop',
          seed: buildDefaultNodeSeed('loop'),
        },
      ],
    },
    {
      key: 'templates',
      title: '预设模板',
      items: NODE_TEMPLATES,
    },
  ];
}
