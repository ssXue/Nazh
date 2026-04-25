import type { NodeSettingsProps } from '../settings-shared';

export function SqlWriterNodeSettings({ draft, updateDraft }: NodeSettingsProps) {
  return (
    <>
      <label>
        <span>数据库路径</span>
        <input
          value={draft.sqlDatabasePath}
          onChange={(event) => updateDraft({ sqlDatabasePath: event.target.value })}
        />
      </label>
      <label>
        <span>表名</span>
        <input value={draft.sqlTable} onChange={(event) => updateDraft({ sqlTable: event.target.value })} />
      </label>
    </>
  );
}
