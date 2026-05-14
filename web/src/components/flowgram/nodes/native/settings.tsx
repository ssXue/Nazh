import type { NodeSettingsProps } from '../settings-shared';
import { getPrimaryEditorLabel } from '../settings-shared';
import { CodeEditor } from '../../CodeEditor';

export function NativeNodeSettings({ draft, updateDraft }: NodeSettingsProps) {
  return (
    <label>
      <span>{getPrimaryEditorLabel(draft.nodeType)}</span>
      <CodeEditor value={draft.message} onChange={(value) => updateDraft({ message: value })} />
    </label>
  );
}
