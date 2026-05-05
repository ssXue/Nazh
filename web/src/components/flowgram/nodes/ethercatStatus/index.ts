import { type NodeDefinition, type NodeSeed, type NodeValidationContext, type NodeValidation } from '../shared';

export const definition = {
  kind: 'ethercatStatus' as const,
  catalog: { category: '硬件接口', description: '查询 EtherCAT 所有从站状态与通道信息' },
  fallbackLabel: 'EtherCAT Status',
  palette: { title: 'Status', badge: 'EtherCAT' },
  ai: {
    hint:
      'EtherCAT 从站状态查询节点；输出 slaves 列表和 channelInfo；需要绑定 ethercat / ethercat-soem / ecat 连接。',
  },
  requiresConnection: true,

  buildDefaultSeed(): NodeSeed {
    return {
      idPrefix: 'ecat_status',
      kind: 'ethercatStatus' as const,
      label: '',
      timeoutMs: 1000,
      config: {},
    };
  },

  normalizeConfig(config: unknown): NodeSeed['config'] {
    return { ...(typeof config === 'object' && config !== null && !Array.isArray(config) ? config : {}) };
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
