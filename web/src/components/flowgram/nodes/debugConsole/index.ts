import { type NodeDefinition, type NodeSeed, type NodeValidationContext, type NodeValidation, isRecord } from '../shared';

export const definition = {
  kind: 'debugConsole' as const,
  catalog: { category: '调试工具', description: '将 payload 打印到调试控制台以供检查' },
  fallbackLabel: 'Debug Console',
  palette: { title: 'Debug Console', badge: 'Debug' },
  ai: { hint: '调试输出节点；config 可含 label 与 pretty。' },

  buildDefaultSeed(): NodeSeed {
    return {
      idPrefix: 'debug_console',
      kind: 'debugConsole' as const,
      label: '',
      timeoutMs: 500,
      config: { label: '', pretty: true },
    };
  },

  normalizeConfig(config: unknown): NodeSeed['config'] {
    const rawConfig = isRecord(config) ? config : {};
    return {
      ...rawConfig,
      label: typeof rawConfig.label === 'string' ? rawConfig.label : '',
      pretty: rawConfig.pretty !== false,
    };
  },

  getNodeSize() {
    return { width: 214, height: 132 };
  },

  buildRegistryMeta() {
    return { defaultExpanded: true, size: this.getNodeSize() };
  },

  validate(ctx: NodeValidationContext): NodeValidation[] {
    return [{ tone: 'info', message: ctx.draft.debugPretty ? '当前以格式化 JSON 输出。' : '当前以紧凑 JSON 输出。' }];
  },
} satisfies NodeDefinition;
