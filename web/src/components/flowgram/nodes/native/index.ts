import { type NodeDefinition, type NodeSeed, type NodeValidationContext, type NodeValidation, normalizeNodeConfig } from '../shared';

export const definition: NodeDefinition = {
  kind: 'native',
  catalog: { category: '数据注入', description: '打印 payload 元数据，可选附加连接上下文' },
  fallbackLabel: 'Native Node',

  fieldValidators: {
    message: v => !v.trim() ? { message: '消息内容为空。', tone: 'warning' } : null,
  },

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

  validate(_ctx: NodeValidationContext): NodeValidation[] {
    return [];
  },
};
