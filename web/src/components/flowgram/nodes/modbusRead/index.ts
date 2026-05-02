import { type NodeDefinition, type NodeSeed, type NodeValidationContext, type NodeValidation, isRecord } from '../shared';
import { parsePositiveInteger, parseFiniteNumber } from '../settings-shared';

export const definition = {
  kind: 'modbusRead' as const,
  catalog: { category: '硬件接口', description: '读取 Modbus 寄存器并将遥测数据写入 payload' },
  fallbackLabel: 'Modbus Read',
  palette: { title: 'Modbus Read', badge: 'Modbus' },
  ai: {
    hint:
      'Modbus 读取；config 可含 unit_id, register, quantity, register_type, base_value, amplitude；通常不填写 connectionId。',
  },

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
      kind: 'modbusRead' as const,
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
    const rawConfig = isRecord(config) ? config : {};
    return {
      ...rawConfig,
      unit_id:
        typeof rawConfig.unit_id === 'number' && Number.isFinite(rawConfig.unit_id)
          ? Math.max(1, Math.round(rawConfig.unit_id))
          : 1,
      register:
        typeof rawConfig.register === 'number' && Number.isFinite(rawConfig.register)
          ? Math.max(1, Math.round(rawConfig.register))
          : 40001,
      quantity:
        typeof rawConfig.quantity === 'number' && Number.isFinite(rawConfig.quantity)
          ? Math.max(1, Math.round(rawConfig.quantity))
          : 1,
      register_type:
        typeof rawConfig.register_type === 'string' ? rawConfig.register_type : 'holding',
      base_value:
        typeof rawConfig.base_value === 'number' && Number.isFinite(rawConfig.base_value)
          ? rawConfig.base_value
          : 64,
      amplitude:
        typeof rawConfig.amplitude === 'number' && Number.isFinite(rawConfig.amplitude)
          ? rawConfig.amplitude
          : 6,
    };
  },

  getOutputPorts() {
    return [
      { key: 'out', label: 'out' },
      { key: 'latest', label: 'latest' },
    ];
  },

  getNodeSize() {
    return { width: 214, height: 132 };
  },

  buildRegistryMeta() {
    // ADR-0014 Phase 2 Task 7：modbusRead 加入 useDynamicPort 族，让
    // `out`（Exec）/ `latest`（Data）两个具名输出端口受控渲染，匹配 Rust
    // 端 pin id；pin-validator 的连接期 PinKind 校验由此真正生效。
    return {
      defaultExpanded: true,
      size: this.getNodeSize(),
      defaultPorts: [{ type: 'input' as const }],
      useDynamicPort: true,
    };
  },

  validate(_ctx: NodeValidationContext): NodeValidation[] {
    return [];
  },
} satisfies NodeDefinition;
