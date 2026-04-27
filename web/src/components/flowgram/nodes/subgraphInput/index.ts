import {
  type NodeDefinition,
  type NodeSeed,
  type NodeValidationContext,
  type NodeValidation,
  normalizeNodeConfig,
} from '../shared';

export const definition: NodeDefinition = {
  kind: 'subgraphInput',
  catalog: {
    category: '子图封装',
    description: '子图内部桥接入口',
  },
  fallbackLabel: 'Input',

  buildDefaultSeed(): NodeSeed {
    return {
      idPrefix: 'sg_in',
      kind: 'subgraphInput',
      label: '',
      timeoutMs: null,
      config: {},
    };
  },

  normalizeConfig(config: unknown): NodeSeed['config'] {
    return normalizeNodeConfig('subgraphInput', config);
  },

  getNodeSize() {
    return { width: 120, height: 80 };
  },

  buildRegistryMeta() {
    return {
      defaultExpanded: true,
      size: this.getNodeSize(),
      defaultPorts: [{ type: 'output' as const }],
    };
  },

  validate(_ctx: NodeValidationContext): NodeValidation[] {
    return [];
  },
};
