import {
  buildDefaultNodeSeed,
  getAllNodeDefinitions,
  normalizeNodeConfig,
  type NazhNodeKind,
} from '../components/flowgram/flowgram-node-library';
import type {
  DescribeNodePinsResponse,
  JsonValue,
  PinDefinition,
} from '../types';
import {
  NODE_CAPABILITY_LABELS,
  capabilityNames,
} from './node-capabilities';
import {
  describeNodePins,
  hasTauriRuntime,
  listNodeTypes,
} from './tauri';
import { formatPinType } from './pin-schema-cache';

export interface WorkflowAiNodeCapability {
  kind: NazhNodeKind;
  category: string;
  description: string;
  defaultConfig: JsonValue;
  aiVisible: boolean;
  editorOnly: boolean;
  runtimeCapabilities: string[];
  inputPins?: PinDefinition[];
  outputPins?: PinDefinition[];
}

export interface WorkflowAiNodeCatalog {
  nodes: WorkflowAiNodeCapability[];
}

const AI_HIDDEN_NODE_KINDS = new Set<NazhNodeKind>([
  'subgraphInput',
  'subgraphOutput',
]);

const AI_EDITOR_ONLY_NODE_KINDS = new Set<NazhNodeKind>(['subgraph']);

const NODE_AI_USAGE_HINTS: Partial<Record<NazhNodeKind, string>> = {
  timer: 'config 可含 interval_ms, immediate, inject。',
  native: 'config 可含 message，用于本地注入或透传。',
  code:
    'config 必须含 script；脚本输入变量是 payload，可用 ai_complete("prompt"), rand(min, max), now_ms(), from_json(text), to_json(value), is_blank(text)。',
  if: 'config 必须含 script；下游边 sourcePortId 只能是 true / false。',
  switch:
    'config 必须含 script 与 branches，branches 形如 [{key, label}]；下游边 sourcePortId 必须对应 branch key 或 default。',
  tryCatch: 'config 必须含 script；下游边 sourcePortId 只能是 try / catch。',
  loop: 'config 必须含 script；下游边 sourcePortId 只能是 body / done。',
  serialTrigger: '串口触发；通常不填写 connectionId，等待用户后续绑定。',
  modbusRead:
    'Modbus 读取；config 可含 unit_id, register, quantity, register_type, base_value, amplitude；通常不填写 connectionId。',
  mqttClient:
    'MQTT 发布或订阅；config.mode 为 publish 或 subscribe，通常不填写 connectionId。',
  httpClient:
    'HTTP/Webhook 发送；config 可含 body_mode, title_template, body_template；通常不填写 connectionId。',
  barkPush:
    'Bark 推送；config 可含 title_template, subtitle_template, body_template, level；通常不填写 connectionId。',
  sqlWriter: 'SQLite 写入；config 可含 database_path, table。',
  debugConsole: '调试输出；config 可含 label, pretty。',
  subgraph:
    '编辑器容器节点，不直接进入 Rust Runner。使用 upsert_subgraph 创建，blocks 内放普通业务节点，系统自动加入 subgraphInput/subgraphOutput 桥接节点并在部署前展平。',
};

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null && !Array.isArray(value);
}

function toJsonValue(value: unknown): JsonValue {
  if (
    value === null ||
    typeof value === 'string' ||
    typeof value === 'number' ||
    typeof value === 'boolean' ||
    Array.isArray(value)
  ) {
    return value as JsonValue;
  }

  if (typeof value === 'object') {
    return value as JsonValue;
  }

  return {};
}

function summarizePins(pins: PinDefinition[] | undefined): string {
  if (!pins || pins.length === 0) {
    return '';
  }

  return pins
    .map((pin) => {
      const typeLabel = formatPinType(pin.pin_type);
      return `${pin.id}: ${typeLabel}${pin.required ? ' (required)' : ''}`;
    })
    .join(', ');
}

function summarizeDefaultConfig(defaultConfig: JsonValue): string {
  if (!isRecord(defaultConfig)) {
    return '';
  }

  const keys = Object.keys(defaultConfig).sort();
  return keys.length > 0 ? `配置键 ${keys.join(', ')}` : '';
}

function normalizeRuntimePins(
  response: DescribeNodePinsResponse | null,
): Pick<WorkflowAiNodeCapability, 'inputPins' | 'outputPins'> {
  if (!response) {
    return {};
  }

  return {
    inputPins: response.inputPins,
    outputPins: response.outputPins,
  };
}

function buildLocalCatalog(): WorkflowAiNodeCatalog {
  return {
    nodes: getAllNodeDefinitions().map((definition) => {
      const defaultSeed = buildDefaultNodeSeed(definition.kind);
      const defaultConfig = normalizeNodeConfig(definition.kind, defaultSeed.config);

      return {
        kind: definition.kind,
        category: definition.catalog.category,
        description: definition.catalog.description,
        defaultConfig: toJsonValue(defaultConfig),
        aiVisible: !AI_HIDDEN_NODE_KINDS.has(definition.kind),
        editorOnly: AI_EDITOR_ONLY_NODE_KINDS.has(definition.kind),
        runtimeCapabilities: [],
      };
    }),
  };
}

async function loadRuntimeNodeCapabilities(
  catalog: WorkflowAiNodeCatalog,
): Promise<WorkflowAiNodeCatalog> {
  if (!hasTauriRuntime()) {
    return catalog;
  }

  try {
    const runtimeTypes = await listNodeTypes();
    const runtimeByName = new Map(
      runtimeTypes.types.map((entry) => [entry.name, entry.capabilities] as const),
    );

    const nodes = await Promise.all(
      catalog.nodes.map(async (node) => {
        const bits = runtimeByName.get(node.kind);
        const runtimeCapabilities =
          bits === undefined
            ? node.runtimeCapabilities
            : capabilityNames(bits).map((name) => NODE_CAPABILITY_LABELS[name]);

        if (bits === undefined) {
          return {
            ...node,
            runtimeCapabilities,
          };
        }

        try {
          const pins = await describeNodePins(
            node.kind,
            node.defaultConfig as Record<string, unknown>,
          );
          return {
            ...node,
            runtimeCapabilities,
            ...normalizeRuntimePins(pins),
          };
        } catch {
          return {
            ...node,
            runtimeCapabilities,
          };
        }
      }),
    );

    return { nodes };
  } catch {
    return catalog;
  }
}

let cachedCatalogPromise: Promise<WorkflowAiNodeCatalog> | null = null;

export function getLocalWorkflowAiNodeCatalog(): WorkflowAiNodeCatalog {
  return buildLocalCatalog();
}

export function getWorkflowAiAllowedNodeKinds(
  catalog: WorkflowAiNodeCatalog = getLocalWorkflowAiNodeCatalog(),
): NazhNodeKind[] {
  return catalog.nodes
    .filter((node) => node.aiVisible)
    .map((node) => node.kind);
}

export function normalizeWorkflowAiNodeKind(
  value: unknown,
  catalog: WorkflowAiNodeCatalog = getLocalWorkflowAiNodeCatalog(),
): NazhNodeKind | null {
  if (typeof value !== 'string') {
    return null;
  }

  const allowed = new Set(getWorkflowAiAllowedNodeKinds(catalog));
  return allowed.has(value as NazhNodeKind) ? (value as NazhNodeKind) : null;
}

export async function loadWorkflowAiNodeCatalog(): Promise<WorkflowAiNodeCatalog> {
  cachedCatalogPromise ??= loadRuntimeNodeCapabilities(buildLocalCatalog());
  return cachedCatalogPromise;
}

export function buildWorkflowAiNodeGuideText(
  catalog: WorkflowAiNodeCatalog = getLocalWorkflowAiNodeCatalog(),
): string {
  return catalog.nodes
    .filter((node) => node.aiVisible)
    .map((node) => {
      const sections = [
        `${node.kind}: ${node.category}；${node.description}`,
      ];
      const caps = node.runtimeCapabilities.join(', ');
      if (caps) {
        sections.push(`能力 ${caps}`);
      }
      const configKeys = summarizeDefaultConfig(node.defaultConfig);
      if (configKeys) {
        sections.push(configKeys);
      }
      const inputs = summarizePins(node.inputPins);
      const outputs = summarizePins(node.outputPins);
      if (inputs) {
        sections.push(`输入 [${inputs}]`);
      }
      if (outputs) {
        sections.push(`输出 [${outputs}]`);
      }
      const hint = NODE_AI_USAGE_HINTS[node.kind];
      if (hint) {
        sections.push(hint);
      }
      if (node.editorOnly) {
        sections.push('editorOnly=true');
      }
      return `- ${sections.join('；')}`;
    })
    .join('\n');
}
