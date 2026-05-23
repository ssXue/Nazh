import type { NodeSettingsProps } from '../settings-shared';
import { getPrimaryEditorLabel } from '../settings-shared';
import { CodeEditor } from '../../CodeEditor';

export function TryCatchNodeSettings({ draft, updateDraft }: NodeSettingsProps) {
  return (
    <label>
      <span>{getPrimaryEditorLabel(draft.nodeType)}</span>
      <CodeEditor value={draft.script} onChange={(value) => updateDraft({ script: value })} />
    </label>
  );
}
