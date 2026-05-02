import type { NodeSettingsProps } from '../settings-shared';
import { SwitchBar } from '../settings-shared';

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
      <SwitchBar
        label="部署后立即触发"
        checked={draft.timerImmediate}
        onChange={(value) => updateDraft({ timerImmediate: value })}
      />
    </>
  );
}
