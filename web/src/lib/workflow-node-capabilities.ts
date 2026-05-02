import {
  getAllNodeDefinitions,
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
  usageHint?: string;
  runtimeCapabilities: string[];
  inputPins?: PinDefinition[];
  outputPins?: PinDefinition[];
}

export interface WorkflowAiNodeCatalog {
  nodes: WorkflowAiNodeCapability[];
}

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
      const defaultSeed = definition.buildDefaultSeed();
      const defaultConfig = definition.normalizeConfig(defaultSeed.config);

      return {
        kind: definition.kind,
        category: definition.catalog.category,
        description: definition.catalog.description,
        defaultConfig: toJsonValue(defaultConfig),
        aiVisible: definition.ai?.visible !== false,
        editorOnly: definition.ai?.editorOnly === true,
        ...(definition.ai?.hint ? { usageHint: definition.ai.hint } : {}),
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
      if (node.usageHint) {
        sections.push(node.usageHint);
      }
      if (node.editorOnly) {
        sections.push('editorOnly=true');
      }
      return `- ${sections.join('；')}`;
    })
    .join('\n');
}
