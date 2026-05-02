import {
  type NodeDefinition,
  type NodeSeed,
  type NodeValidationContext,
  type NodeValidation,
  DEFAULT_SWITCH_BRANCHES,
  isRecord,
  normalizeSwitchBranches,
} from '../shared';

export const definition = {
  kind: 'switch' as const,
  catalog: { category: '流程控制', description: '多路分支路由' },
  fallbackLabel: 'Switch Node',
  palette: { title: 'Switch 分流', badge: 'Switch' },
  ai: {
    hint:
      'config 必须含 script 与 branches；script 返回值匹配 branches[].key，兜底分支 sourcePortId 使用 default。',
  },

  buildDefaultSeed(): NodeSeed {
    return {
      idPrefix: 'switch_node',
      kind: 'switch' as const,
      label: '',
      timeoutMs: 1000,
      config: {
        script: 'payload["route"]',
        branches: [
          { key: 'route_a', label: 'Route A' },
          { key: 'route_b', label: 'Route B' },
        ],
      },
    };
  },

  normalizeConfig(config: unknown): NodeSeed['config'] {
    const rawConfig = isRecord(config) ? config : {};
    return {
      ...rawConfig,
      script: typeof rawConfig.script === 'string' ? rawConfig.script : 'payload["route"]',
      branches: normalizeSwitchBranches(rawConfig.branches),
    };
  },

  getOutputPorts(config: unknown) {
    const rawConfig = isRecord(config) ? config : {};
    return [...normalizeSwitchBranches(rawConfig.branches), ...DEFAULT_SWITCH_BRANCHES];
  },

  getRoutingBranches(config: unknown) {
    const rawConfig = isRecord(config) ? config : {};
    return [...normalizeSwitchBranches(rawConfig.branches), ...DEFAULT_SWITCH_BRANCHES];
  },

  getNodeSize() {
    return { width: 252, height: 188 };
  },

  buildRegistryMeta() {
    return {
      defaultExpanded: true,
      size: this.getNodeSize(),
      defaultPorts: [{ type: 'input' as const }],
      useDynamicPort: true,
    };
  },

  validate(ctx: NodeValidationContext): NodeValidation[] {
    const result: NodeValidation[] = [];
    if (ctx.draft.branches.length === 0) {
      result.push({ tone: 'warning', message: 'Switch 节点至少建议保留一个自定义分支。' });
    }
    return result;
  },
} satisfies NodeDefinition;
