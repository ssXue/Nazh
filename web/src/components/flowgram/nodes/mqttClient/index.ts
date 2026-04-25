import { type NodeDefinition, type NodeSeed, type NodeValidationContext, type NodeValidation, normalizeNodeConfig } from '../shared';

export const definition: NodeDefinition = {
  kind: 'mqttClient',
  catalog: { category: '硬件接口', description: '发布或订阅 MQTT 消息' },
  fallbackLabel: 'MQTT Client',

  fieldValidators: {
    mqttTopic: v => !v.trim() ? 'MQTT 主题不能为空。' : null,
  },

  buildDefaultSeed(): NodeSeed {
    return {
      idPrefix: 'mqtt_client',
      kind: 'mqttClient',
      label: '',
      timeoutMs: 1000,
      config: { mode: 'publish', topic: '', qos: 0, payload_template: '' },
    };
  },

  normalizeConfig(config: unknown): NodeSeed['config'] {
    return normalizeNodeConfig('mqttClient', config);
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
