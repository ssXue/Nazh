import { type NodeDefinition, type NodeSeed, type NodeValidationContext, type NodeValidation, isRecord } from '../shared';

export const definition = {
  kind: 'native' as const,
  catalog: { category: '数据注入', description: '打印 payload 元数据，可选附加连接上下文' },
  fallbackLabel: 'Native Node',
  palette: { title: 'Native', badge: 'Native' },
  ai: { hint: 'config 可含 message，用于本地注入或透传。' },

  fieldValidators: {
    message: v => !v.trim() ? { message: '消息内容为空。', tone: 'warning' } : null,
  },

  buildDefaultSeed(): NodeSeed {
    return {
      idPrefix: 'native_node',
      kind: 'native' as const,
      label: '',
      timeoutMs: null,
      config: { message: 'New native node' },
    };
  },

  normalizeConfig(config: unknown): NodeSeed['config'] {
    const rawConfig = isRecord(config) ? config : {};
    return {
      ...rawConfig,
      message: typeof rawConfig.message === 'string' ? rawConfig.message : '',
    };
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
} satisfies NodeDefinition;
