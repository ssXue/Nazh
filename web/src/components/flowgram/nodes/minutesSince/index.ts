import { type NodeDefinition, type NodeSeed, type NodeValidationContext, type NodeValidation, isRecord } from '../shared';

export const definition = {
  kind: 'minutesSince' as const,
  catalog: { category: '纯计算', description: '给定 RFC3339 时间戳返回距今分钟数（pure-form，仅 Data 引脚）' },
  fallbackLabel: '距今分钟',
  palette: { title: '距今分钟', badge: '分钟' },
  ai: { hint: '纯计算节点；输入 RFC3339 时间戳，输出距今分钟数。' },

  buildDefaultSeed(): NodeSeed {
    return {
      idPrefix: 'minutes_since',
      kind: 'minutesSince' as const,
      label: '',
      timeoutMs: null,
      config: {},
    };
  },

  normalizeConfig(config: unknown): NodeSeed['config'] {
    const rawConfig = isRecord(config) ? config : {};
    return { ...rawConfig };
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
} satisfies NodeDefinition;
