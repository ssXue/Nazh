import {
  type NodeDefinition,
  type NodeSeed,
  type NodeValidationContext,
  type NodeValidation,
  TRYCATCH_BRANCHES,
  isRecord,
} from '../shared';

export const definition = {
  kind: 'tryCatch' as const,
  catalog: { category: '流程控制', description: '脚本异常捕获路由' },
  fallbackLabel: 'TryCatch Node',
  palette: { title: 'Try 捕获', badge: 'Try' },
  ai: { hint: 'config 必须含 script；下游边 sourcePortId 只能是 try / catch。' },

  buildDefaultSeed(): NodeSeed {
    return {
      idPrefix: 'try_catch_node',
      kind: 'tryCatch' as const,
      label: '',
      timeoutMs: 1000,
      config: { script: 'payload' },
    };
  },

  normalizeConfig(config: unknown): NodeSeed['config'] {
    const rawConfig = isRecord(config) ? config : {};
    return {
      ...rawConfig,
      script: typeof rawConfig.script === 'string' ? rawConfig.script : 'payload',
    };
  },

  getOutputPorts() {
    return TRYCATCH_BRANCHES;
  },

  getRoutingBranches() {
    return TRYCATCH_BRANCHES;
  },

  getNodeSize() {
    return { width: 240, height: 168 };
  },

  buildRegistryMeta() {
    return {
      defaultExpanded: true,
      size: this.getNodeSize(),
      defaultPorts: [{ type: 'input' as const }],
      useDynamicPort: true,
    };
  },

  validate(_ctx: NodeValidationContext): NodeValidation[] {
    return [];
  },
} satisfies NodeDefinition;
