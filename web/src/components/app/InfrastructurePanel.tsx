//! 全局资产管理面板：设备视图（主）+ 连接视图（基础设施）
//! 设备语义高于协议适配（AGENTS.md 设计原则三）——设备是业务一等公民，连接是传输基础设施。

import { useCallback, useEffect, useMemo, useState } from 'react';

import { ConnectionStudio } from '../ConnectionStudio';
import type { CanvasNodeOp } from '../FlowgramCanvas';
import type { UseConnectionLibraryResult } from '../../hooks/use-connection-library';
import { useDeviceAssets } from '../../hooks/use-device-assets';
import type { DeviceAssetSummary } from '../../hooks/use-device-assets';
import type { ConnectionRecord } from '../../types';
import { DeviceImportDrawer } from './DeviceImportDrawer';
import { DeviceModelingPanel } from './DeviceModelingPanel';

type InfraTab = 'devices' | 'connections';

interface InfrastructurePanelProps {
  isTauriRuntime: boolean;
  workspacePath: string;
  connectionLibrary: UseConnectionLibraryResult;
  usageByConnection: Map<string, { nodeIds: string[]; projectNames: string[] }>;
  runtimeConnections: ConnectionRecord[];
  onStatusMessage: (msg: string) => void;
  onAddCapabilityToCanvas?: (nodeOp: CanvasNodeOp) => void;
}

export function InfrastructurePanel({
  isTauriRuntime,
  workspacePath,
  connectionLibrary,
  usageByConnection,
  runtimeConnections,
  onStatusMessage,
  onAddCapabilityToCanvas,
}: InfrastructurePanelProps) {
  const [activeTab, setActiveTab] = useState<InfraTab>('devices');
  const [focusConnectionId, setFocusConnectionId] = useState<string | null>(null);
  const [importDrawerOpen, setImportDrawerOpen] = useState(false);

  // 共享的设备资产摘要——用于连接 Tab 反查"绑定设备"。
  // 两个 Tab 各自维护状态，切换到连接 Tab 时刷新一次以兜底跨 Tab 编辑造成的过期。
  const { assets: deviceSummaries, loadAssets: loadDeviceSummaries } =
    useDeviceAssets(workspacePath);

  useEffect(() => {
    if (isTauriRuntime) {
      void loadDeviceSummaries();
    }
  }, [isTauriRuntime, loadDeviceSummaries]);

  const devicesByConnectionId = useMemo(() => {
    const map = new Map<string, DeviceAssetSummary[]>();
    for (const device of deviceSummaries) {
      const cid = device.connection?.id?.trim();
      if (!cid) continue;
      const list = map.get(cid) ?? [];
      list.push(device);
      map.set(cid, list);
    }
    return map;
  }, [deviceSummaries]);

  const handleTabChange = useCallback(
    (next: InfraTab) => {
      setActiveTab(next);
      if (next === 'connections' && isTauriRuntime) {
        // 进入连接 Tab 前刷新设备摘要，确保"绑定设备"反查不滞后。
        void loadDeviceSummaries();
      }
      if (next === 'devices') {
        setFocusConnectionId(null);
      }
    },
    [isTauriRuntime, loadDeviceSummaries],
  );

  const handleJumpToConnection = useCallback((connectionId: string) => {
    setFocusConnectionId(connectionId);
    setActiveTab('connections');
  }, []);

  const activeCount = activeTab === 'devices'
    ? deviceSummaries.length
    : connectionLibrary.connections.length;

  return (
    <section className="infra-panel">
      <div
        className="panel__header panel__header--desktop window-safe-header"
        data-window-drag-region
      >
        <nav className="infra-tabs" role="tablist" data-no-window-drag>
          <button
            type="button"
            role="tab"
            data-testid="infra-tab-devices"
            aria-selected={activeTab === 'devices'}
            className={`infra-tabs__item${activeTab === 'devices' ? ' is-active' : ''}`}
            onClick={() => handleTabChange('devices')}
          >
            设备
          </button>
          <button
            type="button"
            role="tab"
            data-testid="infra-tab-connections"
            aria-selected={activeTab === 'connections'}
            className={`infra-tabs__item${activeTab === 'connections' ? ' is-active' : ''}`}
            onClick={() => handleTabChange('connections')}
          >
            连接
          </button>
        </nav>
        <div className="panel__header-actions" data-no-window-drag>
          <span className="panel__badge">
            {activeCount > 0 ? `${activeCount} 项` : '无'}
          </span>
          <button
            type="button"
            className="panel__action"
            data-testid="infra-import-button"
            onClick={() => setImportDrawerOpen(true)}
          >
            接入设备
          </button>
        </div>
      </div>

      <div className="infra-panel__content">
        {activeTab === 'devices' ? (
          <DeviceModelingPanel
            isTauriRuntime={isTauriRuntime}
            workspacePath={workspacePath}
            connections={connectionLibrary.connections}
            runtimeConnections={runtimeConnections}
            onJumpToConnection={handleJumpToConnection}
            onStatusMessage={onStatusMessage}
            onAddCapabilityToCanvas={onAddCapabilityToCanvas}
            hideHeader
          />
        ) : (
          <ConnectionStudio
            connections={connectionLibrary.connections}
            setConnections={connectionLibrary.setConnections}
            usageByConnection={usageByConnection}
            devicesByConnectionId={devicesByConnectionId}
            runtimeConnections={runtimeConnections}
            isLoading={!connectionLibrary.storage.isReady}
            storageError={connectionLibrary.storage.error}
            focusConnectionId={focusConnectionId}
            onConsumeFocus={() => setFocusConnectionId(null)}
            onStatusMessage={onStatusMessage}
            hideHeader
          />
        )}
      </div>

      {importDrawerOpen ? (
        <div className="infra-drawer-backdrop" onClick={() => setImportDrawerOpen(false)}>
          <DeviceImportDrawer
            workspacePath={workspacePath}
            onClose={() => setImportDrawerOpen(false)}
            onSaved={() => { setImportDrawerOpen(false); void loadDeviceSummaries(); }}
            onStatusMessage={onStatusMessage}
          />
        </div>
      ) : null}
    </section>
  );
}
