import { type NodeDefinition, type NodeSeed, normalizeNodeConfig } from '../shared';

export const definition: NodeDefinition = {
  kind: 'loop',
  catalog: { category: '流程控制', description: '循环迭代与逐项分发' },
  fallbackLabel: 'Loop Node',

  buildDefaultSeed(): NodeSeed {
    return {
      idPrefix: 'loop_node',
      kind: 'loop',
      label: '',
      timeoutMs: 1000,
      config: { script: '[payload]' },
    };
  },

  normalizeConfig(config: unknown): NodeSeed['config'] {
    return normalizeNodeConfig('loop', config);
  },

  getNodeSize() {
    return { width: 244, height: 176 };
  },

  buildRegistryMeta() {
    return {
      defaultExpanded: true,
      size: this.getNodeSize(),
      defaultPorts: [{ type: 'input' as const }],
      useDynamicPort: true,
    };
  },
};
