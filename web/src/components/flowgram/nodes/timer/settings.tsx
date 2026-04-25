import type { NodeSettingsProps } from '../settings-shared';

export function TimerNodeSettings({ draft, updateDraft }: NodeSettingsProps) {
  return (
    <>
      <label>
        <span>触发间隔 ms</span>
        <input
          value={draft.timerIntervalMs}
          onChange={(event) => updateDraft({ timerIntervalMs: event.target.value })}
        />
      </label>
      <label>
        <span>部署后立即触发</span>
        <select
          value={draft.timerImmediate ? 'true' : 'false'}
          onChange={(event) =>
            updateDraft({ timerImmediate: event.target.value === 'true' })
          }
        >
          <option value="true">是</option>
          <option value="false">否</option>
        </select>
      </label>
    </>
  );
}
