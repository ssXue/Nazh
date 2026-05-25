import type { NodeSettingsProps } from '../settings-shared';
import { SwitchBar } from '../settings-shared';

export function DeviceEventTriggerNodeSettings({ draft, updateDraft }: NodeSettingsProps) {
  return (
    <>
      <label>
        <span>设备 ID</span>
        <input
          value={draft.deviceId}
          onChange={(event) => updateDraft({ deviceId: event.target.value })}
          placeholder="绑定设备的连接将自动继承"
        />
      </label>
      <label>
        <span>轮询间隔 ms</span>
        <input
          value={draft.eventPollIntervalMs}
          onChange={(event) => updateDraft({ eventPollIntervalMs: event.target.value })}
          placeholder="1000"
        />
      </label>
      <SwitchBar
        label="模拟模式"
        checked={draft.eventSimulation}
        onChange={(value) => updateDraft({ eventSimulation: value })}
      />
    </>
  );
}
