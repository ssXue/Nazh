import { type NodeDefinition, type NodeSeed, normalizeNodeConfig } from '../shared';

export const definition: NodeDefinition = {
  kind: 'switch',
  catalog: { category: '流程控制', description: '多路分支路由' },
  fallbackLabel: 'Switch Node',

  buildDefaultSeed(): NodeSeed {
    return {
      idPrefix: 'switch_node',
      kind: 'switch',
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
    return normalizeNodeConfig('switch', config);
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
};
