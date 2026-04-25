import {
  type NodeDefinition,
  type NodeSeed,
  type NodeValidationContext,
  type NodeValidation,
  normalizeNodeConfig,
  DEFAULT_HTTP_ALARM_TITLE_TEMPLATE,
} from '../shared';

export const definition: NodeDefinition = {
  kind: 'httpClient',
  catalog: { category: '外部通信', description: '将 payload 发送到 HTTP 端点' },
  fallbackLabel: 'HTTP Client',
  requiresConnection: true,

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
};
