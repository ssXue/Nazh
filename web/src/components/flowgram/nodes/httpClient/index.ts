import {
  type NodeDefinition,
  type NodeSeed,
  normalizeNodeConfig,
  DEFAULT_HTTP_ALARM_TITLE_TEMPLATE,
} from '../shared';

export const definition: NodeDefinition = {
  kind: 'httpClient',
  catalog: { category: '外部通信', description: '将 payload 发送到 HTTP 端点' },
  fallbackLabel: 'HTTP Client',

  buildDefaultSeed(): NodeSeed {
    return {
      idPrefix: 'http_client',
      kind: 'httpClient',
      label: '',
      timeoutMs: 1000,
      config: {
        body_mode: 'json',
        title_template: DEFAULT_HTTP_ALARM_TITLE_TEMPLATE,
        body_template: '',
      },
    };
  },

  normalizeConfig(config: unknown): NodeSeed['config'] {
    return normalizeNodeConfig('httpClient', config);
  },

  getNodeSize() {
    return { width: 214, height: 132 };
  },

  buildRegistryMeta() {
    return { defaultExpanded: true, size: this.getNodeSize() };
  },
};
