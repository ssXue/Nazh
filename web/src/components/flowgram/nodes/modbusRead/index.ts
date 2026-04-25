import { type NodeDefinition, type NodeSeed, normalizeNodeConfig } from '../shared';

export const definition: NodeDefinition = {
  kind: 'modbusRead',
  catalog: { category: '硬件接口', description: '读取 Modbus 寄存器并将遥测数据写入 payload' },
  fallbackLabel: 'Modbus Read',

  buildDefaultSeed(): NodeSeed {
    return {
      idPrefix: 'modbus_read',
      kind: 'modbusRead',
      label: '',
      timeoutMs: 1000,
      config: {
        unit_id: 1,
        register: 40001,
        quantity: 1,
        register_type: 'holding',
        base_value: 64,
        amplitude: 6,
      },
    };
  },

  normalizeConfig(config: unknown): NodeSeed['config'] {
    return normalizeNodeConfig('modbusRead', config);
  },

  getNodeSize() {
    return { width: 214, height: 132 };
  },

  buildRegistryMeta() {
    return { defaultExpanded: true, size: this.getNodeSize() };
  },
};
