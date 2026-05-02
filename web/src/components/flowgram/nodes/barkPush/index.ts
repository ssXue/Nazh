import {
  type NodeDefinition,
  type NodeSeed,
  type NodeValidationContext,
  type NodeValidation,
  DEFAULT_BARK_BODY_TEMPLATE,
  DEFAULT_BARK_TITLE_TEMPLATE,
  isRecord,
} from '../shared';
import { parseNonNegativeInteger } from '../settings-shared';

export const definition = {
  kind: 'barkPush' as const,
  catalog: { category: '外部通信', description: '向 Bark 服务发送 iOS 推送通知' },
  fallbackLabel: 'Bark Push',
  palette: { title: 'Bark Push', badge: 'Bark' },
  ai: {
    hint:
      'Bark 推送节点；config 可含 title_template, subtitle_template, body_template, level, badge, sound, icon, group, url, copy, image, auto_copy, call, archive_mode。',
  },
  requiresConnection: true,

  fieldValidators: {
    barkBadge: v => v.trim() && parseNonNegativeInteger(v) === null ? 'Bark badge 必须是大于等于 0 的整数。' : null,
  },

  buildDefaultSeed(): NodeSeed {
    return {
      idPrefix: 'bark_push',
      kind: 'barkPush' as const,
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
    const rawConfig = isRecord(config) ? config : {};
    const {
      server_url: _unusedServerUrl,
      device_key: _unusedDeviceKey,
      request_timeout_ms: _unusedRequestTimeoutMs,
      ...restConfig
    } = rawConfig;

    return {
      ...restConfig,
      content_mode: rawConfig.content_mode === 'markdown' ? 'markdown' : 'body',
      title_template:
        typeof rawConfig.title_template === 'string'
          ? rawConfig.title_template
          : DEFAULT_BARK_TITLE_TEMPLATE,
      subtitle_template:
        typeof rawConfig.subtitle_template === 'string' ? rawConfig.subtitle_template : '',
      body_template:
        typeof rawConfig.body_template === 'string'
          ? rawConfig.body_template
          : DEFAULT_BARK_BODY_TEMPLATE,
      level:
        rawConfig.level === 'critical' ||
        rawConfig.level === 'timeSensitive' ||
        rawConfig.level === 'passive'
          ? rawConfig.level
          : 'active',
      badge:
        typeof rawConfig.badge === 'number'
          ? String(rawConfig.badge)
          : typeof rawConfig.badge === 'string'
            ? rawConfig.badge
            : '',
      sound: typeof rawConfig.sound === 'string' ? rawConfig.sound : '',
      icon: typeof rawConfig.icon === 'string' ? rawConfig.icon : '',
      group: typeof rawConfig.group === 'string' ? rawConfig.group : '',
      url: typeof rawConfig.url === 'string' ? rawConfig.url : '',
      copy: typeof rawConfig.copy === 'string' ? rawConfig.copy : '',
      image: typeof rawConfig.image === 'string' ? rawConfig.image : '',
      auto_copy: rawConfig.auto_copy === true,
      call: rawConfig.call === true,
      archive_mode:
        rawConfig.archive_mode === 'archive' || rawConfig.archive_mode === 'skip'
          ? rawConfig.archive_mode
          : 'inherit',
    };
  },

  getNodeSize() {
    return { width: 214, height: 132 };
  },

  buildRegistryMeta() {
    return { defaultExpanded: true, size: this.getNodeSize() };
  },

  validate(ctx: NodeValidationContext): NodeValidation[] {
    const result: NodeValidation[] = [];
    if (!ctx.draft.barkTitleTemplate.trim() && !ctx.draft.barkBodyTemplate.trim()) {
      result.push({ tone: 'warning', message: '建议至少填写标题模板或消息模板。' });
    }
    return result;
  },
} satisfies NodeDefinition;
