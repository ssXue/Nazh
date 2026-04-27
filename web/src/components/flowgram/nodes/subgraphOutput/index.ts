import {
  type NodeDefinition,
  type NodeSeed,
  type NodeValidationContext,
  type NodeValidation,
  normalizeNodeConfig,
} from '../shared';

export const definition: NodeDefinition = {
  kind: 'subgraphOutput',
  catalog: {
    category: '子图封装',
    description: '子图内部桥接出口',
  },
  fallbackLabel: 'Output',

  buildDefaultSeed(): NodeSeed {
    return {
      idPrefix: 'sg_out',
      kind: 'subgraphOutput',
      label: '',
      timeoutMs: null,
      config: {},
    };
  },

  normalizeConfig(config: unknown): NodeSeed['config'] {
    return normalizeNodeConfig('subgraphOutput', config);
  },

  getNodeSize() {
    return { width: 120, height: 80 };
  },

  buildRegistryMeta() {
    return {
      defaultExpanded: true,
      size: this.getNodeSize(),
      defaultPorts: [{ type: 'input' as const }],
    };
  },

  validate(_ctx: NodeValidationContext): NodeValidation[] {
    return [];
  },
};
