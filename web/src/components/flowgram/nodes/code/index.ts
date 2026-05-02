import { type NodeDefinition, type NodeSeed, type NodeValidationContext, type NodeValidation, isRecord, normalizeScriptAiConfig } from '../shared';

export const definition = {
  kind: 'code' as const,
  catalog: { category: '脚本执行', description: '沙箱化脚本执行节点' },
  fallbackLabel: 'Code Node',
  palette: { title: 'Code Node', badge: 'Code' },
  ai: {
    hint:
      'config 必须含 script；脚本输入变量是 payload，可用 ai_complete("prompt"), rand(min, max), now_ms(), from_json(text), to_json(value), is_blank(text)。',
  },

  buildDefaultSeed(): NodeSeed {
    return {
      idPrefix: 'code_node',
      kind: 'code' as const,
      label: '',
      timeoutMs: 1000,
      config: { script: 'payload' },
    };
  },

  normalizeConfig(config: unknown): NodeSeed['config'] {
    const rawConfig = isRecord(config) ? config : {};
    const { ai: _unusedAi, ...restConfig } = rawConfig;
    const ai = normalizeScriptAiConfig(rawConfig.ai);

    return {
      ...restConfig,
      script: typeof rawConfig.script === 'string' ? rawConfig.script : 'payload',
      ...(ai ? { ai } : {}),
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
    if (!ctx.draft.script.trim()) {
      result.push({ tone: 'danger', message: '脚本为空。' });
    }
    const { aiProviders, activeAiProviderId, resolvedGlobalAiProvider, preferredCopilotProvider } = ctx;
    if (aiProviders.length === 0) {
      result.push({ tone: 'warning', message: '当前尚未配置全局 AI，运行时将无法完成 AI 调用。' });
    } else if (activeAiProviderId && !preferredCopilotProvider) {
      result.push({ tone: 'danger', message: `全局 AI ${activeAiProviderId} 未在配置中找到。` });
    } else if (!resolvedGlobalAiProvider) {
      result.push({ tone: 'warning', message: '当前还没有选中全局 AI，请先前往 AI 配置页设置。' });
    } else if (!resolvedGlobalAiProvider.enabled) {
      result.push({ tone: 'danger', message: `全局 AI ${resolvedGlobalAiProvider.name} 已被禁用。` });
    } else if (!resolvedGlobalAiProvider.hasApiKey) {
      result.push({ tone: 'danger', message: `全局 AI ${resolvedGlobalAiProvider.name} 尚未配置 API Key。` });
    } else {
      result.push({ tone: 'info', message: `默认使用全局 AI · ${resolvedGlobalAiProvider.name}${resolvedGlobalAiProvider.defaultModel.trim() ? ` · ${resolvedGlobalAiProvider.defaultModel.trim()}` : ' · 使用提供商默认模型'}` });
    }
    return result;
  },
} satisfies NodeDefinition;
