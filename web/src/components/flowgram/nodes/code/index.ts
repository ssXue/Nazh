import { type NodeDefinition, type NodeSeed, normalizeNodeConfig } from '../shared';

export const definition: NodeDefinition = {
  kind: 'code',
  catalog: { category: '脚本执行', description: '沙箱化脚本执行节点' },
  fallbackLabel: 'Code Node',

  buildDefaultSeed(): NodeSeed {
    return {
      idPrefix: 'code_node',
      kind: 'code',
      label: '',
      timeoutMs: 1000,
      config: { script: 'payload' },
    };
  },

  normalizeConfig(config: unknown): NodeSeed['config'] {
    return normalizeNodeConfig('code', config);
  },

  getNodeSize() {
    return { width: 214, height: 132 };
  },

  buildRegistryMeta() {
    return { defaultExpanded: true, size: this.getNodeSize() };
  },
};
