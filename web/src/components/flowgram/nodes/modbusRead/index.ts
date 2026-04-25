import { type NodeDefinition, type NodeSeed, type NodeValidationContext, type NodeValidation, normalizeNodeConfig } from '../shared';
import { parsePositiveInteger, parseFiniteNumber } from '../settings-shared';

export const definition: NodeDefinition = {
  kind: 'modbusRead',
  catalog: { category: '硬件接口', description: '读取 Modbus 寄存器并将遥测数据写入 payload' },
  fallbackLabel: 'Modbus Read',

  fieldValidators: {
    modbusUnitId: v => parsePositiveInteger(v) === null ? 'Modbus Unit ID 必须是大于 0 的整数。' : null,
    modbusRegister: v => parsePositiveInteger(v) === null ? 'Modbus 寄存器地址必须是大于 0 的整数。' : null,
    modbusQuantity: v => parsePositiveInteger(v) === null ? 'Modbus 读取数量必须是大于 0 的整数。' : null,
    modbusBaseValue: v => parseFiniteNumber(v) === null ? 'Modbus 基准值必须是有效数字。' : null,
    modbusAmplitude: v => parseFiniteNumber(v) === null ? 'Modbus 振幅必须是有效数字。' : null,
  },

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

  validate(_ctx: NodeValidationContext): NodeValidation[] {
    return [];
  },
};
