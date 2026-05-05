import type { NodeSettingsProps } from '../settings-shared';
import { SwitchBar } from '../settings-shared';

export function CanWriteNodeSettings({ draft, updateDraft }: NodeSettingsProps) {
  return (
    <>
      <label>
        <span>默认 CAN ID</span>
        <input
          value={draft.canId}
          onChange={(event) => updateDraft({ canId: event.target.value })}
          placeholder="留空时从 payload.can_id 读取"
        />
      </label>
      <SwitchBar
        checked={draft.canIsExtended}
        onChange={(value) => updateDraft({ canIsExtended: value })}
        label="扩展帧"
      />
    </>
  );
}
