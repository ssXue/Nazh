import { type NodeDefinition, type NodeSeed, type NodeValidationContext, type NodeValidation, normalizeNodeConfig } from '../shared';

export const definition: NodeDefinition = {
  kind: 'if',
  catalog: { category: '流程控制', description: '布尔条件分支路由' },
  fallbackLabel: 'IF Node',

  buildDefaultSeed(): NodeSeed {
    return {
      idPrefix: 'if_node',
      kind: 'if',
      label: '',
      timeoutMs: 1000,
      config: { script: 'payload["value"] > 0' },
    };
  },

  normalizeConfig(config: unknown): NodeSeed['config'] {
    return normalizeNodeConfig('if', config);
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
