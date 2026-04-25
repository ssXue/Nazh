import type { NodeSettingsProps } from '../settings-shared';
import { getPrimaryEditorLabel } from '../settings-shared';

export function NativeNodeSettings({ draft, updateDraft }: NodeSettingsProps) {
  return (
    <label>
      <span>{getPrimaryEditorLabel(draft.nodeType)}</span>
      <textarea value={draft.message} onChange={(event) => updateDraft({ message: event.target.value })} />
    </label>
  );
}
