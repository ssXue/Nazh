import { type NodeDefinition, type NodeSeed, type NodeValidationContext, type NodeValidation, normalizeNodeConfig } from '../shared';
import { parsePositiveInteger } from '../settings-shared';

export const definition: NodeDefinition = {
  kind: 'timer',
  catalog: { category: '硬件接口', description: '按固定间隔触发工作流并注入计时元数据' },
  fallbackLabel: 'Timer Node',

  fieldValidators: {
    timerIntervalMs: v => parsePositiveInteger(v) === null ? '定时间隔必须是大于 0 的毫秒数。' : null,
  },

  buildDefaultSeed(): NodeSeed {
    return {
      idPrefix: 'timer_node',
      kind: 'timer',
      label: '',
      timeoutMs: null,
      config: { interval_ms: 5000, immediate: true },
    };
  },

  normalizeConfig(config: unknown): NodeSeed['config'] {
    return normalizeNodeConfig('timer', config);
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
