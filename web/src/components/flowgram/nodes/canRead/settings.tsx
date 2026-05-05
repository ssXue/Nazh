import type { NodeSettingsProps } from '../settings-shared';
import { SwitchBar } from '../settings-shared';

export function CanReadNodeSettings({ draft, updateDraft }: NodeSettingsProps) {
  return (
    <>
      <label>
        <span>CAN ID 过滤</span>
        <input
          value={draft.canId}
          onChange={(event) => updateDraft({ canId: event.target.value })}
          placeholder="留空表示接收全部"
        />
      </label>
      <SwitchBar
        checked={draft.canIsExtended}
        onChange={(value) => updateDraft({ canIsExtended: value })}
        label="扩展帧"
      />
      <label>
        <span>接收超时 ms</span>
        <input
          value={draft.canReadTimeoutMs}
          onChange={(event) => updateDraft({ canReadTimeoutMs: event.target.value })}
        />
      </label>
    </>
  );
}
