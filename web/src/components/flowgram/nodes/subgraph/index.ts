import {
  type NodeDefinition,
  type NodeSeed,
  type NodeValidationContext,
  type NodeValidation,
  normalizeNodeConfig,
} from '../shared';

export const definition: NodeDefinition = {
  kind: 'subgraph',
  catalog: {
    category: '子图封装',
    description: '封装子拓扑为单节点，支持嵌套和参数化',
  },
  fallbackLabel: 'Subgraph',

  buildDefaultSeed(): NodeSeed {
    return {
      idPrefix: 'subgraph',
      kind: 'subgraph',
      label: '',
      timeoutMs: null,
      config: {
        parameterBindings: {},
      },
    };
  },

  normalizeConfig(config: unknown): NodeSeed['config'] {
    return normalizeNodeConfig('subgraph', config);
  },

  getNodeSize() {
    return { width: 560, height: 400 };
  },

  buildRegistryMeta() {
    return {
      defaultExpanded: true,
      size: this.getNodeSize(),
      defaultPorts: [{ type: 'input' as const }, { type: 'output' as const }],
    };
  },

  validate(_ctx: NodeValidationContext): NodeValidation[] {
    return [];
  },
};
