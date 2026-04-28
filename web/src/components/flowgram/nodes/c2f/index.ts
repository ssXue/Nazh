import { type NodeDefinition, type NodeSeed, type NodeValidationContext, type NodeValidation, normalizeNodeConfig } from '../shared';

export const definition: NodeDefinition = {
  kind: 'c2f',
  catalog: { category: '纯计算', description: '摄氏转华氏（pure-form，仅 Data 引脚）' },
  fallbackLabel: 'C→F',

  buildDefaultSeed(): NodeSeed {
    return {
      idPrefix: 'c2f',
      kind: 'c2f',
      label: '',
      timeoutMs: null,
      config: {},
    };
  },

  normalizeConfig(config: unknown): NodeSeed['config'] {
    return normalizeNodeConfig('c2f', config);
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
