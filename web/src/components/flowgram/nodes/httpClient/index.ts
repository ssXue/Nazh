import {
  type NodeDefinition,
  type NodeSeed,
  type NodeValidationContext,
  type NodeValidation,
  DEFAULT_HTTP_ALARM_BODY_TEMPLATE,
  DEFAULT_HTTP_ALARM_TITLE_TEMPLATE,
  inferHttpWebhookKind,
  isRecord,
  normalizeHttpBodyMode,
} from '../shared';

export const definition = {
  kind: 'httpClient' as const,
  catalog: { category: '外部通信', description: '将 payload 发送到 HTTP 端点' },
  fallbackLabel: 'HTTP Client',
  palette: { title: 'HTTP Client', badge: 'HTTP' },
  ai: {
    hint:
      'HTTP 发送节点；config 可含 body_mode, title_template, body_template；连接信息来自 connectionId。',
  },
  requiresConnection: true,

  buildDefaultSeed(): NodeSeed {
    return {
      idPrefix: 'http_client',
      kind: 'httpClient' as const,
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
    const rawConfig = isRecord(config) ? config : {};
    const legacyUrl = typeof rawConfig.url === 'string' ? rawConfig.url : '';
    const {
      url: _unusedUrl,
      method: _unusedMethod,
      headers: _unusedHeaders,
      webhook_kind: rawWebhookKind,
      content_type: _unusedContentType,
      request_timeout_ms: _unusedRequestTimeoutMs,
      at_mobiles: _unusedAtMobiles,
      at_all: _unusedAtAll,
      ...restConfig
    } = rawConfig;
    const webhookKind =
      typeof rawWebhookKind === 'string' && rawWebhookKind.trim()
        ? rawWebhookKind
        : inferHttpWebhookKind(legacyUrl);
    const bodyMode = normalizeHttpBodyMode(rawConfig.body_mode, webhookKind);

    return {
      ...restConfig,
      body_mode: bodyMode,
      title_template:
        typeof rawConfig.title_template === 'string'
          ? rawConfig.title_template
          : DEFAULT_HTTP_ALARM_TITLE_TEMPLATE,
      body_template:
        typeof rawConfig.body_template === 'string'
          ? rawConfig.body_template
          : bodyMode === 'dingtalk_markdown'
            ? DEFAULT_HTTP_ALARM_BODY_TEMPLATE
            : '',
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
    if (ctx.resolvedHttpBodyMode === 'template' && !ctx.draft.httpBodyTemplate.trim()) {
      result.push({ tone: 'warning', message: '自定义模板模式下建议填写消息模板。' });
    }
    if (ctx.resolvedHttpBodyMode === 'dingtalk_markdown' && !ctx.draft.httpTitleTemplate.trim()) {
      result.push({ tone: 'warning', message: '钉钉 Markdown 模式建议填写标题模板。' });
    }
    return result;
  },
} satisfies NodeDefinition;
