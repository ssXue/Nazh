import type { NodeSettingsProps } from '../settings-shared';

export function EthercatPdoNodeSettings({ draft, updateDraft }: NodeSettingsProps) {
  return (
    <label>
      <span>从站地址</span>
      <input
        value={draft.ethercatSlaveAddress}
        onChange={(event) => updateDraft({ ethercatSlaveAddress: event.target.value })}
        placeholder="1"
      />
    </label>
  );
}
