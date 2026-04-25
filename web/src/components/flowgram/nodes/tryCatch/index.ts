import { type NodeDefinition, type NodeSeed, type NodeValidationContext, type NodeValidation, normalizeNodeConfig } from '../shared';

export const definition: NodeDefinition = {
  kind: 'tryCatch',
  catalog: { category: '流程控制', description: '脚本异常捕获路由' },
  fallbackLabel: 'TryCatch Node',

  buildDefaultSeed(): NodeSeed {
    return {
      idPrefix: 'try_catch_node',
      kind: 'tryCatch',
      label: '',
      timeoutMs: 1000,
      config: { script: 'payload' },
    };
  },

  normalizeConfig(config: unknown): NodeSeed['config'] {
    return normalizeNodeConfig('tryCatch', config);
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
};
