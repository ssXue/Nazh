import type { NodeSettingsProps } from '../settings-shared';

export function CapabilityCallNodeSettings({ draft, updateDraft }: NodeSettingsProps) {
  return (
    <>
      <label>
        <span>能力 ID</span>
        <input
          value={draft.capabilityId}
          onChange={(event) => updateDraft({ capabilityId: event.target.value })}
        />
      </label>
      <label>
        <span>设备 ID</span>
        <input
          value={draft.capabilityDeviceId}
          onChange={(event) => updateDraft({ capabilityDeviceId: event.target.value })}
        />
      </label>
      <label>
        <span>执行快照</span>
        <textarea
          rows={8}
          value={draft.capabilityImplementationJson}
          onChange={(event) => updateDraft({ capabilityImplementationJson: event.target.value })}
        />
      </label>
      <label>
        <span>参数</span>
        <textarea
          rows={5}
          value={draft.capabilityArgsJson}
          onChange={(event) => updateDraft({ capabilityArgsJson: event.target.value })}
        />
      </label>
    </>
  );
}
