import { type NodeDefinition, type NodeSeed, normalizeNodeConfig } from '../shared';

export const definition: NodeDefinition = {
  kind: 'sqlWriter',
  catalog: { category: '持久化', description: '将当前 payload 持久化到本地 SQLite 表' },
  fallbackLabel: 'SQL Writer',

  buildDefaultSeed(): NodeSeed {
    return {
      idPrefix: 'sql_writer',
      kind: 'sqlWriter',
      label: '',
      timeoutMs: 1500,
      config: { database_path: './nazh-local.sqlite3', table: 'workflow_logs' },
    };
  },

  normalizeConfig(config: unknown): NodeSeed['config'] {
    return normalizeNodeConfig('sqlWriter', config);
  },

  getNodeSize() {
    return { width: 214, height: 132 };
  },

  buildRegistryMeta() {
    return { defaultExpanded: true, size: this.getNodeSize() };
  },
};
