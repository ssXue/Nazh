import { useCallback, useEffect, useMemo, useState } from 'react';

import { SpotlightCard } from '../animations/SpotlightCard';
import { hasTauriRuntime } from '../../lib/tauri';
import { formatRelativeTimestamp } from '../../lib/projects';
import { useDeviceAssets } from '../../hooks/use-device-assets';
import type { DeviceAssetDetail, DeviceAssetSummary } from '../../hooks/use-device-assets';
import type { ConnectionDefinition, ConnectionRecord } from '../../types';
import {
  SparklesIcon,
  PlusIcon,
  DeleteActionIcon,
  SearchIcon,
  DeviceIcon,
  BackIcon,
  ConnectionsIcon,
} from './AppIcons';
import { ExpandTransition } from './ExpandTransition';
import { DeviceTypeBadge, DeviceConnectionBar, EditableField, buildDeviceConnectionBinding } from './device-modeling-helpers';
import { SignalsTab } from './DeviceSignalsTab';
import { CapabilitiesTab } from './DeviceCapabilitiesTab';
import { SnapshotsTab } from './DeviceSnapshotsTab';

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
              className="asset-card dm-card"
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
              <div className="asset-card__row">
                <div className="asset-card__icon">
                  <DeviceIcon />
                </div>
                <div className="asset-card__row-body">
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

              <div className="asset-card__chips">
                <span className="asset-card__chip">{asset.device_type}</span>
                <span className="asset-card__chip">{`${asset.version} 个快照`}</span>
              </div>

              <div className="asset-card__footer">
                <span className="asset-card__meta">{formatRelativeTimestamp(asset.updated_at)}</span>
                <button
                  type="button"
                  className="asset-card__delete"
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
