import { type NodeDefinition, type NodeSeed, type NodeValidationContext, type NodeValidation, isRecord } from '../shared';

const LOOKUP_DEFAULT_CONFIG = { table: {}, default: null } as unknown as NodeSeed['config'];

export const definition = {
  kind: 'lookup' as const,
  catalog: { category: '纯计算', description: '配置驱动表查找（pure-form，输入 key 标量 → 查表 → 输出 value）' },
  fallbackLabel: '表查找',
  palette: { title: '表查找', badge: '查找' },
  ai: { hint: '纯计算节点；config 可含 table 与 default。' },

  buildDefaultSeed(): NodeSeed {
    return {
      idPrefix: 'lookup',
      kind: 'lookup' as const,
      label: '',
      timeoutMs: null,
      config: LOOKUP_DEFAULT_CONFIG,
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
