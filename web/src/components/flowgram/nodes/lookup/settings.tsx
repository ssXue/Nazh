import type { NodeSettingsProps } from '../settings-shared';

export function LookupNodeSettings({ draft, updateDraft }: NodeSettingsProps) {
  const table = draft.lookupTable;
  const entries = Object.entries(table);

  const updateRow = (oldKey: string, newKey: string, newValue: string) => {
    const next: Record<string, unknown> = {};
    for (const [k, v] of Object.entries(table)) {
      if (k === oldKey) {
        try { next[newKey] = JSON.parse(newValue); }
        catch { next[newKey] = newValue; }
      } else {
        next[k] = v;
      }
    }
    updateDraft({ lookupTable: next });
  };

  const removeRow = (key: string) => {
    const next = { ...table };
    delete next[key];
    updateDraft({ lookupTable: next });
  };

  const addRow = () => {
    updateDraft({ lookupTable: { ...table, '': null } });
  };

  return (
    <>
      <label>
        <span>查找表</span>
      </label>
      <div className="lookup-table-editor">
        {entries.map(([key, value]) => (
          <div key={key} className="lookup-table-editor__row">
            <input
              className="lookup-table-editor__key"
              defaultValue={key}
              onBlur={(e) => updateRow(key, e.target.value, JSON.stringify(value))}
            />
            <input
              className="lookup-table-editor__value"
              defaultValue={JSON.stringify(value)}
              onBlur={(e) => updateRow(key, key, e.target.value)}
            />
            <button type="button" onClick={() => removeRow(key)}>×</button>
          </div>
        ))}
        <button type="button" className="lookup-table-editor__add" onClick={addRow}>
          + 添加行
        </button>
      </div>
      <label>
        <span>未命中默认值</span>
        <input
          value={draft.lookupDefault}
          placeholder='JSON 值（如 42 / "x" / null）'
          onChange={(e) => updateDraft({ lookupDefault: e.target.value })}
        />
      </label>
    </>
  );
}
