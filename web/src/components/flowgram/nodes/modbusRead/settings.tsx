import type { NodeSettingsProps } from '../settings-shared';

export function ModbusReadNodeSettings({ draft, updateDraft }: NodeSettingsProps) {
  return (
    <>
      <label>
        <span>设备单元 ID</span>
        <input
          value={draft.modbusUnitId}
          onChange={(event) => updateDraft({ modbusUnitId: event.target.value })}
        />
      </label>
      <label>
        <span>寄存器地址</span>
        <input
          value={draft.modbusRegister}
          onChange={(event) => updateDraft({ modbusRegister: event.target.value })}
        />
      </label>
      <label>
        <span>读取数量</span>
        <input
          value={draft.modbusQuantity}
          onChange={(event) => updateDraft({ modbusQuantity: event.target.value })}
        />
      </label>
      <label>
        <span>寄存器类型</span>
        <select value={draft.modbusRegisterType} onChange={(event) => updateDraft({ modbusRegisterType: event.target.value })}>
          <option value="holding">Holding Register (03)</option>
          <option value="input">Input Register (04)</option>
          <option value="coil">Coil (01)</option>
          <option value="discrete">Discrete Input (02)</option>
        </select>
      </label>
      <label>
        <span>基准值</span>
        <input
          value={draft.modbusBaseValue}
          onChange={(event) => updateDraft({ modbusBaseValue: event.target.value })}
        />
      </label>
      <label>
        <span>波动幅度</span>
        <input
          value={draft.modbusAmplitude}
          onChange={(event) => updateDraft({ modbusAmplitude: event.target.value })}
        />
      </label>
    </>
  );
}
