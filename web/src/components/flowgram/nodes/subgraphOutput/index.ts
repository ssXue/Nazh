import {
  type NodeDefinition,
  type NodeSeed,
  type NodeValidationContext,
  type NodeValidation,
  isRecord,
} from '../shared';

export const definition = {
  kind: 'subgraphOutput' as const,
  catalog: {
    category: '子图封装',
    description: '子图内部桥接出口',
  },
  fallbackLabel: 'Output',
  palette: { visible: false },
  ai: { visible: false },

  buildDefaultSeed(): NodeSeed {
    return {
      idPrefix: 'sg_out',
      kind: 'subgraphOutput' as const,
      label: '',
      timeoutMs: null,
      config: {},
    };
  },

  normalizeConfig(config: unknown): NodeSeed['config'] {
    const rawConfig = isRecord(config) ? config : {};
    return { ...rawConfig };
  },

  getNodeSize() {
    return { width: 48, height: 48 };
  },

  buildRegistryMeta() {
    return {
      defaultExpanded: true,
      deleteDisable: true,
      copyDisable: true,
      size: this.getNodeSize(),
      defaultPorts: [{ type: 'input' as const }],
    };
  },

  validate(_ctx: NodeValidationContext): NodeValidation[] {
    return [];
  },
} satisfies NodeDefinition;
