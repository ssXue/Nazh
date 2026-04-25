import { type NodeDefinition, type NodeSeed, normalizeNodeConfig } from '../shared';

export const definition: NodeDefinition = {
  kind: 'serialTrigger',
  catalog: { category: '硬件接口', description: '接收串口外设数据流并触发工作流' },
  fallbackLabel: 'Serial Trigger',

  buildDefaultSeed(): NodeSeed {
    return {
      idPrefix: 'serial_trigger',
      kind: 'serialTrigger',
      label: '',
      timeoutMs: null,
      config: { inject: {} },
    };
  },

  normalizeConfig(config: unknown): NodeSeed['config'] {
    return normalizeNodeConfig('serialTrigger', config);
  },

  getNodeSize() {
    return { width: 214, height: 132 };
  },

  buildRegistryMeta() {
    return { defaultExpanded: true, size: this.getNodeSize() };
  },
};
