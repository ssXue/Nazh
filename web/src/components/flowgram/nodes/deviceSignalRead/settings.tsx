import type { NodeSettingsProps } from '../settings-shared';
import { SwitchBar } from '../settings-shared';

export function DeviceSignalReadNodeSettings({ draft, updateDraft }: NodeSettingsProps) {
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
        <span>信号 ID</span>
        <input
          value={draft.signalId}
          onChange={(event) => updateDraft({ signalId: event.target.value })}
        />
      </label>
      <label>
        <span>轮询超时 ms</span>
        <input
          value={draft.signalPollTimeoutMs}
          onChange={(event) => updateDraft({ signalPollTimeoutMs: event.target.value })}
          placeholder="2000"
        />
      </label>
      <SwitchBar
        label="模拟模式"
        checked={draft.signalSimulation}
        onChange={(value) => updateDraft({ signalSimulation: value })}
      />
    </>
  );
}
