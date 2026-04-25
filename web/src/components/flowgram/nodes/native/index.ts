import { type NodeDefinition, type NodeSeed, normalizeNodeConfig } from '../shared';

export const definition: NodeDefinition = {
  kind: 'native',
  catalog: { category: '数据注入', description: '打印 payload 元数据，可选附加连接上下文' },
  fallbackLabel: 'Native Node',

  buildDefaultSeed(): NodeSeed {
    return {
      idPrefix: 'native_node',
      kind: 'native',
      label: '',
      timeoutMs: null,
      config: { message: 'New native node' },
    };
  },

  normalizeConfig(config: unknown): NodeSeed['config'] {
    return normalizeNodeConfig('native', config);
  },

  getNodeSize() {
    return { width: 214, height: 132 };
  },

  buildRegistryMeta() {
    return { defaultExpanded: true, size: this.getNodeSize() };
  },
};
