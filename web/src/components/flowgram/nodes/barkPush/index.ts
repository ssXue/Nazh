import {
  type NodeDefinition,
  type NodeSeed,
  normalizeNodeConfig,
  DEFAULT_BARK_TITLE_TEMPLATE,
  DEFAULT_BARK_BODY_TEMPLATE,
} from '../shared';

export const definition: NodeDefinition = {
  kind: 'barkPush',
  catalog: { category: '外部通信', description: '向 Bark 服务发送 iOS 推送通知' },
  fallbackLabel: 'Bark Push',

  buildDefaultSeed(): NodeSeed {
    return {
      idPrefix: 'bark_push',
      kind: 'barkPush',
      label: '',
      timeoutMs: 1000,
      config: {
        content_mode: 'body',
        title_template: DEFAULT_BARK_TITLE_TEMPLATE,
        subtitle_template: '',
        body_template: DEFAULT_BARK_BODY_TEMPLATE,
        level: 'active',
        badge: '',
        sound: '',
        icon: '',
        group: '',
        url: '',
        copy: '',
        image: '',
        auto_copy: false,
        call: false,
        archive_mode: 'inherit',
      },
    };
  },

  normalizeConfig(config: unknown): NodeSeed['config'] {
    return normalizeNodeConfig('barkPush', config);
  },

  getNodeSize() {
    return { width: 214, height: 132 };
  },

  buildRegistryMeta() {
    return { defaultExpanded: true, size: this.getNodeSize() };
  },
};
