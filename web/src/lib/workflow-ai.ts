import type {
  AiConfigView,
  AiProviderView,
  JsonValue,
  WorkflowGraph,
  WorkflowNodeDefinition,
} from '../types';

export interface GlobalScriptAiConfig {
  providerId: string;
  model?: string;
  systemPrompt?: string;
  temperature?: number;
  maxTokens?: number;
  topP?: number;
  timeoutMs?: number;
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null && !Array.isArray(value);
}

function toFiniteNumber(value: number | bigint | undefined | null): number | undefined {
  if (typeof value === 'number') {
    return Number.isFinite(value) ? value : undefined;
  }

  if (typeof value === 'bigint') {
    const nextValue = Number(value);
    return Number.isFinite(nextValue) ? nextValue : undefined;
  }

  return undefined;
}

export function isAiCapableScriptNode(nodeType: string): boolean {
  return nodeType === 'code';
}

export function isUsableGlobalAiProvider(
  provider: AiProviderView | null | undefined,
): provider is AiProviderView {
  return Boolean(provider?.enabled && provider.hasApiKey);
}

export function resolveGlobalAiProvider(aiConfig: AiConfigView | null): AiProviderView | null {
  if (!aiConfig || aiConfig.providers.length === 0) {
    return null;
  }

  const activeProvider = aiConfig.activeProviderId
    ? aiConfig.providers.find((provider) => provider.id === aiConfig.activeProviderId) ?? null
    : null;

  if (activeProvider) {
    return activeProvider;
  }

  return aiConfig.providers.find((provider) => provider.enabled) ?? aiConfig.providers[0] ?? null;
}

export function stripNodeLocalAiConfig(
  nodeType: string,
  config: WorkflowNodeDefinition['config'] | undefined,
): WorkflowNodeDefinition['config'] {
  if (!isAiCapableScriptNode(nodeType) || !isRecord(config)) {
    return config ?? {};
  }

  const { ai: _removedAi, ...restConfig } = config;
  return restConfig as JsonValue;
}

export function stripWorkflowNodeLocalAiConfig(graph: WorkflowGraph): WorkflowGraph {
  const nextNodes = Object.fromEntries(
    Object.entries(graph.nodes).map(([nodeId, node]) => [
      nodeId,
      {
        ...node,
        config: stripNodeLocalAiConfig(node.type, node.config),
      },
    ]),
  ) as WorkflowGraph['nodes'];

  return {
    ...graph,
    nodes: nextNodes,
  };
}

export function buildGlobalScriptAiConfig(
  aiConfig: AiConfigView | null,
): GlobalScriptAiConfig | null {
  const provider = resolveGlobalAiProvider(aiConfig);
  if (!provider) {
    return null;
  }

  const nextConfig: GlobalScriptAiConfig = {
    providerId: provider.id,
  };

  if (provider.defaultModel.trim()) {
    nextConfig.model = provider.defaultModel.trim();
  }

  if (aiConfig?.agentSettings.systemPrompt?.trim()) {
    nextConfig.systemPrompt = aiConfig.agentSettings.systemPrompt.trim();
  }

  const temperature = toFiniteNumber(aiConfig?.copilotParams.temperature);
  if (temperature !== undefined) {
    nextConfig.temperature = temperature;
  }

  const maxTokens = toFiniteNumber(aiConfig?.copilotParams.maxTokens);
  if (maxTokens !== undefined) {
    nextConfig.maxTokens = maxTokens;
  }

  const topP = toFiniteNumber(aiConfig?.copilotParams.topP);
  if (topP !== undefined) {
    nextConfig.topP = topP;
  }

  const timeoutMs = toFiniteNumber(aiConfig?.agentSettings.timeoutMs);
  if (timeoutMs !== undefined) {
    nextConfig.timeoutMs = timeoutMs;
  }

  return nextConfig;
}

export function applyGlobalAiConfigToWorkflowGraph(
  graph: WorkflowGraph,
  aiConfig: AiConfigView | null,
): WorkflowGraph {
  const strippedGraph = stripWorkflowNodeLocalAiConfig(graph);
  const globalScriptAiConfig = buildGlobalScriptAiConfig(aiConfig);
  if (!globalScriptAiConfig) {
    return strippedGraph;
  }

  const nextNodes = Object.fromEntries(
    Object.entries(strippedGraph.nodes).map(([nodeId, node]) => {
      if (!isAiCapableScriptNode(node.type)) {
        return [nodeId, node];
      }

      const configRecord = isRecord(node.config) ? node.config : {};

      return [
        nodeId,
        {
          ...node,
          config: {
            ...configRecord,
            ai: globalScriptAiConfig,
          },
        },
      ];
    }),
  ) as WorkflowGraph['nodes'];

  return {
    ...strippedGraph,
    nodes: nextNodes,
  };
}
