import { type NodeDefinition, type NodeSeed, type NodeValidationContext, type NodeValidation, isRecord } from '../shared';
import { parsePositiveInteger } from '../settings-shared';

export const definition = {
  kind: 'ethercatPdoWrite' as const,
  catalog: { category: '硬件接口', description: '写入 EtherCAT 从站 PDO 输出数据' },
  fallbackLabel: 'EtherCAT PDO Write',
  palette: { title: 'PDO Write', badge: 'EtherCAT' },
  ai: {
    hint:
      'EtherCAT PDO 写入节点；config 可含 slave_address；payload 需提供 data 数组（[0x01, 0x02, ...]）；需要绑定 ethercat / ethercat-soem / ecat 连接。',
  },
  requiresConnection: true,

  fieldValidators: {
    ethercatSlaveAddress: v =>
      v.trim() && parsePositiveInteger(v) === null ? '从站地址必须是大于 0 的整数。' : null,
  },

  buildDefaultSeed(): NodeSeed {
    return {
      idPrefix: 'ecat_pdo_write',
      kind: 'ethercatPdoWrite' as const,
      label: '',
      timeoutMs: 1000,
      config: {
        slave_address: 1,
      },
    };
  },

  normalizeConfig(config: unknown): NodeSeed['config'] {
    const rawConfig = isRecord(config) ? config : {};
    return {
      ...rawConfig,
      slave_address:
        typeof rawConfig.slave_address === 'number' && Number.isFinite(rawConfig.slave_address)
          ? Math.max(1, Math.round(rawConfig.slave_address))
          : 1,
    };
  },

  getOutputPorts() {
    return [
      { key: 'out', label: 'out' },
    ];
  },

  getNodeSize() {
    return { width: 200, height: 100 };
  },

  buildRegistryMeta() {
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
