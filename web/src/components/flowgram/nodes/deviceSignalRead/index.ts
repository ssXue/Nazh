import type { NodeDefinition, NodeSeed, NodeValidation, NodeValidationContext } from '../shared';
import { isRecord, STANDARD_NODE_SIZE } from '../shared';

export const definition = {
  kind: 'deviceSignalRead' as const,
  catalog: {
    category: '设备能力',
    description: '按信号源定义读取设备原始数据，经解码和 scale 缩放后输出语义化值',
  },
  fallbackLabel: 'Device Signal Read',
  palette: { title: 'Signal Read', badge: 'Signal' },
  ai: {
    hint: '设备信号读取；config 可含 device_id, signal_id, source (SignalSourceSnapshot), scale, unit, simulation, poll_timeout_ms；需绑定连接。',
  },
  requiresConnection: true,
  buildDefaultSeed(): NodeSeed {
    return {
      idPrefix: 'device_signal_read',
      kind: 'deviceSignalRead' as const,
      label: '',
      timeoutMs: 2000,
      config: {
        device_id: '',
        signal_id: '',
        source: {
          type: 'register',
          register: 40001,
          data_type: 'float32',
        },
        simulation: true,
        poll_timeout_ms: 2000,
      },
    };
  },
  normalizeConfig(config: unknown): NodeSeed['config'] {
    const rawConfig = isRecord(config) ? config : {};
    return {
      ...rawConfig,
      device_id: typeof rawConfig.device_id === 'string' ? rawConfig.device_id : '',
      signal_id: typeof rawConfig.signal_id === 'string' ? rawConfig.signal_id : '',
      simulation: rawConfig.simulation === true,
      poll_timeout_ms:
        typeof rawConfig.poll_timeout_ms === 'number' && Number.isFinite(rawConfig.poll_timeout_ms)
          ? Math.max(100, Math.round(rawConfig.poll_timeout_ms))
          : 2000,
    };
  },
  getOutputPorts() {
    return [
      { key: 'out', label: 'out' },
      { key: 'latest', label: 'latest' },
    ];
  },
  getNodeSize() {
    return STANDARD_NODE_SIZE;
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
