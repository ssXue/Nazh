import type { NodeSettingsProps } from '../settings-shared';
import { getPrimaryEditorLabel } from '../settings-shared';
import { CodeEditor } from '../../CodeEditor';

export function IfNodeSettings({ draft, updateDraft, aiGenerating, preferredCopilotProvider, aiGenerateButtonTitle, onOpenAiDialog }: NodeSettingsProps) {
  return (
    <label>
      <span>
        {getPrimaryEditorLabel(draft.nodeType)}
        <button
          type="button"
          className="ghost flowgram-btn-ai"
          disabled={!preferredCopilotProvider || aiGenerating}
          onClick={onOpenAiDialog}
          title={aiGenerateButtonTitle}
        >
          {aiGenerating ? '生成中...' : 'AI 生成'}
        </button>
      </span>
      <CodeEditor value={draft.script} onChange={(value) => updateDraft({ script: value })} />
    </label>
  );
}
