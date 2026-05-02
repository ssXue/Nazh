import { type NodeDefinition, type NodeSeed, type NodeValidationContext, type NodeValidation, isRecord } from '../shared';

export const definition = {
  kind: 'sqlWriter' as const,
  catalog: { category: '持久化', description: '将当前 payload 持久化到本地 SQLite 表' },
  fallbackLabel: 'SQL Writer',
  palette: { title: 'SQL Writer', badge: 'SQL' },
  ai: { hint: 'SQLite 写入节点；config 可含 database_path 与 table。' },

  fieldValidators: {
    sqlDatabasePath: v => !v.trim() ? { message: '数据库路径为空。', tone: 'warning' } : null,
    sqlTable: v => !v.trim() ? '表名不能为空。' : null,
  },

  buildDefaultSeed(): NodeSeed {
    return {
      idPrefix: 'sql_writer',
      kind: 'sqlWriter' as const,
      label: '',
      timeoutMs: 1500,
      config: { database_path: './nazh-local.sqlite3', table: 'workflow_logs' },
    };
  },

  normalizeConfig(config: unknown): NodeSeed['config'] {
    const rawConfig = isRecord(config) ? config : {};
    return {
      ...rawConfig,
      database_path:
        typeof rawConfig.database_path === 'string' ? rawConfig.database_path : './nazh-local.sqlite3',
      table: typeof rawConfig.table === 'string' ? rawConfig.table : 'workflow_logs',
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
