import type {
  FlowNodeJSON,
  WorkflowNodeJSON,
  WorkflowNodeRegistry,
} from '@flowgram.ai/free-layout-editor';

export type NazhNodeKind = 'native' | 'rhai';

export interface NodeSeed {
  idPrefix: string;
  kind: NazhNodeKind;
  label: string;
  connectionId?: string | null;
  aiDescription?: string | null;
  timeoutMs?: number | null;
  config: {
    message?: string;
    script?: string;
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

interface FlowgramNodeData {
  label?: string;
  nodeType?: string;
  connectionId?: string | null;
  aiDescription?: string | null;
  timeoutMs?: number | null;
  config?: {
    message?: string;
    script?: string;
    [key: string]: unknown;
  };
}

const FLOWGRAM_NODE_SIZE = {
  width: 214,
  height: 132,
} as const;

const NODE_TEMPLATES: FlowgramPaletteItem[] = [
  {
    key: 'plc-ingest',
    title: 'PLC 采集',
    description: '工业连接入口。',
    badge: 'Native',
    seed: {
      idPrefix: 'plc_ingest',
      kind: 'native',
      label: 'PLC Ingest',
      aiDescription: 'Collect industrial frames from the configured device connection.',
      timeoutMs: null,
      config: {
        message: 'PLC frame received',
      },
    },
  },
  {
    key: 'normalize',
    title: '标准化',
    description: '统一字段结构。',
    badge: 'Rhai',
    seed: {
      idPrefix: 'normalize_payload',
      kind: 'rhai',
      label: 'Normalize Payload',
      connectionId: null,
      aiDescription: 'Normalize raw values into a stable payload schema.',
      timeoutMs: 1000,
      config: {
        script:
          'payload["normalized"] = true; payload["status"] = payload["value"] > 80 ? "alert" : "nominal"; payload',
      },
    },
  },
  {
    key: 'alarm',
    title: '阈值告警',
    description: '快速生成告警标记。',
    badge: 'Rhai',
    seed: {
      idPrefix: 'threshold_alarm',
      kind: 'rhai',
      label: 'Threshold Alarm',
      connectionId: null,
      aiDescription: 'Apply threshold rules and enrich the payload with alarm state.',
      timeoutMs: 1000,
      config: {
        script:
          'let value = payload["value"]; payload["alarm"] = value > 80; payload["severity"] = value > 95 ? "high" : "normal"; payload',
      },
    },
  },
  {
    key: 'publish',
    title: '结果发布',
    description: '流程末端出口。',
    badge: 'Native',
    seed: {
      idPrefix: 'publish_result',
      kind: 'native',
      label: 'Publish Result',
      aiDescription: 'Ship the processed payload to an external system or sink.',
      timeoutMs: 1500,
      config: {
        message: 'Publish workflow result',
      },
    },
  },
];

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null && !Array.isArray(value);
}

function normalizeNodeKind(value: unknown): NazhNodeKind {
  return value === 'rhai' ? 'rhai' : 'native';
}

function normalizeTimeoutValue(value: unknown): number | null {
  if (typeof value !== 'number' || !Number.isFinite(value) || value <= 0) {
    return null;
  }

  return value;
}

export function parseTimeoutMs(value: string): number | null {
  const normalized = value.trim();
  if (!normalized) {
    return null;
  }

  const numeric = Number(normalized);
  return normalizeTimeoutValue(numeric);
}

export function buildDefaultNodeSeed(kind: NazhNodeKind): NodeSeed {
  if (kind === 'native') {
    return {
      idPrefix: 'native_node',
      kind: 'native',
      label: '',
      aiDescription: 'Native node for protocol IO and resource-bound operations.',
      timeoutMs: null,
      config: {
        message: 'New native node',
      },
    };
  }

  return {
    idPrefix: 'rhai_node',
    kind: 'rhai',
    label: '',
    connectionId: null,
    aiDescription: 'Rhai node for dynamic business logic.',
    timeoutMs: 1000,
    config: {
      script: 'payload',
    },
  };
}

export function resolveNodeData(
  seed: NodeSeed,
  fallbackLabel: string,
  primaryConnectionId: string | null,
): Required<FlowgramNodeData> {
  const connectionId =
    seed.connectionId === undefined
      ? seed.kind === 'native'
        ? primaryConnectionId
        : null
      : seed.connectionId;
  const label = seed.label.trim() || fallbackLabel;

  if (seed.kind === 'native') {
    return {
      label,
      nodeType: 'native',
      connectionId,
      aiDescription: seed.aiDescription ?? null,
      timeoutMs: seed.timeoutMs ?? null,
      config: {
        ...seed.config,
        message: typeof seed.config.message === 'string' ? seed.config.message : '',
      },
    };
  }

  return {
    label,
    nodeType: 'rhai',
    connectionId,
    aiDescription: seed.aiDescription ?? null,
    timeoutMs: seed.timeoutMs ?? null,
    config: {
      ...seed.config,
      script: typeof seed.config.script === 'string' ? seed.config.script : 'payload',
    },
  };
}

export function buildPaletteNodeJson(
  seed: NodeSeed,
  primaryConnectionId: string | null,
  baseJson?: Partial<WorkflowNodeJSON>,
): Partial<WorkflowNodeJSON> {
  const fallbackLabel =
    seed.label.trim() ||
    (seed.kind === 'native' ? 'Native Node' : 'Rhai Node');
  const baseData = isRecord(baseJson?.data) ? (baseJson.data as Record<string, unknown>) : {};
  const nextData = resolveNodeData(seed, fallbackLabel, primaryConnectionId);

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
  primaryConnectionId: string | null,
): FlowNodeJSON {
  const rawData = isRecord(json.data) ? (json.data as FlowgramNodeData) : {};
  const nodeType = normalizeNodeKind(rawData.nodeType ?? json.type);
  const rawConfig = isRecord(rawData.config) ? rawData.config : {};
  const fallbackLabel = json.id || (nodeType === 'native' ? 'Native Node' : 'Rhai Node');

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
      connectionId:
        rawData.connectionId === undefined
          ? nodeType === 'native'
            ? primaryConnectionId
            : null
          : rawData.connectionId ?? null,
      aiDescription:
        typeof rawData.aiDescription === 'string' && rawData.aiDescription.trim()
          ? rawData.aiDescription
          : null,
      timeoutMs: normalizeTimeoutValue(rawData.timeoutMs),
      config:
        nodeType === 'native'
          ? {
              ...rawConfig,
              message: typeof rawConfig.message === 'string' ? rawConfig.message : '',
            }
          : {
              ...rawConfig,
              script: typeof rawConfig.script === 'string' ? rawConfig.script : 'payload',
            },
    },
  };
}

export function createFlowgramNodeRegistries(
  primaryConnectionId: string | null,
): WorkflowNodeRegistry[] {
  return ['native', 'rhai'].map((kind) => ({
    type: kind,
    meta: {
      defaultExpanded: true,
      size: FLOWGRAM_NODE_SIZE,
    },
    onAdd: () => buildPaletteNodeJson(buildDefaultNodeSeed(kind as NazhNodeKind), primaryConnectionId),
  }));
}

export function getDefaultFlowgramNodeRegistry(type: string): WorkflowNodeRegistry {
  return {
    type,
    meta: {
      defaultExpanded: true,
      size: FLOWGRAM_NODE_SIZE,
    },
  };
}

export function getFlowgramPaletteSections(): FlowgramPaletteSection[] {
  return [
    {
      key: 'blank',
      title: '基础节点',
      items: [
        {
          key: 'blank-native',
          title: '空白 Native',
          description: '协议接入与资源调用。',
          badge: 'Native',
          seed: buildDefaultNodeSeed('native'),
        },
        {
          key: 'blank-rhai',
          title: '空白 Rhai',
          description: '脚本逻辑与数据变换。',
          badge: 'Rhai',
          seed: buildDefaultNodeSeed('rhai'),
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
