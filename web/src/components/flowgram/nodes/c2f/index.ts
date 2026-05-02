import { type NodeDefinition, type NodeSeed, type NodeValidationContext, type NodeValidation, isRecord } from '../shared';

export const definition = {
  kind: 'c2f' as const,
  catalog: { category: '纯计算', description: '摄氏转华氏（pure-form，仅 Data 引脚）' },
  fallbackLabel: 'C→F',
  palette: { title: 'C→F 转换', badge: 'C→F' },
  ai: { hint: '纯计算节点；输入摄氏度，输出华氏度。' },

  buildDefaultSeed(): NodeSeed {
    return {
      idPrefix: 'c2f',
      kind: 'c2f' as const,
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
