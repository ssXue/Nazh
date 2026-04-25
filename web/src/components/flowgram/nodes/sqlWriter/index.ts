import { type NodeDefinition, type NodeSeed, type NodeValidationContext, type NodeValidation, normalizeNodeConfig } from '../shared';

export const definition: NodeDefinition = {
  kind: 'sqlWriter',
  catalog: { category: '持久化', description: '将当前 payload 持久化到本地 SQLite 表' },
  fallbackLabel: 'SQL Writer',

  fieldValidators: {
    sqlDatabasePath: v => !v.trim() ? { message: '数据库路径为空。', tone: 'warning' } : null,
    sqlTable: v => !v.trim() ? '表名不能为空。' : null,
  },

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

  validate(_ctx: NodeValidationContext): NodeValidation[] {
    return [];
  },
};
