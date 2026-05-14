import type { NodeSettingsProps } from '../settings-shared';
import { CodeEditor } from '../../CodeEditor';

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
        <CodeEditor
          language="json"
          value={draft.capabilityImplementationJson}
          onChange={(value) => updateDraft({ capabilityImplementationJson: value })}
        />
      </label>
      <label>
        <span>参数</span>
        <CodeEditor
          language="json"
          value={draft.capabilityArgsJson}
          onChange={(value) => updateDraft({ capabilityArgsJson: value })}
        />
      </label>
    </>
  );
}
