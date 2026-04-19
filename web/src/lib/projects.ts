import { formatWorkflowGraph } from './flowgram';
import { parseWorkflowGraph } from './graph';
import type {
  ConnectionDefinition,
  JsonValue,
  WorkflowGraph,
  WorkflowNodeDefinition,
} from '../types';

export const CURRENT_USER_NAME = 'ssxue';
export const PROJECT_LIBRARY_STORAGE_KEY = 'nazh.project-library';
export const PROJECT_PACKAGE_KIND = 'nazh.project';
export const PROJECT_LIBRARY_KIND = 'nazh.project-library';
export const PROJECT_SCHEMA_VERSION = 2;
export const MAX_PROJECT_SNAPSHOTS = 20;

export interface ProjectEnvironmentDiff {
  connections?: Record<string, JsonValue>;
  nodeConfigs?: Record<string, JsonValue>;
}

export interface ProjectEnvironment {
  id: string;
  name: string;
  description: string;
  updatedAt: string;
  diff: ProjectEnvironmentDiff;
}

export type ProjectSnapshotReason =
  | 'seed'
  | 'manual'
  | 'import'
  | 'migration'
  | 'rollback';

export interface ProjectSnapshot {
  id: string;
  label: string;
  description: string;
  createdAt: string;
  reason: ProjectSnapshotReason;
  astText: string;
  payloadText: string;
  activeEnvironmentId: string;
  environments: ProjectEnvironment[];
}

export interface ProjectRecord {
  id: string;
  name: string;
  description: string;
  createdAt: string;
  updatedAt: string;
  astText: string;
  payloadText: string;
  activeEnvironmentId: string;
  environments: ProjectEnvironment[];
  snapshots: ProjectSnapshot[];
  migrationNotes: string[];
}

export interface ProjectLibraryState {
  kind: typeof PROJECT_LIBRARY_KIND;
  schemaVersion: typeof PROJECT_SCHEMA_VERSION;
  projects: ProjectRecord[];
}

export interface ProjectPackage {
  kind: typeof PROJECT_PACKAGE_KIND;
  schemaVersion: typeof PROJECT_SCHEMA_VERSION;
  exportedAt: string;
  project: ProjectRecord;
}

export interface ImportProjectsResult {
  importedProjects: ProjectRecord[];
  migrationNotes: string[];
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null && !Array.isArray(value);
}

function cloneJson<T>(value: T): T {
  return JSON.parse(JSON.stringify(value)) as T;
}

function nowIso(): string {
  return new Date().toISOString();
}

function slugify(value: string): string {
  const normalized = value
    .trim()
    .toLowerCase()
    .replace(/[^a-z0-9\u4e00-\u9fa5]+/g, '-')
    .replace(/^-+|-+$/g, '');

  return normalized || 'project';
}

function createId(prefix: string): string {
  if (typeof crypto !== 'undefined' && 'randomUUID' in crypto) {
    return `${prefix}-${crypto.randomUUID().slice(0, 8)}`;
  }

  return `${prefix}-${Math.random().toString(36).slice(2, 10)}`;
}

function normalizeString(value: unknown, fallback: string): string {
  return typeof value === 'string' && value.trim() ? value.trim() : fallback;
}

function normalizeJsonValue(value: unknown, fallback: JsonValue = {}): JsonValue {
  if (
    value === null ||
    typeof value === 'string' ||
    typeof value === 'number' ||
    typeof value === 'boolean'
  ) {
    return value;
  }

  if (Array.isArray(value)) {
    return value.map((item) => normalizeJsonValue(item, null));
  }

  if (isRecord(value)) {
    return Object.entries(value).reduce<Record<string, JsonValue>>((acc, [key, nextValue]) => {
      acc[key] = normalizeJsonValue(nextValue, null);
      return acc;
    }, {});
  }

  return fallback;
}

function asJsonObject(value: unknown): Record<string, JsonValue> {
  const normalized = normalizeJsonValue(value, {});
  return isRecord(normalized) ? (normalized as Record<string, JsonValue>) : {};
}

function deepMergeJson(baseValue: JsonValue, overrideValue: JsonValue): JsonValue {
  if (Array.isArray(overrideValue)) {
    return cloneJson(overrideValue);
  }

  if (isRecord(baseValue) && isRecord(overrideValue)) {
    const result: Record<string, JsonValue> = { ...baseValue } as Record<string, JsonValue>;

    Object.entries(overrideValue).forEach(([key, nextValue]) => {
      const currentValue = result[key] ?? null;
      const normalizedNextValue = normalizeJsonValue(nextValue, null);
      result[key] =
        isRecord(currentValue) && isRecord(normalizedNextValue)
          ? deepMergeJson(currentValue, normalizedNextValue)
          : cloneJson(normalizedNextValue);
    });

    return result;
  }

  return cloneJson(overrideValue);
}

function formatSnapshotReason(reason: ProjectSnapshotReason): string {
  switch (reason) {
    case 'seed':
      return '模板初始化';
    case 'manual':
      return '手动快照';
    case 'import':
      return '导入';
    case 'migration':
      return '迁移';
    case 'rollback':
      return '回滚前保护';
  }
}

export function formatRelativeTimestamp(timestamp: string): string {
  const target = new Date(timestamp).getTime();
  if (Number.isNaN(target)) {
    return '未知时间';
  }

  const diff = Date.now() - target;
  const minute = 60 * 1000;
  const hour = 60 * minute;
  const day = 24 * hour;

  if (diff < minute) {
    return '刚刚';
  }

  if (diff < hour) {
    return `${Math.max(1, Math.floor(diff / minute))} 分钟前`;
  }

  if (diff < day) {
    return `${Math.max(1, Math.floor(diff / hour))} 小时前`;
  }

  if (diff < 7 * day) {
    return `${Math.max(1, Math.floor(diff / day))} 天前`;
  }

  return new Intl.DateTimeFormat('zh-CN', {
    month: '2-digit',
    day: '2-digit',
    hour: '2-digit',
    minute: '2-digit',
  }).format(target);
}

export function parseProjectNodeCount(astText: string): number {
  const parsed = parseWorkflowGraph(astText);
  return parsed.graph ? Object.keys(parsed.graph.nodes).length : 0;
}

function createEnvironment(
  name: string,
  description: string,
  diff: ProjectEnvironmentDiff = {},
): ProjectEnvironment {
  return {
    id: createId('env'),
    name,
    description,
    updatedAt: nowIso(),
    diff: cloneJson(diff),
  };
}

function createSnapshot(
  project: Pick<
    ProjectRecord,
    'name' | 'description' | 'astText' | 'payloadText' | 'activeEnvironmentId' | 'environments'
  >,
  reason: ProjectSnapshotReason,
  label?: string,
  description?: string,
): ProjectSnapshot {
  const createdAt = nowIso();

  return {
    id: createId('snapshot'),
    label: label ?? `${formatSnapshotReason(reason)} · ${project.name}`,
    description: description ?? project.description,
    createdAt,
    reason,
    astText: normalizeProjectAstText(project.astText),
    payloadText: project.payloadText,
    activeEnvironmentId: project.activeEnvironmentId,
    environments: cloneJson(project.environments),
  };
}

function ensureSnapshotLimit(snapshots: ProjectSnapshot[]): ProjectSnapshot[] {
  return snapshots
    .slice()
    .sort((left, right) => right.createdAt.localeCompare(left.createdAt))
    .slice(0, MAX_PROJECT_SNAPSHOTS);
}

function createNode(
  id: string,
  type: string,
  x: number,
  y: number,
  config: JsonValue,
  extras: Partial<WorkflowNodeDefinition> = {},
): WorkflowNodeDefinition {
  return {
    id,
    type,
    config,
    meta: {
      position: { x, y },
    },
    ...extras,
  };
}

function buildIndustrialAlarmExample(boardName: string): WorkflowGraph {
  return {
    name: boardName,
    connections: [
      {
        id: 'plc-main',
        type: 'modbus',
        metadata: {
          host: '192.168.10.11',
          port: 502,
          unit_id: 1,
          register: 40001,
        },
      },
    ],
    nodes: {
      timer_trigger: createNode('timer_trigger', 'timer', 48, 88, {
        interval_ms: 5000,
        immediate: true,
        inject: {
          gateway: 'edge-a',
          scene: boardName,
        },
      }),
      modbus_read: createNode(
        'modbus_read',
        'modbusRead',
        348,
        88,
        {
          unit_id: 1,
          register: 40001,
          quantity: 1,
          base_value: 68,
          amplitude: 6,
        },
        {
          connection_id: 'plc-main',
          timeout_ms: 1000,
        },
      ),
      code_clean: createNode(
        'code_clean',
        'code',
        648,
        88,
        {
          script:
            'let value = payload["value"]; payload["temperature_c"] = value; payload["temperature_f"] = (value * 1.8) + 32.0; payload["severity"] = value > 120 ? "alert" : "nominal"; payload["route"] = payload["severity"]; payload["tag"] = `${payload["gateway"]}:boiler-a`; payload',
        },
        {
          timeout_ms: 1000,
        },
      ),
      route_switch: createNode(
        'route_switch',
        'switch',
        968,
        72,
        {
          script: 'payload["route"]',
          branches: [
            { key: 'nominal', label: 'Nominal' },
            { key: 'alert', label: 'Alert' },
          ],
        },
        {
          timeout_ms: 1000,
        },
      ),
      sql_writer: createNode(
        'sql_writer',
        'sqlWriter',
        1288,
        176,
        {
          database_path: './data/edge-runtime.sqlite3',
          table: 'temperature_audit',
        },
        {
          timeout_ms: 1500,
        },
      ),
      http_alarm: createNode(
        'http_alarm',
        'httpClient',
        1288,
        -8,
        {
          method: 'POST',
          url: 'https://oapi.dingtalk.com/robot/send?access_token=replace_me',
          webhook_kind: 'dingtalk',
          body_mode: 'dingtalk_markdown',
          content_type: 'application/json',
          request_timeout_ms: 4000,
          title_template: 'Nazh 工业告警 · {{payload.tag}} · {{payload.severity}}',
          body_template:
            '### Nazh 工业告警\n- 设备：{{payload.tag}}\n- 场景：{{payload.scene}}\n- 温度：{{payload.temperature_c}} °C / {{payload.temperature_f}} °F\n- 严重级别：{{payload.severity}}\n- Trace：{{trace_id}}\n- 时间：{{timestamp}}',
          at_mobiles: [],
          at_all: false,
          headers: {
            'X-Alarm-Source': 'nazh',
          },
        },
        {
          timeout_ms: 1500,
        },
      ),
      debug_console: createNode(
        'debug_console',
        'debugConsole',
        1608,
        88,
        {
          label: 'final-output',
          pretty: true,
        },
        {
          timeout_ms: 500,
        },
      ),
    },
    edges: [
      { from: 'timer_trigger', to: 'modbus_read' },
      { from: 'modbus_read', to: 'code_clean' },
      { from: 'code_clean', to: 'route_switch' },
      { from: 'route_switch', to: 'sql_writer', source_port_id: 'nominal' },
      { from: 'route_switch', to: 'http_alarm', source_port_id: 'alert' },
      { from: 'sql_writer', to: 'debug_console' },
      { from: 'http_alarm', to: 'debug_console' },
    ],
  };
}

function buildDataPipelineExample(boardName: string): WorkflowGraph {
  return {
    name: boardName,
    connections: [
      {
        id: 'serial-ingress',
        type: 'serial',
        metadata: {
          port_path: '/dev/tty.usbserial-0001',
          baud_rate: 9600,
        },
      },
      {
        id: 'warehouse-http',
        type: 'http',
        metadata: {
          url: 'https://example.com/iot/ingest',
          method: 'POST',
        },
      },
    ],
    nodes: {
      serial_trigger: createNode(
        'serial_trigger',
        'serialTrigger',
        56,
        96,
        {
          decode: 'ascii',
          trim: true,
        },
        {
          connection_id: 'serial-ingress',
        },
      ),
      clean_payload: createNode(
        'clean_payload',
        'code',
        372,
        96,
        {
          script:
            'payload["barcode"] = String(payload["value"] ?? payload["raw"] ?? "").trim(); payload["line"] = "packing"; payload["received_at"] = timestamp; payload',
        },
        {
        },
      ),
      persist_raw: createNode(
        'persist_raw',
        'sqlWriter',
        692,
        188,
        {
          database_path: './data/barcode-audit.sqlite3',
          table: 'barcode_events',
        },
        {
        },
      ),
      forward_http: createNode(
        'forward_http',
        'httpClient',
        692,
        4,
        {
          method: 'POST',
          url: 'https://example.com/iot/ingest',
          body_mode: 'json',
          content_type: 'application/json',
          headers: {
            'X-Source': 'nazh-edge',
          },
        },
        {
          connection_id: 'warehouse-http',
        },
      ),
      console_tap: createNode(
        'console_tap',
        'debugConsole',
        1034,
        96,
        {
          label: 'barcode-stream',
          pretty: true,
        },
        {
        },
      ),
    },
    edges: [
      { from: 'serial_trigger', to: 'clean_payload' },
      { from: 'clean_payload', to: 'persist_raw' },
      { from: 'clean_payload', to: 'forward_http' },
      { from: 'persist_raw', to: 'console_tap' },
      { from: 'forward_http', to: 'console_tap' },
    ],
  };
}

function buildNotificationExample(boardName: string): WorkflowGraph {
  return {
    name: boardName,
    connections: [
      {
        id: 'mqtt-broker',
        type: 'mqtt',
        metadata: {
          host: 'broker.emqx.io',
          port: 1883,
          topic: 'factory/alerts',
        },
      },
    ],
    nodes: {
      timer_trigger: createNode('timer_trigger', 'timer', 56, 90, {
        interval_ms: 10000,
        immediate: true,
        inject: {
          source: 'supervisor',
          board: boardName,
        },
      }),
      compose_alert: createNode(
        'compose_alert',
        'code',
        356,
        90,
        {
          script:
            'payload["severity"] = "warn"; payload["message"] = `${payload["source"]} check-in`; payload["topic"] = "factory/alerts"; payload',
        },
      ),
      branch_by_severity: createNode(
        'branch_by_severity',
        'if',
        668,
        82,
        {
          condition: 'payload["severity"] == "warn"',
          true_label: 'warn',
          false_label: 'ignore',
        },
      ),
      notify_http: createNode(
        'notify_http',
        'httpClient',
        984,
        -6,
        {
          method: 'POST',
          url: 'https://example.com/robot/notify',
          body_mode: 'json',
          content_type: 'application/json',
        },
      ),
      debug_console: createNode(
        'debug_console',
        'debugConsole',
        984,
        178,
        {
          label: 'notify-result',
          pretty: true,
        },
      ),
    },
    edges: [
      { from: 'timer_trigger', to: 'compose_alert' },
      { from: 'compose_alert', to: 'branch_by_severity' },
      { from: 'branch_by_severity', to: 'notify_http', source_port_id: 'true' },
      { from: 'branch_by_severity', to: 'debug_console', source_port_id: 'false' },
      { from: 'notify_http', to: 'debug_console' },
    ],
  };
}

function buildStarterWorkflow(boardName: string): WorkflowGraph {
  return {
    name: boardName,
    connections: [],
    nodes: {
      timer_trigger: createNode('timer_trigger', 'timer', 64, 116, {
        interval_ms: 3000,
        immediate: true,
      }),
      debug_console: createNode(
        'debug_console',
        'debugConsole',
        368,
        116,
        {
          label: 'starter',
          pretty: true,
        },
      ),
    },
    edges: [{ from: 'timer_trigger', to: 'debug_console' }],
  };
}

function buildSeedProjectRecord(
  id: string,
  name: string,
  description: string,
  graph: WorkflowGraph,
  payloadText: string,
  environments: ProjectEnvironment[],
): ProjectRecord {
  const astText = formatWorkflowGraph(graph);
  const createdAt = nowIso();
  const record: ProjectRecord = {
    id,
    name,
    description,
    createdAt,
    updatedAt: createdAt,
    astText,
    payloadText,
    activeEnvironmentId: environments[0]?.id ?? '',
    environments,
    snapshots: [],
    migrationNotes: [],
  };

  return {
    ...record,
    snapshots: [
      createSnapshot(
        record,
        'seed',
        '模板初始化',
        '首个可回滚版本，来自工程模板初始化。',
      ),
    ],
  };
}

function buildSeedProjects(): ProjectRecord[] {
  return [
    buildSeedProjectRecord(
      'industrial-alarm',
      '工业告警联动',
      'Timer + Modbus + Code + Switch + HTTP / SQLite / Debug 的完整示例工程',
      buildIndustrialAlarmExample('工业告警联动'),
      JSON.stringify(
        {
          manual: true,
          operator: CURRENT_USER_NAME,
          reason: 'manual override',
        },
        null,
        2,
      ),
      [
        createEnvironment('生产环境', '现场设备与正式通知通道。'),
        createEnvironment('测试环境', '替换 PLC 地址、数据库路径和告警 URL。', {
          connections: {
            'plc-main': {
              host: '192.168.10.99',
              port: 1502,
            },
          },
          nodeConfigs: {
            sql_writer: {
              database_path: './data/test-edge-runtime.sqlite3',
            },
            http_alarm: {
              url: 'https://oapi.dingtalk.com/robot/send?access_token=test_replace_me',
              headers: {
                'X-Alarm-Source': 'nazh-test',
              },
            },
          },
        }),
      ],
    ),
    buildSeedProjectRecord(
      'data-pipeline',
      '数据管道',
      '串口采集、清洗、落库与 HTTP 转发的边缘数据工程',
      buildDataPipelineExample('数据管道'),
      JSON.stringify(
        {
          raw: '  SN-2026-0411-0007  ',
          station: 'packing-line',
        },
        null,
        2,
      ),
      [
        createEnvironment('生产环境', '对接正式串口设备与上游接口。'),
        createEnvironment('开发环境', '将串口与 HTTP 目标切换到本地模拟链路。', {
          connections: {
            'serial-ingress': {
              port_path: '/dev/tty.usbserial-mock',
            },
            'warehouse-http': {
              url: 'http://127.0.0.1:8787/mock/ingest',
            },
          },
          nodeConfigs: {
            forward_http: {
              url: 'http://127.0.0.1:8787/mock/ingest',
            },
          },
        }),
      ],
    ),
    buildSeedProjectRecord(
      'notification-flow',
      '告警通知流',
      '从巡检心跳到通知分流的轻量告警工程',
      buildNotificationExample('告警通知流'),
      JSON.stringify(
        {
          source: '巡检站',
        },
        null,
        2,
      ),
      [
        createEnvironment('生产环境', '正式机器人通道。'),
        createEnvironment('预演环境', '只输出到调试台，不发送正式通知。', {
          nodeConfigs: {
            notify_http: {
              url: 'https://example.com/robot/rehearsal',
            },
          },
        }),
      ],
    ),
  ];
}

export function buildDefaultProjectLibrary(): ProjectLibraryState {
  return {
    kind: PROJECT_LIBRARY_KIND,
    schemaVersion: PROJECT_SCHEMA_VERSION,
    projects: buildSeedProjects(),
  };
}

function normalizeEnvironmentDiff(value: unknown): ProjectEnvironmentDiff {
  if (!isRecord(value)) {
    return {};
  }

  return {
    connections: isRecord(value.connections) ? asJsonObject(value.connections) : {},
    nodeConfigs: isRecord(value.nodeConfigs)
      ? asJsonObject(value.nodeConfigs)
      : isRecord(value.node_configs)
        ? asJsonObject(value.node_configs)
        : {},
  };
}

function normalizeEnvironment(value: unknown, index: number): ProjectEnvironment {
  const source = isRecord(value) ? value : {};

  return {
    id: normalizeString(source.id, `env-${index + 1}`),
    name: normalizeString(source.name, `环境 ${index + 1}`),
    description: typeof source.description === 'string' ? source.description : '',
    updatedAt: normalizeString(source.updatedAt, nowIso()),
    diff: normalizeEnvironmentDiff(source.diff),
  };
}

function normalizeSnapshot(value: unknown, project: ProjectRecord, index: number): ProjectSnapshot {
  const source = isRecord(value) ? value : {};
  const environments = Array.isArray(source.environments)
    ? source.environments.map((item, itemIndex) => normalizeEnvironment(item, itemIndex))
    : cloneJson(project.environments);
  const activeEnvironmentId = normalizeString(
    source.activeEnvironmentId,
    environments[0]?.id ?? project.activeEnvironmentId,
  );

  return {
    id: normalizeString(source.id, `snapshot-${index + 1}`),
    label: normalizeString(source.label, `快照 ${index + 1}`),
    description: typeof source.description === 'string' ? source.description : project.description,
    createdAt: normalizeString(source.createdAt, nowIso()),
    reason:
      source.reason === 'seed' ||
      source.reason === 'manual' ||
      source.reason === 'import' ||
      source.reason === 'migration' ||
      source.reason === 'rollback'
        ? source.reason
        : 'manual',
    astText: normalizeProjectAstText(normalizeString(source.astText, project.astText)),
    payloadText: normalizeString(source.payloadText, project.payloadText),
    activeEnvironmentId,
    environments,
  };
}

function normalizeProjectRecord(
  value: unknown,
  fallbackName: string,
  migrationNotes: string[] = [],
): ProjectRecord {
  const source = isRecord(value) ? value : {};
  const astTextCandidate =
    typeof source.astText === 'string'
      ? source.astText
      : typeof source.workflowAst === 'string'
        ? source.workflowAst
        : formatWorkflowGraph(buildStarterWorkflow(fallbackName));
  const parsedGraph = parseWorkflowGraph(astTextCandidate);
  const astText = parsedGraph.graph
    ? formatWorkflowGraph(parsedGraph.graph)
    : formatWorkflowGraph(buildStarterWorkflow(fallbackName));
  const createdAt = normalizeString(source.createdAt, nowIso());
  const updatedAt = normalizeString(source.updatedAt, createdAt);
  const environments = Array.isArray(source.environments)
    ? source.environments.map((item, index) => normalizeEnvironment(item, index))
    : [createEnvironment('生产环境', '默认环境。')];
  const activeEnvironmentId = normalizeString(
    source.activeEnvironmentId,
    environments[0]?.id ?? '',
  );

  const project: ProjectRecord = {
    id: normalizeString(source.id, createId('project')),
    name: normalizeString(source.name, fallbackName),
    description:
      typeof source.description === 'string' ? source.description : '从旧版工程结构迁移而来。',
    createdAt,
    updatedAt,
    astText,
    payloadText:
      typeof source.payloadText === 'string'
        ? source.payloadText
        : JSON.stringify({ manual: true }, null, 2),
    activeEnvironmentId,
    environments,
    snapshots: [],
    migrationNotes: [
      ...migrationNotes,
      ...(Array.isArray(source.migrationNotes)
        ? source.migrationNotes.filter((item): item is string => typeof item === 'string')
        : []),
    ],
  };

  const snapshots = Array.isArray(source.snapshots)
    ? source.snapshots.map((item, index) => normalizeSnapshot(item, project, index))
    : [];

  return {
    ...project,
    snapshots:
      snapshots.length > 0
        ? ensureSnapshotLimit(snapshots)
        : [createSnapshot(project, project.migrationNotes.length > 0 ? 'migration' : 'seed')],
  };
}

function isWorkflowGraphLike(value: unknown): value is WorkflowGraph {
  return isRecord(value) && isRecord(value.nodes) && Array.isArray(value.edges);
}

function ensureUniqueProjectId(projects: ProjectRecord[], preferredId: string): string {
  const normalizedId = slugify(preferredId);
  const existingIds = new Set(projects.map((project) => project.id));
  if (!existingIds.has(normalizedId)) {
    return normalizedId;
  }

  let suffix = 2;
  while (existingIds.has(`${normalizedId}-${suffix}`)) {
    suffix += 1;
  }

  return `${normalizedId}-${suffix}`;
}

export function loadProjectLibrary(): ProjectLibraryState {
  if (typeof window === 'undefined') {
    return buildDefaultProjectLibrary();
  }

  try {
    const raw = window.localStorage.getItem(PROJECT_LIBRARY_STORAGE_KEY);
    if (!raw) {
      return buildDefaultProjectLibrary();
    }
    return parseProjectLibraryText(raw);
  } catch {
    return buildDefaultProjectLibrary();
  }
}

export function parseProjectLibraryText(raw: string): ProjectLibraryState {
  const parsed = JSON.parse(raw) as unknown;
  if (!isRecord(parsed) || !Array.isArray(parsed.projects)) {
    throw new Error('工程库文件格式无效。');
  }

  const projects = parsed.projects.map((item, index) =>
    normalizeProjectRecord(item, `工程 ${index + 1}`),
  );

  return {
    kind: PROJECT_LIBRARY_KIND,
    schemaVersion: PROJECT_SCHEMA_VERSION,
    projects,
  };
}

export function persistProjectLibrary(library: ProjectLibraryState) {
  if (typeof window === 'undefined') {
    return;
  }

  try {
    window.localStorage.setItem(PROJECT_LIBRARY_STORAGE_KEY, JSON.stringify(library));
  } catch {
    // Ignore storage failures.
  }
}

export function getActiveEnvironment(project: ProjectRecord): ProjectEnvironment | null {
  return (
    project.environments.find((environment) => environment.id === project.activeEnvironmentId) ??
    project.environments[0] ??
    null
  );
}

export function applyEnvironmentToGraph(
  graph: WorkflowGraph,
  environment: ProjectEnvironment | null,
): WorkflowGraph {
  if (!environment) {
    return cloneJson(graph);
  }

  const nextGraph = cloneJson(graph);
  const nodeDiffs = environment.diff.nodeConfigs ?? {};

  Object.entries(nodeDiffs).forEach(([nodeId, override]) => {
    const targetNode = nextGraph.nodes[nodeId];
    if (!targetNode) {
      return;
    }

    targetNode.config = deepMergeJson(targetNode.config ?? {}, override);
  });

  return nextGraph;
}

export function applyEnvironmentToConnectionDefinitions(
  definitions: ConnectionDefinition[],
  environment: ProjectEnvironment | null,
): ConnectionDefinition[] {
  if (!environment) {
    return cloneJson(definitions);
  }

  const connectionDiffs = environment.diff.connections ?? {};
  return definitions.map((definition) => {
    const override = connectionDiffs[definition.id];
    if (!override) {
      return cloneJson(definition);
    }

    return {
      ...cloneJson(definition),
      metadata: deepMergeJson(definition.metadata, override),
    };
  });
}

function stripGraphConnectionDefinitions(graph: WorkflowGraph): WorkflowGraph {
  return {
    ...cloneJson(graph),
    connections: [],
  };
}

function normalizeProjectAstText(astText: string): string {
  const parsed = parseWorkflowGraph(astText);
  return parsed.graph
    ? formatWorkflowGraph(stripGraphConnectionDefinitions(parsed.graph))
    : astText;
}

export function createNewProjectRecord(name: string, description?: string): ProjectRecord {
  const projectName = name.trim() || '未命名工程';
  const graph = buildStarterWorkflow(projectName);
  const astText = formatWorkflowGraph(stripGraphConnectionDefinitions(graph));
  const createdAt = nowIso();
  const environments = [createEnvironment('生产环境', '默认环境。')];
  const project: ProjectRecord = {
    id: slugify(projectName),
    name: projectName,
    description: description?.trim() || '新的工作流工程',
    createdAt,
    updatedAt: createdAt,
    astText,
    payloadText: JSON.stringify(
      {
        manual: true,
        created_by: CURRENT_USER_NAME,
      },
      null,
      2,
    ),
    activeEnvironmentId: environments[0].id,
    environments,
    snapshots: [],
    migrationNotes: [],
  };

  return {
    ...project,
    snapshots: [createSnapshot(project, 'seed', '初始版本', '创建工程时生成的首个版本。')],
  };
}

export function renameProjectRecord(
  project: ProjectRecord,
  patch: Partial<Pick<ProjectRecord, 'name' | 'description' | 'astText' | 'payloadText'>>,
): ProjectRecord {
  const normalizedAstText =
    typeof patch.astText === 'string' ? normalizeProjectAstText(patch.astText) : patch.astText;

  return {
    ...project,
    ...patch,
    ...(normalizedAstText === undefined ? {} : { astText: normalizedAstText }),
    updatedAt: nowIso(),
  };
}

export function createProjectSnapshot(
  project: ProjectRecord,
  label?: string,
  description?: string,
): ProjectRecord {
  return {
    ...project,
    updatedAt: nowIso(),
    snapshots: ensureSnapshotLimit([
      createSnapshot(project, 'manual', label, description),
      ...project.snapshots,
    ]),
  };
}

export function rollbackProjectToSnapshot(
  project: ProjectRecord,
  snapshotId: string,
): ProjectRecord {
  const target = project.snapshots.find((snapshot) => snapshot.id === snapshotId);
  if (!target) {
    return project;
  }

  const rollbackProtection = createSnapshot(
    project,
    'rollback',
    `回滚前 · ${project.name}`,
    `回滚到 ${target.label} 之前自动保留的版本。`,
  );

  return {
    ...project,
    astText: target.astText,
    payloadText: target.payloadText,
    activeEnvironmentId: target.activeEnvironmentId,
    environments: cloneJson(target.environments),
    updatedAt: nowIso(),
    snapshots: ensureSnapshotLimit([rollbackProtection, ...project.snapshots]),
  };
}

export function upsertProjectRecord(
  projects: ProjectRecord[],
  project: ProjectRecord,
): ProjectRecord[] {
  const existingIndex = projects.findIndex((item) => item.id === project.id);
  if (existingIndex === -1) {
    return [project, ...projects];
  }

  const nextProjects = projects.slice();
  nextProjects[existingIndex] = project;
  return nextProjects;
}

function buildImportedProjectFromGraph(graph: WorkflowGraph, name?: string): ProjectRecord {
  const projectName = name?.trim() || graph.name?.trim() || '导入工程';
  const environments = [createEnvironment('生产环境', '导入后生成的默认环境。')];
  const createdAt = nowIso();
  const project: ProjectRecord = {
    id: slugify(projectName),
    name: projectName,
    description: '从裸工作流 AST 导入并迁移得到。',
    createdAt,
    updatedAt: createdAt,
    astText: formatWorkflowGraph(graph),
    payloadText: JSON.stringify({ imported: true }, null, 2),
    activeEnvironmentId: environments[0].id,
    environments,
    snapshots: [],
    migrationNotes: ['已从裸工作流 AST 迁移为 Nazh 工程包。'],
  };

  return {
    ...project,
    snapshots: [
      createSnapshot(project, 'import', '导入版本', '从裸工作流导入后创建的首个版本。'),
    ],
  };
}

export function importProjectsFromText(sourceText: string): ImportProjectsResult {
  const parsed = JSON.parse(sourceText) as unknown;

  if (isWorkflowGraphLike(parsed)) {
    const project = buildImportedProjectFromGraph(parsed);
    return {
      importedProjects: [project],
      migrationNotes: project.migrationNotes,
    };
  }

  if (!isRecord(parsed)) {
    throw new Error('导入文件不是有效的项目包。');
  }

  if (parsed.kind === PROJECT_PACKAGE_KIND) {
    const sourceProject = isRecord(parsed.project) ? parsed.project : parsed;
    const schemaVersion =
      typeof parsed.schemaVersion === 'number' ? parsed.schemaVersion : PROJECT_SCHEMA_VERSION;
    const migrationNotes =
      schemaVersion === PROJECT_SCHEMA_VERSION
        ? []
        : [`已从 schema v${schemaVersion} 迁移到 v${PROJECT_SCHEMA_VERSION}。`];
    const project = normalizeProjectRecord(
      sourceProject,
      normalizeString(sourceProject.name, '导入工程'),
      migrationNotes,
    );

    return {
      importedProjects: [project],
      migrationNotes: project.migrationNotes,
    };
  }

  if (parsed.kind === PROJECT_LIBRARY_KIND && Array.isArray(parsed.projects)) {
    const schemaVersion =
      typeof parsed.schemaVersion === 'number' ? parsed.schemaVersion : PROJECT_SCHEMA_VERSION;
    const migrationNotes =
      schemaVersion === PROJECT_SCHEMA_VERSION
        ? []
        : [`已从工程库 schema v${schemaVersion} 迁移到 v${PROJECT_SCHEMA_VERSION}。`];
    const projects = parsed.projects.map((item, index) =>
      normalizeProjectRecord(item, `导入工程 ${index + 1}`, migrationNotes),
    );

    return {
      importedProjects: projects,
      migrationNotes,
    };
  }

  if (typeof parsed.workflowAst === 'string') {
    const parsedGraph = parseWorkflowGraph(parsed.workflowAst);
    if (!parsedGraph.graph) {
      throw new Error(parsedGraph.error ?? '导入的工作流 AST 无法解析。');
    }

    const project = normalizeProjectRecord(
      {
        ...parsed,
        astText: formatWorkflowGraph(parsedGraph.graph),
      },
      normalizeString(parsed.name, '导入工程'),
      ['已从旧版工程包结构迁移到当前版本。'],
    );

    return {
      importedProjects: [project],
      migrationNotes: project.migrationNotes,
    };
  }

  throw new Error('暂不支持该导入文件格式。');
}

export function prepareProjectExport(project: ProjectRecord): {
  fileName: string;
  text: string;
} {
  const payload: ProjectPackage = {
    kind: PROJECT_PACKAGE_KIND,
    schemaVersion: PROJECT_SCHEMA_VERSION,
    exportedAt: nowIso(),
    project,
  };

  return {
    fileName: `${slugify(project.name)}.nazh-project.json`,
    text: JSON.stringify(payload, null, 2),
  };
}

export function mergeImportedProjects(
  existingProjects: ProjectRecord[],
  importedProjects: ProjectRecord[],
): ProjectRecord[] {
  let nextProjects = existingProjects.slice();

  importedProjects.forEach((project) => {
    const nextId = ensureUniqueProjectId(nextProjects, project.id || project.name);
    nextProjects = upsertProjectRecord(nextProjects, {
      ...project,
      id: nextId,
      updatedAt: nowIso(),
    });
  });

  return nextProjects.sort((left, right) => right.updatedAt.localeCompare(left.updatedAt));
}
