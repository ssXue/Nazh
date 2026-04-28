import { type NodeDefinition, type NodeSeed, type NodeValidationContext, type NodeValidation, normalizeNodeConfig } from '../shared';

export const definition: NodeDefinition = {
  kind: 'minutesSince',
  catalog: { category: '纯计算', description: '给定 RFC3339 时间戳返回距今分钟数（pure-form，仅 Data 引脚）' },
  fallbackLabel: '距今分钟',

  buildDefaultSeed(): NodeSeed {
    return {
      idPrefix: 'minutes_since',
      kind: 'minutesSince',
      label: '',
      timeoutMs: null,
      config: {},
    };
  },

  normalizeConfig(config: unknown): NodeSeed['config'] {
    return normalizeNodeConfig('minutesSince', config);
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
