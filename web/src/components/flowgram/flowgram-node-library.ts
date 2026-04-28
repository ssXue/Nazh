import type {
  WorkflowNodeRegistry,
} from '@flowgram.ai/free-layout-editor';

export type {
  NazhNodeKind,
  NazhNodeDisplayType,
  FlowgramLogicBranch,
  FlowgramScriptAiConfig,
  NodeSeed,
  FlowgramPaletteItem,
  FlowgramPaletteSection,
  FlowgramConnectionDefaults,
} from './nodes/shared';

export {
  normalizeNodeKind,
  normalizeNodeConfig,
  normalizeFlowgramNodeJson,
  buildPaletteNodeJson,
  getLogicNodeBranchDefinitions,
  parseTimeoutMs,
  inferHttpWebhookKind,
  normalizeHttpBodyMode,
  resolveNodeData,
  resolveDefaultConnectionId,
  getFallbackNodeLabel,
  IF_BRANCHES,
  TRYCATCH_BRANCHES,
  LOOP_BRANCHES,
  DEFAULT_SWITCH_BRANCHES,
  DEFAULT_HTTP_ALARM_TITLE_TEMPLATE,
  DEFAULT_HTTP_ALARM_BODY_TEMPLATE,
  DEFAULT_BARK_TITLE_TEMPLATE,
  DEFAULT_BARK_BODY_TEMPLATE,
} from './nodes/shared';

export { NODE_CATEGORIES, NODE_CATEGORY_MAP } from './nodes/catalog';
export type { NodeCategory } from './nodes/catalog';

import type {
  NazhNodeKind,
  NodeSeed,
  FlowgramConnectionDefaults,
  FlowgramPaletteSection,
} from './nodes/shared';
import {
  normalizeNodeKind,
  buildPaletteNodeJson,
  DEFAULT_HTTP_ALARM_TITLE_TEMPLATE,
  DEFAULT_HTTP_ALARM_BODY_TEMPLATE,
  DEFAULT_BARK_TITLE_TEMPLATE,
  DEFAULT_BARK_BODY_TEMPLATE,
} from './nodes/shared';

import { definition as nativeDef } from './nodes/native';
import { definition as codeDef } from './nodes/code';
import { definition as timerDef } from './nodes/timer';
import { definition as serialTriggerDef } from './nodes/serialTrigger';
import { definition as modbusReadDef } from './nodes/modbusRead';
import { definition as mqttClientDef } from './nodes/mqttClient';
import { definition as ifDef } from './nodes/if';
import { definition as switchDef } from './nodes/switch';
import { definition as tryCatchDef } from './nodes/tryCatch';
import { definition as loopDef } from './nodes/loop';
import { definition as httpClientDef } from './nodes/httpClient';
import { definition as barkPushDef } from './nodes/barkPush';
import { definition as sqlWriterDef } from './nodes/sqlWriter';
import { definition as debugConsoleDef } from './nodes/debugConsole';
import { definition as subgraphDef, SG_IN_POS, SG_OUT_POS } from './nodes/subgraph';
import { definition as subgraphInputDef } from './nodes/subgraphInput';
import { definition as subgraphOutputDef } from './nodes/subgraphOutput';

const ALL_DEFS = [
  nativeDef, codeDef, timerDef, serialTriggerDef, modbusReadDef, mqttClientDef,
  ifDef, switchDef, tryCatchDef, loopDef,
  httpClientDef, barkPushDef, sqlWriterDef, debugConsoleDef,
  subgraphDef, subgraphInputDef, subgraphOutputDef,
];

const DEF_MAP = new Map(ALL_DEFS.map((d) => [d.kind, d]));

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

export function getAllNodeDefinitions() {
  return [...ALL_DEFS];
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

export function createFlowgramNodeRegistries(
  connectionDefaults: FlowgramConnectionDefaults,
): WorkflowNodeRegistry[] {
  return ALL_DEFS.map((def) => ({
    type: def.kind,
    meta: def.buildRegistryMeta(),
    onAdd: () => {
      if (def.kind === 'subgraph') {
        return buildSubgraphPaletteJson(def.buildDefaultSeed(), connectionDefaults);
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
  return [
    {
      key: 'blank',
      title: '基础节点',
      items: [
        { key: 'blank-timer', title: 'Timer', description: timerDef.catalog.description, badge: 'Timer', seed: timerDef.buildDefaultSeed() },
        { key: 'blank-serial', title: 'Serial Trigger', description: serialTriggerDef.catalog.description, badge: 'Serial', seed: serialTriggerDef.buildDefaultSeed() },
        { key: 'blank-modbus', title: 'Modbus Read', description: modbusReadDef.catalog.description, badge: 'Modbus', seed: modbusReadDef.buildDefaultSeed() },
        { key: 'blank-mqtt', title: 'MQTT Client', description: mqttClientDef.catalog.description, badge: 'MQTT', seed: mqttClientDef.buildDefaultSeed() },
        { key: 'blank-code', title: 'Code Node', description: codeDef.catalog.description, badge: 'Code', seed: codeDef.buildDefaultSeed() },
        { key: 'blank-http', title: 'HTTP Client', description: httpClientDef.catalog.description, badge: 'HTTP', seed: httpClientDef.buildDefaultSeed() },
        { key: 'blank-bark', title: 'Bark Push', description: barkPushDef.catalog.description, badge: 'Bark', seed: barkPushDef.buildDefaultSeed() },
        { key: 'blank-sql', title: 'SQL Writer', description: sqlWriterDef.catalog.description, badge: 'SQL', seed: sqlWriterDef.buildDefaultSeed() },
        { key: 'blank-debug', title: 'Debug Console', description: debugConsoleDef.catalog.description, badge: 'Debug', seed: debugConsoleDef.buildDefaultSeed() },
        { key: 'blank-native', title: 'Native', description: nativeDef.catalog.description, badge: 'Native', seed: nativeDef.buildDefaultSeed() },
      ],
    },
    {
      key: 'logic',
      title: '逻辑节点',
      items: [
        { key: 'blank-if', title: 'IF 条件', description: ifDef.catalog.description, badge: 'IF', seed: ifDef.buildDefaultSeed() },
        { key: 'blank-switch', title: 'Switch 分流', description: switchDef.catalog.description, badge: 'Switch', seed: switchDef.buildDefaultSeed() },
        { key: 'blank-try-catch', title: 'Try 捕获', description: tryCatchDef.catalog.description, badge: 'Try', seed: tryCatchDef.buildDefaultSeed() },
        { key: 'blank-loop', title: 'Loop 迭代', description: loopDef.catalog.description, badge: 'Loop', seed: loopDef.buildDefaultSeed() },
      ],
    },
    {
      key: 'subgraph',
      title: '子图封装',
      items: [
        { key: 'blank-subgraph', title: 'Subgraph', description: subgraphDef.catalog.description, badge: 'Subgraph', seed: subgraphDef.buildDefaultSeed() },
      ],
    },
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
