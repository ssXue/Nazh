import type { NodeSettingsProps } from '../settings-shared';
import { getPrimaryEditorLabel } from '../settings-shared';

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
      <textarea value={draft.script} onChange={(event) => updateDraft({ script: event.target.value })} />
    </label>
  );
}
