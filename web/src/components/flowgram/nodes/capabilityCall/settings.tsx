import type { NodeSettingsProps } from '../settings-shared';
import { CodeEditor } from '../../CodeEditor';

export function CapabilityCallNodeSettings({ draft, updateDraft, connections }: NodeSettingsProps) {
  // ADR-0026：连接从设备继承，不直接编辑。展示解析后的连接信息（只读）。
  const deviceId = draft.capabilityDeviceId.trim();
  const inheritedConnectionId = draft.connectionId.trim();
  const resolvedConnection = inheritedConnectionId
    ? connections.find((c) => c.id === inheritedConnectionId)
    : undefined;

  return (
    <>
      <label>
        <span>设备 ID</span>
        <input
          value={draft.capabilityDeviceId}
          onChange={(event) => updateDraft({ capabilityDeviceId: event.target.value })}
          placeholder="设备绑定连接后自动继承"
        />
      </label>
      <div className="flowgram-form__info-row">
        <span className="flowgram-form__info-label">继承连接</span>
        <span className="flowgram-form__info-value">
          {deviceId ? (
            inheritedConnectionId ? (
              resolvedConnection
                ? `${inheritedConnectionId} · ${resolvedConnection.type}`
                : `${inheritedConnectionId}（未找到对应资产）`
            ) : (
              <span className="flowgram-form__info-value--warn">设备未绑定连接</span>
            )
          ) : (
            <span className="flowgram-form__info-value--muted">请先填写设备 ID</span>
          )}
        </span>
      </div>
      <label>
        <span>能力 ID</span>
        <input
          value={draft.capabilityId}
          onChange={(event) => updateDraft({ capabilityId: event.target.value })}
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
