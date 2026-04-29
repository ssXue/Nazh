import { type NodeDefinition, type NodeSeed, type NodeValidationContext, type NodeValidation, normalizeNodeConfig } from '../shared';

const LOOKUP_DEFAULT_CONFIG = { table: {}, default: null } as unknown as NodeSeed['config'];

export const definition: NodeDefinition = {
  kind: 'lookup',
  catalog: { category: '纯计算', description: '配置驱动表查找（pure-form，输入 key 标量 → 查表 → 输出 value）' },
  fallbackLabel: '表查找',

  buildDefaultSeed(): NodeSeed {
    return {
      idPrefix: 'lookup',
      kind: 'lookup',
      label: '',
      timeoutMs: null,
      config: LOOKUP_DEFAULT_CONFIG,
    };
  },

  normalizeConfig(config: unknown): NodeSeed['config'] {
    return normalizeNodeConfig('lookup', config);
  },

  getNodeSize() {
    return { width: 180, height: 100 };
  },

  buildRegistryMeta() {
    return { defaultExpanded: true, size: this.getNodeSize() };
  },

  validate(_ctx: NodeValidationContext): NodeValidation[] {
    return [];
  },
};
