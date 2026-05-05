import type {
  FlowNodeJSON,
  WorkflowNodeJSON,
  WorkflowNodeRegistry,
} from '@flowgram.ai/free-layout-editor';

export type {
  FlowgramLogicBranch,
  FlowgramScriptAiConfig,
  NodeSeed,
  NodeCatalogInfo,
  NodeDefinition,
  FlowgramPaletteItem,
  FlowgramPaletteSection,
  FlowgramConnectionDefaults,
} from './nodes/shared';

export {
  parseTimeoutMs,
  inferHttpWebhookKind,
  normalizeHttpBodyMode,
  IF_BRANCHES,
  TRYCATCH_BRANCHES,
  LOOP_BRANCHES,
  DEFAULT_SWITCH_BRANCHES,
  DEFAULT_HTTP_ALARM_TITLE_TEMPLATE,
  DEFAULT_HTTP_ALARM_BODY_TEMPLATE,
  DEFAULT_BARK_TITLE_TEMPLATE,
  DEFAULT_BARK_BODY_TEMPLATE,
} from './nodes/shared';

export { NODE_CATEGORIES } from './nodes/catalog';
export type { NodeCategory } from './nodes/catalog';

import type {
  FlowgramLogicBranch,
  NodeSeed,
  NodeCatalogInfo,
  NodeDefinition,
  FlowgramConnectionDefaults,
  FlowgramPaletteItem,
  FlowgramPaletteSection,
} from './nodes/shared';
import {
  isRecord,
  normalizeTimeoutValue,
  DEFAULT_HTTP_ALARM_TITLE_TEMPLATE,
  DEFAULT_HTTP_ALARM_BODY_TEMPLATE,
  DEFAULT_BARK_TITLE_TEMPLATE,
  DEFAULT_BARK_BODY_TEMPLATE,
} from './nodes/shared';
import { NODE_CATEGORIES } from './nodes/catalog';

import { definition as nativeDef } from './nodes/native';
import { definition as codeDef } from './nodes/code';
import { definition as timerDef } from './nodes/timer';
import { definition as serialTriggerDef } from './nodes/serialTrigger';
import { definition as modbusReadDef } from './nodes/modbusRead';
import { definition as capabilityCallDef } from './nodes/capabilityCall';
import { definition as mqttClientDef } from './nodes/mqttClient';
import { definition as ifDef } from './nodes/if';
import { definition as switchDef } from './nodes/switch';
import { definition as tryCatchDef } from './nodes/tryCatch';
import { definition as loopDef, LOOP_IN_POS, LOOP_OUT_POS } from './nodes/loop';
import { definition as httpClientDef } from './nodes/httpClient';
import { definition as barkPushDef } from './nodes/barkPush';
import { definition as sqlWriterDef } from './nodes/sqlWriter';
import { definition as debugConsoleDef } from './nodes/debugConsole';
import { definition as subgraphDef, SG_IN_POS, SG_OUT_POS } from './nodes/subgraph';
import { definition as subgraphInputDef } from './nodes/subgraphInput';
import { definition as subgraphOutputDef } from './nodes/subgraphOutput';
import { definition as c2fDef } from './nodes/c2f';
import { definition as lookupDef } from './nodes/lookup';
import { definition as humanLoopDef } from './nodes/humanLoop';
import { definition as minutesSinceDef } from './nodes/minutesSince';

interface FlowgramNodeData {
  label?: string;
  nodeType?: string;
  displayType?: unknown;
  connectionId?: string | null;
  timeoutMs?: number | null;
  config?: {
    message?: string;
    script?: string;
    branches?: FlowgramLogicBranch[];
    [key: string]: unknown;
  };
  [key: string]: unknown;
}

const ALL_DEFS = [
  nativeDef, codeDef, timerDef, serialTriggerDef, modbusReadDef, mqttClientDef,
  capabilityCallDef,
  ifDef, switchDef, tryCatchDef, loopDef,
  httpClientDef, barkPushDef, sqlWriterDef, debugConsoleDef,
  subgraphDef, subgraphInputDef, subgraphOutputDef,
  c2fDef, minutesSinceDef, lookupDef, humanLoopDef,
] as const satisfies readonly NodeDefinition[];

export type KnownEditorNodeType = (typeof ALL_DEFS)[number]['kind'];
export type NazhNodeKind = KnownEditorNodeType;
export type NazhNodeDisplayType = NazhNodeKind;
export type RuntimeNodeType = string;

const NODE_DEFS: readonly NodeDefinition[] = ALL_DEFS;
const DEF_MAP = new Map<NazhNodeKind, NodeDefinition>(
  NODE_DEFS.map((definition) => [definition.kind as NazhNodeKind, definition]),
);

export function isKnownEditorNodeType(value: unknown): value is KnownEditorNodeType {
  return typeof value === 'string' && DEF_MAP.has(value as NazhNodeKind);
}

export function isEditorOnlyNodeType(value: unknown): boolean {
  return isKnownEditorNodeType(value) && DEF_MAP.get(value)?.ai?.editorOnly === true;
}

export function preserveNodeType(value: unknown, fallback: RuntimeNodeType): RuntimeNodeType {
  return typeof value === 'string' && value.trim() ? value : fallback;
}

export function normalizeNodeKind(value: unknown): NazhNodeKind {
  return isKnownEditorNodeType(value) ? value : 'native';
}

function normalizeDisplayType(value: unknown, fallback: NazhNodeKind): NazhNodeDisplayType {
  return normalizeNodeKind(value ?? fallback);
}

export function getFallbackNodeLabel(kind: NazhNodeKind): string {
  return DEF_MAP.get(kind)?.fallbackLabel ?? nativeDef.fallbackLabel;
}

export function resolveNodeDisplayLabel(
  nodeType: unknown,
  label?: unknown,
): string {
  if (typeof label === 'string' && label.trim()) {
    return label.trim();
  }

  if (isKnownEditorNodeType(nodeType)) {
    return getFallbackNodeLabel(nodeType);
  }

  if (typeof nodeType === 'string' && nodeType.trim() && nodeType !== 'native') {
    return nodeType.trim();
  }

  return nativeDef.fallbackLabel;
}

export function normalizeNodeConfig(
  nodeType: NazhNodeKind,
  config: unknown,
): NodeSeed['config'] {
  return (DEF_MAP.get(nodeType) ?? nativeDef).normalizeConfig(config);
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
      ? resolveDefaultConnectionId(seed.kind as NazhNodeKind, connectionDefaults)
      : seed.connectionId;
  const label = resolveNodeDisplayLabel(seed.kind, seed.label || fallbackLabel);

  return {
    label,
    nodeType: seed.kind,
    displayType: normalizeDisplayType(seed.displayType, seed.kind as NazhNodeKind),
    connectionId,
    timeoutMs: seed.timeoutMs ?? null,
    config: normalizeNodeConfig(seed.kind as NazhNodeKind, seed.config),
  };
}

export function buildPaletteNodeJson(
  seed: NodeSeed,
  connectionDefaults: FlowgramConnectionDefaults,
  baseJson?: Partial<WorkflowNodeJSON>,
): Partial<WorkflowNodeJSON> {
  const fallbackLabel = resolveNodeDisplayLabel(seed.kind, seed.label);
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
  const fallbackLabel = resolveNodeDisplayLabel(nodeType);
  const normalizedConfig = normalizeNodeConfig(nodeType, rawData.config);

  return {
    ...json,
    type: nodeType,
    data: {
      ...rawData,
      label:
        typeof rawData.label === 'string' && rawData.label.trim()
          ? rawData.label.trim()
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

export function getFlowgramOutputPorts(
  nodeType: unknown,
  config: unknown,
): FlowgramLogicBranch[] {
  const kind = normalizeNodeKind(nodeType);
  return DEF_MAP.get(kind)?.getOutputPorts?.(config) ?? [];
}

export function getRoutingBranchDefinitions(
  nodeType: unknown,
  config: unknown,
): FlowgramLogicBranch[] {
  const kind = normalizeNodeKind(nodeType);
  return DEF_MAP.get(kind)?.getRoutingBranches?.(config) ?? [];
}

export function getLogicNodeBranchDefinitions(
  nodeType: unknown,
  config: unknown,
): FlowgramLogicBranch[] {
  return getFlowgramOutputPorts(nodeType, config);
}

export function getNodeCatalogInfo(nodeType: unknown): NodeCatalogInfo {
  if (isKnownEditorNodeType(nodeType)) {
    return DEF_MAP.get(nodeType)?.catalog ?? nativeDef.catalog;
  }

  return {
    category: '其他',
    description: '运行时或第三方节点',
  };
}

function validateUniqueBranchKeys(
  errors: string[],
  kind: string,
  source: string,
  branches: FlowgramLogicBranch[],
): void {
  const seen = new Set<string>();
  for (const branch of branches) {
    if (!branch.key.trim()) {
      errors.push(`${kind}.${source} 含空 branch key`);
      continue;
    }
    if (seen.has(branch.key)) {
      errors.push(`${kind}.${source} 重复 branch key: ${branch.key}`);
    }
    seen.add(branch.key);
  }
}

export function validateNodeRegistry(
  definitions: readonly NodeDefinition[] = NODE_DEFS,
): void {
  const errors: string[] = [];
  const seenKinds = new Set<string>();
  const categories = new Set<string>(NODE_CATEGORIES);

  for (const def of definitions) {
    if (seenKinds.has(def.kind)) {
      errors.push(`重复节点 kind: ${def.kind}`);
    }
    seenKinds.add(def.kind);

    if (!def.fallbackLabel.trim()) {
      errors.push(`${def.kind} 缺少 fallbackLabel`);
    }
    if (!def.catalog.description.trim()) {
      errors.push(`${def.kind} 缺少 catalog.description`);
    }
    if (!categories.has(def.catalog.category)) {
      errors.push(`${def.kind} catalog.category 非法: ${def.catalog.category}`);
    }

    const seed = def.buildDefaultSeed();
    if (seed.kind !== def.kind) {
      errors.push(`${def.kind} buildDefaultSeed().kind=${seed.kind}`);
    }

    if (def.palette?.visible === false && def.ai?.visible !== false) {
      errors.push(`${def.kind} 已隐藏 palette，但 AI catalog 未显式隐藏`);
    }
    if (def.ai?.editorOnly === true && def.ai.visible === false) {
      errors.push(`${def.kind} 同时声明 editorOnly 与 AI hidden`);
    }

    validateUniqueBranchKeys(errors, def.kind, 'outputs', def.getOutputPorts?.(seed.config) ?? []);
    validateUniqueBranchKeys(errors, def.kind, 'routingBranches', def.getRoutingBranches?.(seed.config) ?? []);
  }

  if (errors.length > 0) {
    throw new Error(`NodeDefinition 注册表不合法：\n${errors.map((error) => `- ${error}`).join('\n')}`);
  }
}

export function getDefaultHttpAlarmTitleTemplate(): string {
  return DEFAULT_HTTP_ALARM_TITLE_TEMPLATE;
}
export function getDefaultHttpAlarmBodyTemplate(): string {
  return DEFAULT_HTTP_ALARM_BODY_TEMPLATE;
}
export function getDefaultBarkTitleTemplate(): string {
  return DEFAULT_BARK_TITLE_TEMPLATE;
}
export function getDefaultBarkBodyTemplate(): string {
  return DEFAULT_BARK_BODY_TEMPLATE;
}

export function getNodeDefinition(kind: NazhNodeKind) {
  return DEF_MAP.get(kind);
}

export function getAllNodeDefinitions(): NodeDefinition<NazhNodeKind>[] {
  return [...NODE_DEFS] as NodeDefinition<NazhNodeKind>[];
}

export function buildDefaultNodeSeed(kind: NazhNodeKind): NodeSeed {
  const def = DEF_MAP.get(kind);
  return def ? def.buildDefaultSeed() : nativeDef.buildDefaultSeed();
}

/**
 * 子图节点拖入画布时自带 sg-in / sg-out 桥接节点，
 * 用户连内部业务节点即可形成完整子图（ADR-0013）。
 */
function buildSubgraphPaletteJson(
  seed: NodeSeed,
  connectionDefaults: FlowgramConnectionDefaults,
): Partial<import('@flowgram.ai/free-layout-editor').WorkflowNodeJSON> {
  const base = buildPaletteNodeJson(seed, connectionDefaults);
  return {
    ...base,
    blocks: [
      {
        id: 'sg-in',
        type: 'subgraphInput',
        meta: { position: { x: SG_IN_POS.x, y: SG_IN_POS.y } },
        data: {
          label: 'Input',
          nodeType: 'subgraphInput',
          config: {},
        },
      },
      {
        id: 'sg-out',
        type: 'subgraphOutput',
        meta: { position: { x: SG_OUT_POS.x, y: SG_OUT_POS.y } },
        data: {
          label: 'Output',
          nodeType: 'subgraphOutput',
          config: {},
        },
      },
    ],
    edges: [],
  };
}

function buildLoopPaletteJson(
  seed: NodeSeed,
  connectionDefaults: FlowgramConnectionDefaults,
): Partial<import('@flowgram.ai/free-layout-editor').WorkflowNodeJSON> {
  const base = buildPaletteNodeJson(seed, connectionDefaults);
  return {
    ...base,
    blocks: [
      {
        id: 'loop-iterate',
        type: 'subgraphInput',
        meta: { position: { x: LOOP_IN_POS.x, y: LOOP_IN_POS.y } },
        data: {
          label: 'Iterate',
          nodeType: 'subgraphInput',
          config: {},
        },
      },
      {
        id: 'loop-emit',
        type: 'subgraphOutput',
        meta: { position: { x: LOOP_OUT_POS.x, y: LOOP_OUT_POS.y } },
        data: {
          label: 'Emit',
          nodeType: 'subgraphOutput',
          config: {},
        },
      },
    ],
    edges: [],
  };
}

export function createFlowgramNodeRegistries(
  connectionDefaults: FlowgramConnectionDefaults,
): WorkflowNodeRegistry[] {
  return NODE_DEFS.map((def) => ({
    type: def.kind,
    meta: def.buildRegistryMeta(),
    onAdd: () => {
      if (def.kind === 'subgraph') {
        return buildSubgraphPaletteJson(def.buildDefaultSeed(), connectionDefaults);
      }
      if (def.kind === 'loop') {
        return buildLoopPaletteJson(def.buildDefaultSeed(), connectionDefaults);
      }
      return buildPaletteNodeJson(def.buildDefaultSeed(), connectionDefaults);
    },
  }));
}

export function getDefaultFlowgramNodeRegistry(type: string): WorkflowNodeRegistry {
  const kind = normalizeNodeKind(type);
  const def = DEF_MAP.get(kind);
  return {
    type: kind,
    meta: def ? def.buildRegistryMeta() : { defaultExpanded: true },
  };
}

export function getFlowgramPaletteSections(): FlowgramPaletteSection[] {
  const sections = NODE_CATEGORIES.map((category): FlowgramPaletteSection | null => {
    const items = NODE_DEFS
      .filter((def) => def.catalog.category === category && def.palette?.visible !== false)
      .map((def): FlowgramPaletteItem => ({
        key: `blank-${def.kind}`,
        title: def.palette?.title ?? def.fallbackLabel,
        description: def.catalog.description,
        badge: def.palette?.badge ?? def.fallbackLabel,
        seed: def.buildDefaultSeed(),
      }));

    if (items.length === 0) {
      return null;
    }

    return {
      key: category,
      title: category,
      items,
    };
  }).filter((section): section is FlowgramPaletteSection => section !== null);

  return [
    ...sections,
    {
      key: 'templates',
      title: '预设模板',
      items: NODE_TEMPLATES,
    },
  ];
}

const NODE_TEMPLATES: import('./nodes/shared').FlowgramPaletteItem[] = [
  {
    key: 'timer-trigger',
    title: '定时触发',
    description: '按固定间隔启动流程。',
    badge: 'Timer',
    seed: { idPrefix: 'timer_trigger', kind: 'timer', label: 'Timer Trigger', timeoutMs: null, config: { interval_ms: 5000, immediate: true, inject: { source: 'timer' } } },
  },
  {
    key: 'serial-trigger',
    title: '串口触发',
    description: '监听扫码枪、RFID 等串口外设主动上报。',
    badge: 'Serial',
    seed: { idPrefix: 'serial_trigger', kind: 'serialTrigger', label: 'Serial Trigger', timeoutMs: null, config: { inject: { source: 'serial' } } },
  },
  {
    key: 'modbus-temperature',
    title: 'Modbus 采集',
    description: '读取 Modbus 寄存器。',
    badge: 'Modbus',
    seed: { idPrefix: 'modbus_read', kind: 'modbusRead', label: 'Modbus Read', timeoutMs: 1000, config: { unit_id: 1, register: 40001, quantity: 1, register_type: 'holding', base_value: 64, amplitude: 6 } },
  },
  {
    key: 'switch-router',
    title: 'Switch 分流',
    description: '按 route 字段分支。',
    badge: 'Switch',
    seed: { idPrefix: 'switch_router', kind: 'switch', label: 'Switch Router', connectionId: null, timeoutMs: 1000, config: { script: 'payload["status"]', branches: [{ key: 'nominal', label: 'Nominal' }, { key: 'alert', label: 'Alert' }] } },
  },
  {
    key: 'payload-cleaner',
    title: '数据清洗',
    description: '脚本规范化数据结构。',
    badge: 'Code',
    seed: { idPrefix: 'code_clean', kind: 'code', label: 'Code Clean', timeoutMs: 1000, config: { script: 'payload["temperature"] = payload["value"]; payload["severity"] = payload["value"] > 88 ? "high" : "normal"; payload' } },
  },
  {
    key: 'bark-alert',
    title: 'Bark 推送',
    description: '向 iPhone 发送 Bark 告警通知。',
    badge: 'Bark',
    seed: { idPrefix: 'bark_push', kind: 'barkPush', label: 'Bark Alert', timeoutMs: 1000, config: { content_mode: 'body', title_template: DEFAULT_BARK_TITLE_TEMPLATE, subtitle_template: '{{payload.severity}}', body_template: DEFAULT_BARK_BODY_TEMPLATE, level: 'active', badge: '', sound: '', icon: '', group: 'nazh-alert', url: '', copy: '', image: '', auto_copy: false, call: false, archive_mode: 'inherit' } },
  },
  {
    key: 'dingtalk-alarm',
    title: '钉钉报警',
    description: '通过 HTTP 发告警。',
    badge: 'HTTP',
    seed: { idPrefix: 'http_alarm', kind: 'httpClient', label: 'HTTP Alert', timeoutMs: 1000, config: { body_mode: 'dingtalk_markdown', title_template: DEFAULT_HTTP_ALARM_TITLE_TEMPLATE, body_template: DEFAULT_HTTP_ALARM_BODY_TEMPLATE } },
  },
  {
    key: 'sqlite-audit',
    title: 'SQLite 记录',
    description: '写入本地审计记录。',
    badge: 'SQL',
    seed: { idPrefix: 'sql_writer', kind: 'sqlWriter', label: 'SQL Writer', timeoutMs: 1500, config: { database_path: './data/nazh.sqlite3', table: 'workflow_logs' } },
  },
  {
    key: 'debug-tap',
    title: '调试输出',
    description: '将数据打印到控制台。',
    badge: 'Debug',
    seed: { idPrefix: 'debug_console', kind: 'debugConsole', label: 'Debug Console', timeoutMs: 500, config: { label: 'runtime-tap', pretty: true } },
  },
];
