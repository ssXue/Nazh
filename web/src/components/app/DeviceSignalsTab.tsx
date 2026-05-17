import { useCallback, useState } from 'react';

import { formatRelativeTimestamp } from '../../lib/projects';
import { useDeviceAssets } from '../../hooks/use-device-assets';
import type { DeviceAssetDetail } from '../../hooks/use-device-assets';
import { DeleteActionIcon, PlusIcon } from './AppIcons';
import { EditableField, formatSignalType } from './device-modeling-helpers';

/** 信号 Tab。通过后端命令增删改字段。 */
export function SignalsTab({
  detail,
  workspacePath,
  onReload,
  onStatusMessage,
}: {
  detail: DeviceAssetDetail;
  workspacePath: string;
  onReload: () => void;
  onStatusMessage: (msg: string) => void;
}) {
  const { patchField, addSignal, removeSignal, addAlarm, removeAlarm } = useDeviceAssets(workspacePath);
  const [patching, setPatching] = useState(false);
  const [addingSignal, setAddingSignal] = useState(false);
  const [addingAlarm, setAddingAlarm] = useState(false);
  const [newSignalId, setNewSignalId] = useState('');
  const [newSignalType, setNewSignalType] = useState('analog_input');
  const [newAlarmId, setNewAlarmId] = useState('');
  const [newAlarmCondition, setNewAlarmCondition] = useState('');

  const spec = detail.spec_json;
  const signals = (spec?.signals ?? []) as Array<Record<string, unknown>>;
  const alarms = (spec?.alarms ?? []) as Array<Record<string, unknown>>;

  const patch = useCallback(
    async (jsonPath: string, value: string) => {
      setPatching(true);
      try {
        await patchField(detail.id, jsonPath, value);
        onReload();
      } catch (error) {
        onStatusMessage(`修改失败: ${error}`);
      } finally {
        setPatching(false);
      }
    },
    [detail.id, patchField, onReload, onStatusMessage],
  );

  const handleAddSignal = useCallback(async () => {
    if (!newSignalId.trim()) return;
    setPatching(true);
    try {
      const yaml = `id: ${newSignalId.trim()}\nsignal_type: ${newSignalType}\nsource:\n  type: register\n  register: 0\n  data_type: u16\n`;
      await addSignal(detail.id, yaml);
      setNewSignalId('');
      setAddingSignal(false);
      onStatusMessage(`信号 ${newSignalId} 已添加`);
      onReload();
    } catch (error) {
      onStatusMessage(`添加信号失败: ${error}`);
    } finally {
      setPatching(false);
    }
  }, [detail.id, newSignalId, newSignalType, addSignal, onReload, onStatusMessage]);

  const handleRemoveSignal = useCallback(async (index: number) => {
    setPatching(true);
    try {
      await removeSignal(detail.id, index);
      onStatusMessage('信号已删除');
      onReload();
    } catch (error) {
      onStatusMessage(`删除信号失败: ${error}`);
    } finally {
      setPatching(false);
    }
  }, [detail.id, removeSignal, onReload, onStatusMessage]);

  const handleAddAlarm = useCallback(async () => {
    if (!newAlarmId.trim() || !newAlarmCondition.trim()) return;
    setPatching(true);
    try {
      const yaml = `id: ${newAlarmId.trim()}\ncondition: "${newAlarmCondition.trim()}"\nseverity: warning\n`;
      await addAlarm(detail.id, yaml);
      setNewAlarmId('');
      setNewAlarmCondition('');
      setAddingAlarm(false);
      onStatusMessage(`告警 ${newAlarmId} 已添加`);
      onReload();
    } catch (error) {
      onStatusMessage(`添加告警失败: ${error}`);
    } finally {
      setPatching(false);
    }
  }, [detail.id, newAlarmId, newAlarmCondition, addAlarm, onReload, onStatusMessage]);

  const handleRemoveAlarm = useCallback(async (index: number) => {
    setPatching(true);
    try {
      await removeAlarm(detail.id, index);
      onStatusMessage('告警已删除');
      onReload();
    } catch (error) {
      onStatusMessage(`删除告警失败: ${error}`);
    } finally {
      setPatching(false);
    }
  }, [detail.id, removeAlarm, onReload, onStatusMessage]);

  return (
    <div className="dm-detail-body">
      {/* 信号表 */}
      <div className="dm-section-card">
        <div className="dm-section-card__header">
          <h3>信号 ({signals.length})</h3>
          <button type="button" className="dm-btn dm-btn--primary" disabled={patching} onClick={() => setAddingSignal(true)}>
            <PlusIcon width={14} height={14} />
            新增信号
          </button>
        </div>
        <div className="dm-section-card__body dm-section-card__body--no-pad">
          {signals.length > 0 ? (
            <table className="dm-table">
              <thead>
                <tr>
                  <th>ID</th>
                  <th>类型</th>
                  <th>单位</th>
                  <th>量程</th>
                  <th>缩放</th>
                  <th>来源</th>
                  <th />
                </tr>
              </thead>
              <tbody>
                {signals.map((sig, i) => {
                  const range = sig.range as number[] | undefined;
                  return (
                    <tr key={String(sig.id ?? i)}>
                      <td>
                        <EditableField value={String(sig.id)} label="signal.id" onSave={(v) => void patch(`/signals/${i}/id`, v)} disabled={patching} />
                      </td>
                      <td>
                        <span className={`dm-tag dm-tag--signal-${String(sig.signal_type).split('_')[0]}`}>
                          {formatSignalType(String(sig.signal_type))}
                        </span>
                      </td>
                      <td>
                        <EditableField value={sig.unit ? String(sig.unit) : '-'} label="signal.unit" onSave={(v) => void patch(`/signals/${i}/unit`, v)} disabled={patching} />
                      </td>
                      <td className="dm-mono">
                        {range
                          ? (
                            <>
                              <EditableField value={String(range[0])} label="range.min" onSave={(v) => void patch(`/signals/${i}/range/0`, v)} disabled={patching} />
                              {' ~ '}
                              <EditableField value={String(range[1])} label="range.max" onSave={(v) => void patch(`/signals/${i}/range/1`, v)} disabled={patching} />
                            </>
                          )
                          : '-'}
                      </td>
                      <td>
                        <EditableField value={sig.scale ? String(sig.scale) : '-'} label="signal.scale" onSave={(v) => void patch(`/signals/${i}/scale`, v)} disabled={patching} />
                      </td>
                      <td className="dm-mono">
                        {sig.source
                          ? String((sig.source as Record<string, unknown>).type ?? '-')
                          : '-'}
                      </td>
                      <td>
                        <button
                          type="button"
                          className="dm-btn dm-btn--danger"
                          title="删除信号"
                          disabled={patching}
                          onClick={() => void handleRemoveSignal(i)}
                        >
                          <DeleteActionIcon width={14} height={14} />
                        </button>
                      </td>
                    </tr>
                  );
                })}
              </tbody>
            </table>
          ) : (
            <div style={{ padding: '16px', textAlign: 'center', color: 'var(--muted)' }}>暂无信号</div>
          )}

          {addingSignal && (
            <div className="dm-add-row">
              <input
                type="text"
                className="dm-add-row__input"
                placeholder="信号 ID"
                value={newSignalId}
                onChange={(e) => setNewSignalId(e.target.value)}
                autoFocus
              />
              <select
                className="dm-add-row__select"
                value={newSignalType}
                onChange={(e) => setNewSignalType(e.target.value)}
              >
                <option value="analog_input">模拟输入</option>
                <option value="analog_output">模拟输出</option>
                <option value="digital_input">数字输入</option>
                <option value="digital_output">数字输出</option>
              </select>
              <button type="button" className="dm-btn dm-btn--primary" disabled={!newSignalId.trim() || patching} onClick={() => void handleAddSignal()}>
                确认
              </button>
              <button type="button" className="dm-btn" onClick={() => { setAddingSignal(false); setNewSignalId(''); }}>
                取消
              </button>
            </div>
          )}
        </div>
      </div>

      {/* 告警表 */}
      <div className="dm-section-card">
        <div className="dm-section-card__header">
          <h3>告警 ({alarms.length})</h3>
          <button type="button" className="dm-btn dm-btn--primary" disabled={patching} onClick={() => setAddingAlarm(true)}>
            <PlusIcon width={14} height={14} />
            新增告警
          </button>
        </div>
        <div className="dm-section-card__body dm-section-card__body--no-pad">
          {alarms.length > 0 ? (
            <table className="dm-table">
              <thead>
                <tr>
                  <th>ID</th>
                  <th>条件</th>
                  <th>级别</th>
                  <th>动作</th>
                  <th />
                </tr>
              </thead>
              <tbody>
                {alarms.map((alarm, i) => (
                  <tr key={String(alarm.id ?? i)}>
                    <td className="dm-mono">{String(alarm.id)}</td>
                    <td>
                      <EditableField value={String(alarm.condition)} label="condition" onSave={(v) => void patch(`/alarms/${i}/condition`, v)} disabled={patching} />
                    </td>
                    <td>
                      <span className={`dm-severity dm-severity--${String(alarm.severity)}`}>
                        {String(alarm.severity)}
                      </span>
                    </td>
                    <td>
                      <EditableField value={alarm.action ? String(alarm.action) : '-'} label="action" onSave={(v) => void patch(`/alarms/${i}/action`, v)} disabled={patching} />
                    </td>
                    <td>
                      <button
                        type="button"
                        className="dm-btn dm-btn--danger"
                        title="删除告警"
                        disabled={patching}
                        onClick={() => void handleRemoveAlarm(i)}
                      >
                        <DeleteActionIcon width={14} height={14} />
                      </button>
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          ) : (
            <div style={{ padding: '16px', textAlign: 'center', color: 'var(--muted)' }}>暂无告警</div>
          )}

          {addingAlarm && (
            <div className="dm-add-row">
              <input
                type="text"
                className="dm-add-row__input"
                placeholder="告警 ID"
                value={newAlarmId}
                onChange={(e) => setNewAlarmId(e.target.value)}
                autoFocus
              />
              <input
                type="text"
                className="dm-add-row__input dm-add-row__input--wide"
                placeholder="条件表达式（如 pressure > 34）"
                value={newAlarmCondition}
                onChange={(e) => setNewAlarmCondition(e.target.value)}
              />
              <button type="button" className="dm-btn dm-btn--primary" disabled={!newAlarmId.trim() || !newAlarmCondition.trim() || patching} onClick={() => void handleAddAlarm()}>
                确认
              </button>
              <button type="button" className="dm-btn" onClick={() => { setAddingAlarm(false); setNewAlarmId(''); setNewAlarmCondition(''); }}>
                取消
              </button>
            </div>
          )}
        </div>
      </div>

      {/* 元数据 */}
      <div className="dm-section-card">
        <div className="dm-section-card__header">
          <h3>元数据</h3>
        </div>
        <div className="dm-section-card__body">
          <div className="dm-meta-grid">
            <span>创建时间</span>
            <span>{formatRelativeTimestamp(detail.created_at)}</span>
            <span>更新时间</span>
            <span>{formatRelativeTimestamp(detail.updated_at)}</span>
          </div>
        </div>
      </div>
    </div>
  );
}
