import { type NodeDefinition, type NodeSeed, type NodeValidationContext, type NodeValidation, isRecord } from '../shared';

export const definition = {
  kind: 'mqttClient' as const,
  catalog: { category: '硬件接口', description: '发布或订阅 MQTT 消息' },
  fallbackLabel: 'MQTT Client',
  palette: { title: 'MQTT Client', badge: 'MQTT' },
  ai: { hint: 'MQTT 发布或订阅；config.mode 为 publish 或 subscribe，通常不填写 connectionId。' },

  fieldValidators: {
    mqttTopic: v => !v.trim() ? 'MQTT 主题不能为空。' : null,
  },

  buildDefaultSeed(): NodeSeed {
    return {
      idPrefix: 'mqtt_client',
      kind: 'mqttClient' as const,
      label: '',
      timeoutMs: 1000,
      config: { mode: 'publish', topic: '', qos: 0, payload_template: '' },
    };
  },

  normalizeConfig(config: unknown): NodeSeed['config'] {
    const rawConfig = isRecord(config) ? config : {};
    return {
      ...rawConfig,
      mode: rawConfig.mode === 'subscribe' ? 'subscribe' : 'publish',
      topic: typeof rawConfig.topic === 'string' ? rawConfig.topic : '',
      qos: [0, 1, 2].includes(rawConfig.qos as number) ? rawConfig.qos : 0,
      payload_template:
        typeof rawConfig.payload_template === 'string' ? rawConfig.payload_template : '',
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
