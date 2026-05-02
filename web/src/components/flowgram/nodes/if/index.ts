import { type NodeDefinition, type NodeSeed, type NodeValidationContext, type NodeValidation, IF_BRANCHES, isRecord } from '../shared';

export const definition = {
  kind: 'if' as const,
  catalog: { category: '流程控制', description: '布尔条件分支路由' },
  fallbackLabel: 'IF Node',
  palette: { title: 'IF 条件', badge: 'IF' },
  ai: { hint: 'config 必须含 script；下游边 sourcePortId 只能是 true / false。' },

  buildDefaultSeed(): NodeSeed {
    return {
      idPrefix: 'if_node',
      kind: 'if' as const,
      label: '',
      timeoutMs: 1000,
      config: { script: 'payload["value"] > 0' },
    };
  },

  normalizeConfig(config: unknown): NodeSeed['config'] {
    const rawConfig = isRecord(config) ? config : {};
    return {
      ...rawConfig,
      script: typeof rawConfig.script === 'string' ? rawConfig.script : 'payload',
    };
  },

  getOutputPorts() {
    return IF_BRANCHES;
  },

  getRoutingBranches() {
    return IF_BRANCHES;
  },

  getNodeSize() {
    return { width: 240, height: 168 };
  },

  buildRegistryMeta() {
    return {
      defaultExpanded: true,
      size: this.getNodeSize(),
      defaultPorts: [{ type: 'input' as const }],
      useDynamicPort: true,
    };
  },

  validate(_ctx: NodeValidationContext): NodeValidation[] {
    return [];
  },
} satisfies NodeDefinition;
