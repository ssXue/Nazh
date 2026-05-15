import type { NodeDefinition, NodeSeed, NodeValidation, NodeValidationContext } from '../shared';
import { isRecord, STANDARD_NODE_SIZE } from '../shared';

export const definition = {
  kind: 'deviceEventTrigger' as const,
  catalog: {
    category: '设备能力',
    description: '监听设备事件（MQTT/CAN/Modbus 轮询/串口帧），经解码和 scale 缩放后触发 DAG',
  },
  fallbackLabel: 'Device Event Trigger',
  palette: { title: 'Event Trigger', badge: 'Event' },
  ai: {
    hint: '设备事件触发；config 可含 device_id, signals (SignalListenerSnapshot[]), simulation, poll_interval_ms；需绑定连接。',
  },
  requiresConnection: true,
  buildDefaultSeed(): NodeSeed {
    return {
      idPrefix: 'device_event_trigger',
      kind: 'deviceEventTrigger' as const,
      label: '',
      timeoutMs: null,
      config: {
        device_id: '',
        signals: [],
        simulation: true,
        poll_interval_ms: 1000,
      },
    };
  },
  normalizeConfig(config: unknown): NodeSeed['config'] {
    const rawConfig = isRecord(config) ? config : {};
    return {
      ...rawConfig,
      device_id: typeof rawConfig.device_id === 'string' ? rawConfig.device_id : '',
      signals: Array.isArray(rawConfig.signals) ? rawConfig.signals : [],
      simulation: rawConfig.simulation === true,
      poll_interval_ms:
        typeof rawConfig.poll_interval_ms === 'number' &&
        Number.isFinite(rawConfig.poll_interval_ms)
          ? Math.max(100, Math.round(rawConfig.poll_interval_ms))
          : 1000,
    };
  },
  getNodeSize() {
    return STANDARD_NODE_SIZE;
  },
  buildRegistryMeta() {
    return {
      defaultExpanded: true,
      size: this.getNodeSize(),
    };
  },
  validate(_ctx: NodeValidationContext): NodeValidation[] {
    return [];
  },
} satisfies NodeDefinition;
