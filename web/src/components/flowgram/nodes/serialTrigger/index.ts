import { type NodeDefinition, type NodeSeed, type NodeValidationContext, type NodeValidation, isRecord } from '../shared';

export const definition = {
  kind: 'serialTrigger' as const,
  catalog: { category: '硬件接口', description: '接收串口外设数据流并触发工作流' },
  fallbackLabel: 'Serial Trigger',
  palette: { title: 'Serial Trigger', badge: 'Serial' },
  ai: { hint: '串口触发；通常不填写 connectionId，等待用户后续绑定。' },
  requiresConnection: true,

  buildDefaultSeed(): NodeSeed {
    return {
      idPrefix: 'serial_trigger',
      kind: 'serialTrigger' as const,
      label: '',
      timeoutMs: null,
      config: { inject: {} },
    };
  },

  normalizeConfig(config: unknown): NodeSeed['config'] {
    const rawConfig = isRecord(config) ? config : {};
    return {
      inject: isRecord(rawConfig.inject) ? rawConfig.inject : {},
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
