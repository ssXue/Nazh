import { useCallback, useEffect, useMemo, useState } from 'react';

import { SpotlightCard } from '../animations/SpotlightCard';
import { hasTauriRuntime } from '../../lib/tauri';
import { formatRelativeTimestamp } from '../../lib/projects';
import { useDeviceAssets } from '../../hooks/use-device-assets';
import type { DeviceAssetDetail, DeviceAssetSummary } from '../../hooks/use-device-assets';
import { useCapabilities } from '../../hooks/use-capabilities';
import type { CapabilitySummary, CapabilityDetail, GeneratedCapability } from '../../hooks/use-capabilities';
import type { ConnectionDefinition, ConnectionRecord } from '../../types';
import { connectionRuntimeState } from '../connection-studio-utils';
import {
  SparklesIcon,
  PlusIcon,
  DeleteActionIcon,
  SearchIcon,
  DeviceIcon,
  BackIcon,
  PencilIcon,
  SnapshotIcon,
  ConnectionsIcon,
} from './AppIcons';
import { ExpandTransition } from './ExpandTransition';

interface DeviceModelingPanelProps {
  isTauriRuntime: boolean;
  workspacePath: string;
  /** 全局连接资源定义，用于在设备卡片/详情显示绑定连接信息。 */
  connections?: ConnectionDefinition[];
  /** 运行时连接快照，用于显示连接健康状态。 */
  runtimeConnections?: ConnectionRecord[];
  /** 设备详情中点击"切换到连接"时回调，由 InfrastructurePanel 处理跨 Tab 跳转。 */
  onJumpToConnection?: (connectionId: string) => void;
  onStatusMessage: (message: string) => void;
  /** 将能力添加到画布的回调（来自 InfrastructurePanel/StudioContentRouter）。 */
  onAddCapabilityToCanvas?: (nodeOp: import('../FlowgramCanvas').CanvasNodeOp) => void;
  /** 由 InfrastructurePanel 传入 true 以跳过 grid view 的 panel__header（外层已统一渲染）。 */
  hideHeader?: boolean;
}

/** 设备绑定的连接摘要——给卡片/详情共用的派生结构。 */
interface DeviceConnectionBinding {
  connectionId: string;
  connectionType: string;
  unit: number | null;
  /** 全局连接库中是否存在该 ID 的定义（true = 已绑定有效连接）。 */
  isResolved: boolean;
  /** 全局连接库中匹配到的定义（解析名称、参数）。 */
  definition: ConnectionDefinition | undefined;
  /** 运行时连接（含 health 快照）。 */
  runtime: ConnectionRecord | undefined;
}

function buildDeviceConnectionBinding(
  asset: DeviceAssetSummary,
  connectionsById: Map<string, ConnectionDefinition>,
  runtimeById: Map<string, ConnectionRecord>,
): DeviceConnectionBinding | null {
  const ref = asset.connection;
  if (!ref?.id?.trim()) {
    return null;
  }
  const cid = ref.id.trim();
  const definition = connectionsById.get(cid);
  return {
    connectionId: cid,
    connectionType: ref.type?.trim() || definition?.type || '',
    unit: typeof ref.unit === 'number' ? ref.unit : null,
    isResolved: Boolean(definition),
    definition,
    runtime: runtimeById.get(cid),
  };
}

export function DeviceModelingPanel({
  isTauriRuntime,
  workspacePath,
  connections = [],
  runtimeConnections = [],
  onJumpToConnection,
  onStatusMessage,
  onAddCapabilityToCanvas,
  hideHeader = false,
}: DeviceModelingPanelProps) {
  const connectionsById = useMemo(
    () => new Map(connections.map((c) => [c.id, c])),
    [connections],
  );
  const runtimeById = useMemo(
    () => new Map(runtimeConnections.map((c) => [c.id, c])),
    [runtimeConnections],
  );
  const {
    assets,
    loading,
    loadAssets,
    loadDetail,
    deleteAsset,
    bindConnection,
  } = useDeviceAssets(workspacePath);

  const [detail, setDetail] = useState<DeviceAssetDetail | null>(null);
  const [detailLoading, setDetailLoading] = useState(false);
  const [searchQuery, setSearchQuery] = useState('');

  const filteredAssets = useMemo(() => {
    if (!searchQuery.trim()) return assets;
    const q = searchQuery.toLowerCase();
    return assets.filter(
      (a) =>
        a.name.toLowerCase().includes(q) ||
        a.device_type.toLowerCase().includes(q) ||
        a.id.toLowerCase().includes(q),
    );
  }, [assets, searchQuery]);

  useEffect(() => {
    void loadAssets();
  }, [loadAssets]);

  const handleOpenDetail = useCallback(
    async (id: string) => {
      setDetail(null);
      setDetailLoading(true);
      try {
        const result = await loadDetail(id);
        setDetail(result);
      } catch (error) {
        onStatusMessage(`加载设备详情失败: ${error}`);
      } finally {
        setDetailLoading(false);
      }
    },
    [loadDetail, onStatusMessage],
  );

  const handleCloseDetail = useCallback(() => {
    setDetail(null);
  }, []);

  const handleDelete = useCallback(
    async (id: string) => {
      try {
        await deleteAsset(id);
        onStatusMessage(`设备 ${id} 已删除`);
        setDetail(null);
      } catch (error) {
        onStatusMessage(`删除设备失败: ${error}`);
      }
    },
    [deleteAsset, onStatusMessage],
  );

  if (!isTauriRuntime) {
    return (
      <div className="device-modeling">
        <div
          className="panel__header panel__header--desktop window-safe-header"
          data-window-drag-region
        >
          <div className="panel__header__heading">
            <h2>设备建模</h2>
            <span data-testid="device-empty-state">设备建模功能需要 Tauri 桌面运行时。</span>
          </div>
        </div>
      </div>
    );
  }

  const showDetail = detail && !detailLoading;

  const gridBase = (
    <div className="dm-grid-view">
      {!hideHeader && (
        <div
          className="panel__header panel__header--desktop window-safe-header"
          data-window-drag-region
        >
          <div className="panel__header__heading">
            <h2>设备资产</h2>
            <span>{assets.length} 个设备资产</span>
          </div>
        </div>
      )}

      {assets.length > 0 && (
        <div className="dm-grid-search" data-no-window-drag>
          <SearchIcon width={14} height={14} />
          <input
            type="text"
            className="dm-grid-search-input"
            placeholder="搜索设备..."
            value={searchQuery}
            onChange={(e) => setSearchQuery(e.target.value)}
          />
        </div>
      )}

      <div className="dm-grid">
        {loading ? (
          <div className="dm-loading-card">加载中...</div>
        ) : filteredAssets.length === 0 && assets.length > 0 ? (
          <div className="dm-empty-card">
            <strong>无匹配设备</strong>
            <span>尝试其他搜索词</span>
          </div>
        ) : (
          filteredAssets.map((asset) => (
            <SpotlightCard
              as="article"
              key={asset.id}
              className="dm-card"
              role="button"
              tabIndex={0}
              onClick={() => void handleOpenDetail(asset.id)}
              onKeyDown={(e) => {
                if (e.key === 'Enter' || e.key === ' ') {
                  e.preventDefault();
                  void handleOpenDetail(asset.id);
                }
              }}
              spotlightColor="rgba(74, 114, 201, 0.06)"
            >
              <div className="dm-card-row">
                <div className="board-card__icon">
                  <DeviceIcon />
                </div>
                <div className="board-card__body">
                  <strong className="board-card__name" title={asset.name}>{asset.name}</strong>
                  <span className="board-card__desc">{asset.device_type}</span>
                </div>
              </div>

              <div className="dm-card-connection-select" onClick={(e) => e.stopPropagation()}>
                <ConnectionsIcon width={12} height={12} />
                <select
                  value={asset.connection?.id ?? ''}
                  onChange={(e) => {
                    const cid = e.target.value;
                    if (!cid) {
                      void bindConnection(asset.id, null, null, null);
                    } else {
                      const def = connectionsById.get(cid);
                      void bindConnection(asset.id, def?.type ?? '', cid, asset.connection?.unit ?? null);
                    }
                  }}
                >
                  <option value="">未绑定</option>
                  {connections.map((c) => (
                    <option key={c.id} value={c.id}>{c.id}</option>
                  ))}
                </select>
              </div>

              <div className="board-card__chips">
                <span className="board-card__chip">{asset.device_type}</span>
                <span className="board-card__chip">{`${asset.version} 个快照`}</span>
              </div>

              <div className="board-card__footer">
                <span className="board-card__meta">{formatRelativeTimestamp(asset.updated_at)}</span>
                <button
                  type="button"
                  className="board-card__delete"
                  aria-label={`删除设备 ${asset.name}`}
                  title={`删除设备 ${asset.name}`}
                  onClick={(e) => {
                    e.stopPropagation();
                    void handleDelete(asset.id);
                  }}
                >
                  <DeleteActionIcon />
                </button>
              </div>
            </SpotlightCard>
          ))
        )}
      </div>
    </div>
  );

  const detailOverlay = detail ? (
    <DetailPanel
      detail={detail}
      workspacePath={workspacePath}
      connectionsById={connectionsById}
      runtimeById={runtimeById}
      onJumpToConnection={onJumpToConnection}
      onReload={() => void handleOpenDetail(detail.id)}
      onBack={handleCloseDetail}
      onDelete={() => void handleDelete(detail.id)}
      onStatusMessage={onStatusMessage}
      onAddCapabilityToCanvas={onAddCapabilityToCanvas}
    />
  ) : null;

  return (
    <div className="device-modeling">
      <ExpandTransition
        active={!!showDetail}
        loading={detailLoading}
        mode="centered"
        base={gridBase}
        overlay={detailOverlay}
      />
    </div>
  );
}

/** 设备类型标记。 */
function DeviceTypeBadge({ type }: { type: string }) {
  const initial = type.charAt(0).toUpperCase();
  return <span className="dm-type-badge">{initial}</span>;
}

/** 设备详情子面板。 */
function DetailPanel({
  detail,
  workspacePath,
  connectionsById,
  runtimeById,
  onJumpToConnection,
  onReload,
  onBack,
  onDelete,
  onStatusMessage,
  onAddCapabilityToCanvas,
}: {
  detail: DeviceAssetDetail;
  workspacePath: string;
  connectionsById: Map<string, ConnectionDefinition>;
  runtimeById: Map<string, ConnectionRecord>;
  onJumpToConnection?: (connectionId: string) => void;
  onReload: () => void;
  onBack: () => void;
  onDelete: () => void;
  onStatusMessage: (msg: string) => void;
  onAddCapabilityToCanvas?: (nodeOp: import('../FlowgramCanvas').CanvasNodeOp) => void;
}) {
  const [tab, setTab] = useState<'signals' | 'capabilities' | 'snapshots'>('signals');
  const [patching, setPatching] = useState(false);
  const { patchField } = useDeviceAssets(workspacePath);

  const spec = detail.spec_json;

  const patch = useCallback(
    async (jsonPath: string, value: string) => {
      setPatching(true);
      try {
        await patchField(detail.id, jsonPath, value);
        onStatusMessage('已更新');
        onReload();
      } catch (error) {
        onStatusMessage(`更新失败: ${error}`);
      } finally {
        setPatching(false);
      }
    },
    [detail.id, patchField, onReload, onStatusMessage],
  );

  const modelName = String((spec?.model as string | undefined) ?? spec?.id ?? detail.name ?? detail.id);
  const manufacturer = spec?.manufacturer as string | undefined;

  return (
    <div className="dm-detail-dialog">
      {/* 头部 */}
      <div className="dm-detail-header">
        <div className="dm-detail-header__left">
          <button
            type="button"
            className="dm-back-btn"
            onClick={onBack}
            title="返回设备列表"
          >
            <BackIcon />
          </button>
          <DeviceTypeBadge type={String(spec?.type ?? detail.device_type)} />
          <div className="dm-detail-header__info">
            <EditableField
              value={modelName}
              label="model"
              onSave={(v) => void patch('/model', v)}
              disabled={patching}
              className="dm-detail-header__title"
            />
            <div className="dm-detail-header__badges">
              <span className="dm-badge dm-badge--type">
                {String(spec?.type ?? detail.device_type)}
              </span>
              <span className="dm-badge dm-badge--version">
                {detail.version} 个快照
              </span>
              {manufacturer && (
                <EditableField
                  value={manufacturer}
                  label="manufacturer"
                  onSave={(v) => void patch('/manufacturer', v)}
                  disabled={patching}
                  className="dm-badge dm-badge--meta"
                />
              )}
              {(spec?.id as string | undefined) && (
                <span className="dm-badge dm-badge--meta">
                  {spec.id as string}
                </span>
              )}
              {(spec?.network_group as string | undefined) && (
                <span className="dm-badge dm-badge--meta">
                  {(spec.network_group as string)}
                </span>
              )}
            </div>
          </div>
        </div>
        <button
          type="button"
          className="dm-btn dm-btn--danger"
          title="删除设备"
          onClick={onDelete}
        >
          <DeleteActionIcon width={16} height={16} />
        </button>
      </div>

      {/* 绑定连接栏（置顶，物理链路一目了然） */}
      <DeviceConnectionBar
        detail={detail}
        connectionsById={connectionsById}
        runtimeById={runtimeById}
        onJumpToConnection={onJumpToConnection}
      />

      {/* Tab 切换 */}
      <div className="dm-tabs">
        <button
          type="button"
          className={`dm-tabs__item${tab === 'signals' ? ' is-active' : ''}`}
          onClick={() => setTab('signals')}
        >
          信号
        </button>
        <button
          type="button"
          className={`dm-tabs__item${tab === 'capabilities' ? ' is-active' : ''}`}
          onClick={() => setTab('capabilities')}
        >
          能力
        </button>
        <button
          type="button"
          className={`dm-tabs__item${tab === 'snapshots' ? ' is-active' : ''}`}
          onClick={() => setTab('snapshots')}
        >
          快照
        </button>
      </div>

      <div className="dm-detail-dialog__body">
        {tab === 'signals' ? (
          <SignalsTab detail={detail} workspacePath={workspacePath} onReload={onReload} onStatusMessage={onStatusMessage} />
        ) : tab === 'capabilities' ? (
          <CapabilitiesTab deviceId={detail.id} workspacePath={workspacePath} onAddToCanvas={onAddCapabilityToCanvas} />
        ) : (
          <SnapshotsTab deviceId={detail.id} workspacePath={workspacePath} onReload={onReload} onStatusMessage={onStatusMessage} />
        )}
      </div>
    </div>
  );
}

/** 信号 Tab。通过后端命令增删改字段。 */
function SignalsTab({
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

/** 能力 Tab。 */
function CapabilitiesTab({ deviceId, workspacePath, onAddToCanvas }: {
  deviceId: string;
  workspacePath: string;
  onAddToCanvas?: (nodeOp: import('../FlowgramCanvas').CanvasNodeOp) => void;
}) {
  const {
    capabilities,
    loading,
    loadCapabilities,
    loadDetail,
    deleteCapability,
    generateFromDevice,
    saveCapability,
  } = useCapabilities(workspacePath);

  const [selectedCapId, setSelectedCapId] = useState<string | null>(null);
  const [capDetail, setCapDetail] = useState<CapabilityDetail | null>(null);
  const [generated, setGenerated] = useState<GeneratedCapability[]>([]);
  const [generating, setGenerating] = useState(false);

  useEffect(() => {
    void loadCapabilities(deviceId);
  }, [deviceId, loadCapabilities]);

  const handleSelectCap = useCallback(
    async (id: string) => {
      setSelectedCapId(id);
      setCapDetail(null);
      try {
        const result = await loadDetail(id);
        setCapDetail(result);
      } catch {
        /* 忽略加载错误 */
      }
    },
    [loadDetail],
  );

  const handleGenerate = async () => {
    setGenerating(true);
    try {
      const result = await generateFromDevice(deviceId);
      setGenerated(result);
    } catch {
      /* 忽略生成错误 */
    } finally {
      setGenerating(false);
    }
  };

  const handleSaveGenerated = async (gen: GeneratedCapability) => {
    try {
      const name = gen.capability_id.split('.').pop() ?? gen.capability_id;
      await saveCapability(gen.capability_id, deviceId, name, '自动生成', gen.capability_yaml);
      setGenerated((prev) => prev.filter((g) => g.capability_id !== gen.capability_id));
    } catch {
      /* 忽略保存错误 */
    }
  };

  const handleDeleteCap = async (id: string) => {
    try {
      await deleteCapability(id, deviceId);
      if (selectedCapId === id) {
        setSelectedCapId(null);
        setCapDetail(null);
      }
    } catch {
      /* 忽略删除错误 */
    }
  };

  const spec = capDetail?.spec_json;
  const inputs = spec?.inputs as Array<Record<string, unknown>> | undefined;
  const preconditions = spec?.preconditions as string[] | undefined;
  const effects = spec?.effects as string[] | undefined;
  const fallback = spec?.fallback as string[] | undefined;
  const safety = spec?.safety as Record<string, unknown> | undefined;
  const implementation = spec?.implementation as Record<string, unknown> | undefined;

  return (
    <div className="dm-detail-body">
      {/* 标签行 */}
      <div className="dm-cap-tags">
        {loading && <span className="dm-meta">加载中...</span>}
        {!loading && capabilities.length === 0 && generated.length === 0 && (
          <span className="dm-meta">暂无能力</span>
        )}
        {capabilities.map((cap) => (
          <button
            key={cap.id}
            type="button"
            className={`dm-cap-tag${selectedCapId === cap.id ? ' is-active' : ''}`}
            onClick={() => void handleSelectCap(cap.id)}
            title={cap.description || cap.name}
          >
            {cap.name}
          </button>
        ))}
        {generated.map((gen) => (
          <span key={gen.capability_id} className="dm-cap-tag dm-cap-tag--generated">
            {gen.capability_id.split('.').pop()}
            <button
              type="button"
              className="dm-cap-tag__save"
              onClick={() => void handleSaveGenerated(gen)}
              title="保存"
            >
              保存
            </button>
          </span>
        ))}
        <button
          type="button"
          className="dm-cap-tag dm-cap-tag--action"
          onClick={() => void handleGenerate()}
          disabled={generating}
          title="从设备信号自动生成能力"
        >
          {generating ? '生成中...' : '生成'}
        </button>
      </div>

      {/* 能力详情 */}
      {!capDetail ? (
        selectedCapId === null && !loading && capabilities.length > 0 && (
          <div className="dm-section-card">
            <div className="dm-section-card__body" style={{ textAlign: 'center', padding: '24px', color: 'var(--muted)' }}>
              点击上方能力标签查看详情
            </div>
          </div>
        )
      ) : (
        <div className="dm-detail-body" style={{ padding: 0, gap: 8 }}>
          {/* 头部 */}
          <div className="dm-detail-header" style={{ padding: '0 0 12px', borderBottom: '1px solid var(--panel-border)' }}>
            <div className="dm-detail-header__left">
              <DeviceTypeBadge type="C" />
              <div className="dm-detail-header__info">
                <h2>{capDetail.name}</h2>
                <div className="dm-detail-header__badges">
                  {implementation && (
                    <span className={`dm-badge dm-badge--type ${implTypeBadgeClass(String(implementation.type ?? ''))}`}>
                      {String(implementation.type ?? '-')}
                    </span>
                  )}
                  {safety && (
                    <span className={`dm-cap-safety-badge dm-cap-safety-badge--${String(safety.level ?? 'low')}`}>
                      {String(safety.level ?? 'low')}
                    </span>
                  )}
                  <span className="dm-badge dm-badge--version">{capDetail.version} 个快照</span>
                  {capDetail.description && (
                    <span className="dm-badge dm-badge--meta">{capDetail.description}</span>
                  )}
                </div>
              </div>
            </div>
            {onAddToCanvas ? (
              <button
                type="button"
                className="dm-btn dm-btn--ghost"
                title="添加到画布"
                onClick={() => {
                  const impl = capDetail.spec_json?.implementation as Record<string, unknown> | undefined;
                  onAddToCanvas({
                    id: `capability_call_${capDetail.id.replace(/[^a-zA-Z0-9_-]/g, '_')}`,
                    type: 'capabilityCall',
                    label: capDetail.name,
                    config: {
                      capability_id: capDetail.id,
                      device_id: deviceId,
                      implementation: impl ?? { type: 'script', content: 'payload' },
                      args: {},
                    },
                  });
                }}
              >
                添加到画布
              </button>
            ) : null}
            <button
              type="button"
              className="dm-btn dm-btn--danger"
              title="删除能力"
              onClick={() => void handleDeleteCap(capDetail.id)}
            >
              <DeleteActionIcon width={14} height={14} />
            </button>
          </div>

          {/* 输入参数 */}
          {inputs && inputs.length > 0 && (
            <div className="dm-section-card">
              <div className="dm-section-card__header">
                <h3>输入参数 ({inputs.length})</h3>
              </div>
              <div className="dm-section-card__body dm-section-card__body--no-pad">
                <table className="dm-table">
                  <thead>
                    <tr>
                      <th>ID</th>
                      <th>单位</th>
                      <th>量程</th>
                      <th>必填</th>
                    </tr>
                  </thead>
                  <tbody>
                    {inputs.map((inp, i) => (
                      <tr key={String(inp.id ?? i)}>
                        <td className="dm-mono">{String(inp.id)}</td>
                        <td>{inp.unit ? String(inp.unit) : '-'}</td>
                        <td className="dm-mono">
                          {inp.range
                            ? `[${(inp.range as { min: number; max: number }).min}, ${(inp.range as { min: number; max: number }).max}]`
                            : '-'}
                        </td>
                        <td>
                          {inp.required ? (
                            <>
                              <span className="dm-required-dot" />
                              必填
                            </>
                          ) : (
                            '可选'
                          )}
                        </td>
                      </tr>
                    ))}
                  </tbody>
                </table>
              </div>
            </div>
          )}

          {/* 实现 */}
          {implementation && (
            <div className="dm-section-card">
              <div className="dm-section-card__header">
                <h3>实现</h3>
              </div>
              <div className="dm-section-card__body">
                <div className="dm-impl-grid">
                  {implementation.register != null && (
                    <>
                      <span className="dm-meta">寄存器</span>
                      <span className="dm-mono">{String(implementation.register)}</span>
                    </>
                  )}
                  {implementation.topic != null && (
                    <>
                      <span className="dm-meta">主题</span>
                      <span className="dm-mono">{String(implementation.topic)}</span>
                    </>
                  )}
                  {(implementation.value ?? implementation.payload ?? implementation.command ?? implementation.content) != null && (
                    <>
                      <span className="dm-meta">值</span>
                      <span className="dm-mono">
                        {String(implementation.value ?? implementation.payload ?? implementation.command ?? implementation.content)}
                      </span>
                    </>
                  )}
                </div>
              </div>
            </div>
          )}

          {/* 安全约束 */}
          {safety && (
            <div className="dm-section-card">
              <div className="dm-section-card__header">
                <h3>安全约束</h3>
              </div>
              <div className="dm-section-card__body">
                <div className="dm-safety-row">
                  <span className={`dm-cap-safety-badge dm-cap-safety-badge--${String(safety.level ?? 'low')}`}>
                    {String(safety.level ?? 'low')}
                  </span>
                  {Boolean(safety.requires_approval) && (
                    <span className="dm-meta">需要审批</span>
                  )}
                  {safety.max_execution_time != null && (
                    <span className="dm-meta">最大执行时间: {String(safety.max_execution_time)}</span>
                  )}
                </div>
              </div>
            </div>
          )}

          {/* 前置条件 */}
          {preconditions && preconditions.length > 0 && (
            <div className="dm-section-card">
              <div className="dm-section-card__header">
                <h3>前置条件 ({preconditions.length})</h3>
              </div>
              <div className="dm-section-card__body">
                <ul className="dm-condition-list">
                  {preconditions.map((cond, i) => (
                    <li key={i} className="dm-condition-item">
                      {String(cond)}
                    </li>
                  ))}
                </ul>
              </div>
            </div>
          )}

          {/* 副作用 */}
          {effects && effects.length > 0 && (
            <div className="dm-section-card">
              <div className="dm-section-card__header">
                <h3>副作用 ({effects.length})</h3>
              </div>
              <div className="dm-section-card__body">
                <ul className="dm-condition-list">
                  {effects.map((eff, i) => (
                    <li key={i} className="dm-condition-item">
                      {String(eff)}
                    </li>
                  ))}
                </ul>
              </div>
            </div>
          )}

          {/* 后备能力 */}
          {fallback && fallback.length > 0 && (
            <div className="dm-section-card">
              <div className="dm-section-card__header">
                <h3>后备能力 ({fallback.length})</h3>
              </div>
              <div className="dm-section-card__body">
                <ul className="dm-condition-list">
                  {fallback.map((fb, i) => (
                    <li key={i} className="dm-condition-item">
                      {String(fb)}
                    </li>
                  ))}
                </ul>
              </div>
            </div>
          )}
        </div>
      )}
    </div>
  );
}

/** 快照 Tab。 */
function SnapshotsTab({
  deviceId,
  workspacePath,
  onReload,
  onStatusMessage,
}: {
  deviceId: string;
  workspacePath: string;
  onReload: () => void;
  onStatusMessage: (msg: string) => void;
}) {
  const { listSnapshots, createSnapshot, rollbackSnapshot, deleteSnapshot } = useDeviceAssets(workspacePath);
  const [snapshots, setSnapshots] = useState<Array<{
    version: number;
    label: string;
    description: string;
    reason: string;
    created_at: string;
  }>>([]);
  const [loading, setLoading] = useState(true);
  const [creating, setCreating] = useState(false);
  const [newLabel, setNewLabel] = useState('');

  const loadSnapshots = useCallback(async () => {
    setLoading(true);
    try {
      const result = await listSnapshots(deviceId);
      setSnapshots(result);
    } catch {
      /* 忽略 */
    } finally {
      setLoading(false);
    }
  }, [deviceId, listSnapshots]);

  useEffect(() => {
    void loadSnapshots();
  }, [loadSnapshots]);

  const handleCreate = useCallback(async () => {
    setCreating(true);
    try {
      await createSnapshot(deviceId, newLabel.trim() || undefined);
      onStatusMessage('快照已创建');
      setNewLabel('');
      await loadSnapshots();
      onReload();
    } catch (error) {
      onStatusMessage(`创建快照失败: ${error}`);
    } finally {
      setCreating(false);
    }
  }, [deviceId, newLabel, createSnapshot, loadSnapshots, onReload, onStatusMessage]);

  const handleRollback = useCallback(async (version: number) => {
    try {
      await rollbackSnapshot(deviceId, version);
      onStatusMessage(`已回滚到快照 v${version}`);
      await loadSnapshots();
      onReload();
    } catch (error) {
      onStatusMessage(`回滚失败: ${error}`);
    }
  }, [deviceId, rollbackSnapshot, loadSnapshots, onReload, onStatusMessage]);

  const handleDelete = useCallback(async (version: number) => {
    try {
      await deleteSnapshot(deviceId, version);
      await loadSnapshots();
    } catch (error) {
      onStatusMessage(`删除快照失败: ${error}`);
    }
  }, [deviceId, deleteSnapshot, loadSnapshots, onStatusMessage]);

  const reasonLabel = (reason: string): string => {
    switch (reason) {
      case 'seed': return '初始';
      case 'manual': return '手动';
      case 'import': return '导入';
      case 'edit': return '编辑';
      case 'rollback': return '保护';
      default: return reason;
    }
  };

  return (
    <div className="dm-detail-body">
      {/* 创建快照 */}
      <div className="dm-section-card">
        <div className="dm-section-card__header">
          <h3>创建快照</h3>
        </div>
        <div className="dm-section-card__body">
          <div className="dm-snapshot-create">
            <input
              type="text"
              className="dm-snapshot-create__input"
              placeholder="快照标签（可选）"
              value={newLabel}
              onChange={(e) => setNewLabel(e.target.value)}
              disabled={creating}
            />
            <button
              type="button"
              className="dm-btn dm-btn--primary"
              disabled={creating}
              onClick={() => void handleCreate()}
            >
              <SnapshotIcon width={14} height={14} />
              {creating ? '创建中...' : '创建'}
            </button>
          </div>
        </div>
      </div>

      {/* 快照列表 */}
      <div className="dm-section-card">
        <div className="dm-section-card__header">
          <h3>历史快照 ({snapshots.length})</h3>
        </div>
        <div className="dm-section-card__body dm-section-card__body--no-pad">
          {loading ? (
            <div style={{ padding: '16px', textAlign: 'center', color: 'var(--muted)' }}>加载中...</div>
          ) : snapshots.length === 0 ? (
            <div style={{ padding: '16px', textAlign: 'center', color: 'var(--muted)' }}>暂无快照</div>
          ) : (
            <div className="dm-snapshot-list">
              {snapshots.map((snap) => (
                <div key={snap.version} className="dm-snapshot-card">
                  <div className="dm-snapshot-card__info">
                    <strong>{snap.label}</strong>
                    {snap.description && <span>{snap.description}</span>}
                  </div>
                  <div className="dm-snapshot-card__meta">
                    <em className={`dm-snapshot-reason dm-snapshot-reason--${snap.reason}`}>
                      {reasonLabel(snap.reason)}
                    </em>
                    <span>v{snap.version}</span>
                    <span>{formatRelativeTimestamp(snap.created_at)}</span>
                  </div>
                  <div className="dm-snapshot-card__actions">
                    <button
                      type="button"
                      className="dm-btn"
                      title="回滚到此快照"
                      onClick={() => void handleRollback(snap.version)}
                    >
                      回滚
                    </button>
                    <button
                      type="button"
                      className="dm-btn dm-btn--danger"
                      title="删除此快照元数据"
                      onClick={() => void handleDelete(snap.version)}
                    >
                      <DeleteActionIcon width={14} height={14} />
                    </button>
                  </div>
                </div>
              ))}
            </div>
          )}
        </div>
      </div>
    </div>
  );
}

// ---- 辅助函数 ----

/** 可内联编辑的文本字段。点击后进入编辑模式，Enter/失焦保存，Esc 取消。 */
function EditableField({
  value,
  label,
  onSave,
  disabled,
  className,
}: {
  value: string;
  label: string;
  onSave: (newValue: string) => void;
  disabled?: boolean;
  className?: string;
}) {
  const [editing, setEditing] = useState(false);
  const [draft, setDraft] = useState(value);

  if (!editing) {
    return (
      <span
        className={`dm-editable${className ? ` ${className}` : ''}`}
        role="button"
        tabIndex={0}
        title={value}
        onClick={() => { setDraft(value); setEditing(true); }}
        onKeyDown={(e) => {
          if (e.key === 'Enter') { setDraft(value); setEditing(true); }
        }}
      >
        {value}
        <PencilIcon width={12} height={12} />
      </span>
    );
  }

  return (
    <input
      className="dm-editable__input"
      type="text"
      value={draft}
      disabled={disabled}
      autoFocus
      onChange={(e) => setDraft(e.target.value)}
      onBlur={() => {
        if (draft.trim() && draft !== value) {
          onSave(draft.trim());
        }
        setEditing(false);
      }}
      onKeyDown={(e) => {
        if (e.key === 'Enter') {
          if (draft.trim() && draft !== value) {
            onSave(draft.trim());
          }
          setEditing(false);
        } else if (e.key === 'Escape') {
          setEditing(false);
        }
      }}
    />
  );
}

function formatSignalType(t: string): string {
  return t.replace(/_/g, ' ');
}

/** 实现 type → CSS class 后缀。 */
function implTypeBadgeClass(type: string): string {
  const lower = type.toLowerCase();
  if (lower.includes('read')) return 'dm-badge--impl-read';
  if (lower.includes('write')) return 'dm-badge--impl-write';
  if (lower.includes('control')) return 'dm-badge--impl-control';
  return '';
}

/** 设备卡片上的连接绑定行（紧凑视图）。 */
/** 设备详情顶部的绑定连接栏（置顶展示物理链路 + 切换连接入口）。 */
function DeviceConnectionBar({
  detail,
  connectionsById,
  runtimeById,
  onJumpToConnection,
}: {
  detail: DeviceAssetDetail;
  connectionsById: Map<string, ConnectionDefinition>;
  runtimeById: Map<string, ConnectionRecord>;
  onJumpToConnection?: (connectionId: string) => void;
}) {
  const conn = detail.spec_json?.connection as
    | { type?: string; id?: string; unit?: number }
    | undefined;
  if (!conn?.id?.trim()) {
    return (
      <div className="dm-detail-connection-bar dm-detail-connection-bar--missing">
        <ConnectionsIcon width={14} height={14} />
        <span>该设备尚未绑定连接，下方信号表的源会因此无法读取。</span>
      </div>
    );
  }
  const cid = conn.id.trim();
  const definition = connectionsById.get(cid);
  const runtime = runtimeById.get(cid);
  const runtimeState = connectionRuntimeState(runtime);
  const isResolved = Boolean(definition);

  return (
    <div className={`dm-detail-connection-bar${isResolved ? '' : ' dm-detail-connection-bar--unresolved'}`}>
      <div className="dm-detail-connection-bar__head">
        <ConnectionsIcon width={14} height={14} />
        <span className="dm-detail-connection-bar__type">{conn.type || definition?.type || '未知协议'}</span>
        <span className="dm-detail-connection-bar__id">{cid}</span>
        {conn.unit != null && (
          <span className="dm-detail-connection-bar__unit">站号 {conn.unit}</span>
        )}
      </div>
      <div className="dm-detail-connection-bar__tail">
        <span className={`connection-status is-${runtimeState.state}`}>
          <span className="connection-status__dot" />
          {runtimeState.label}
        </span>
        {!isResolved && (
          <span className="dm-detail-connection-bar__hint">
            未在全局连接库中找到该 ID，先在连接 Tab 中创建。
          </span>
        )}
        {onJumpToConnection && (
          <button
            type="button"
            className="dm-btn dm-btn--ghost"
            onClick={() => onJumpToConnection(cid)}
            title="切换到连接 Tab 并定位此连接"
          >
            前往连接
          </button>
        )}
      </div>
    </div>
  );
}
