import type { NodeSettingsProps } from '../settings-shared';

export function DebugConsoleNodeSettings({ draft, updateDraft }: NodeSettingsProps) {
  return (
    <>
      <label>
        <span>输出标签</span>
        <input
          value={draft.debugLabel}
          onChange={(event) => updateDraft({ debugLabel: event.target.value })}
        />
      </label>
      <label>
        <span>输出格式</span>
        <select
          value={draft.debugPretty ? 'pretty' : 'compact'}
          onChange={(event) => updateDraft({ debugPretty: event.target.value === 'pretty' })}
        >
          <option value="pretty">格式化 JSON</option>
          <option value="compact">紧凑 JSON</option>
        </select>
      </label>
    </>
  );
}
