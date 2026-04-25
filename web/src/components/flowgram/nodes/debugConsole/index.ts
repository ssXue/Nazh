import { type NodeDefinition, type NodeSeed, normalizeNodeConfig } from '../shared';

export const definition: NodeDefinition = {
  kind: 'debugConsole',
  catalog: { category: '调试工具', description: '将 payload 打印到调试控制台以供检查' },
  fallbackLabel: 'Debug Console',

  buildDefaultSeed(): NodeSeed {
    return {
      idPrefix: 'debug_console',
      kind: 'debugConsole',
      label: '',
      timeoutMs: 500,
      config: { label: '', pretty: true },
    };
  },

  normalizeConfig(config: unknown): NodeSeed['config'] {
    return normalizeNodeConfig('debugConsole', config);
  },

  getNodeSize() {
    return { width: 214, height: 132 };
  },

  buildRegistryMeta() {
    return { defaultExpanded: true, size: this.getNodeSize() };
  },
};
