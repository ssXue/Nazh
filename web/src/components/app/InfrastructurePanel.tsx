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

  return (
    <section className="infra-panel">
      <nav className="infra-tab-bar" aria-label="资产管理视图切换">
        <button
          type="button"
          className={`infra-tab${activeTab === 'devices' ? ' is-active' : ''}`}
          onClick={() => handleTabChange('devices')}
        >
          设备
          {deviceSummaries.length > 0 && (
            <span className="infra-tab__badge">{deviceSummaries.length}</span>
          )}
        </button>
        <button
          type="button"
          className={`infra-tab${activeTab === 'connections' ? ' is-active' : ''}`}
          onClick={() => handleTabChange('connections')}
        >
          连接
          {connectionLibrary.connections.length > 0 && (
            <span className="infra-tab__badge">{connectionLibrary.connections.length}</span>
          )}
        </button>
        <div className="infra-tab-bar__spacer" />
        <button
          type="button"
          className="infra-tab-bar__action"
          onClick={() => setImportDrawerOpen(true)}
        >
          接入设备
        </button>
      </nav>

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
