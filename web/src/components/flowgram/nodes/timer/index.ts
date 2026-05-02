import { type NodeDefinition, type NodeSeed, type NodeValidationContext, type NodeValidation, isRecord } from '../shared';
import { parsePositiveInteger } from '../settings-shared';

export const definition = {
  kind: 'timer' as const,
  catalog: { category: '硬件接口', description: '按固定间隔触发工作流并注入计时元数据' },
  fallbackLabel: 'Timer Node',
  palette: { title: 'Timer', badge: 'Timer' },
  ai: { hint: 'config 可含 interval_ms, immediate, inject。' },

  fieldValidators: {
    timerIntervalMs: v => parsePositiveInteger(v) === null ? '定时间隔必须是大于 0 的毫秒数。' : null,
  },

  buildDefaultSeed(): NodeSeed {
    return {
      idPrefix: 'timer_node',
      kind: 'timer' as const,
      label: '',
      timeoutMs: null,
      config: { interval_ms: 5000, immediate: true },
    };
  },

  normalizeConfig(config: unknown): NodeSeed['config'] {
    const rawConfig = isRecord(config) ? config : {};
    return {
      ...rawConfig,
      interval_ms:
        typeof rawConfig.interval_ms === 'number' && Number.isFinite(rawConfig.interval_ms)
          ? Math.max(1, Math.round(rawConfig.interval_ms))
          : 5000,
      immediate: rawConfig.immediate !== false,
      inject: isRecord(rawConfig.inject) ? rawConfig.inject : {},
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
