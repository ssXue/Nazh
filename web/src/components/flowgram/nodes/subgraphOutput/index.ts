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
};
