import type { NodeSettingsProps } from '../settings-shared';

export function MqttClientNodeSettings({ draft, updateDraft }: NodeSettingsProps) {
  return (
    <>
      <label>
        <span>工作模式</span>
        <select value={draft.mqttMode} onChange={(event) => updateDraft({ mqttMode: event.target.value })}>
          <option value="publish">发布 (Publish)</option>
          <option value="subscribe">订阅 (Subscribe)</option>
        </select>
      </label>
      <label>
        <span>主题</span>
        <input
          value={draft.mqttTopic}
          onChange={(event) => updateDraft({ mqttTopic: event.target.value })}
          placeholder="sensors/temperature"
        />
      </label>
      <label>
        <span>QoS</span>
        <select value={draft.mqttQos} onChange={(event) => updateDraft({ mqttQos: event.target.value })}>
          <option value="0">0 - 最多一次</option>
          <option value="1">1 - 至少一次</option>
          <option value="2">2 - 恰好一次</option>
        </select>
      </label>
      {draft.mqttMode === 'publish' ? (
        <label>
          <span>载荷模板</span>
          <textarea
            value={draft.mqttPayloadTemplate}
            onChange={(event) => updateDraft({ mqttPayloadTemplate: event.target.value })}
          />
        </label>
      ) : null}
    </>
  );
}
