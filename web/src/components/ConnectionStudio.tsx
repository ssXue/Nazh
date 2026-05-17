/**
 * 连接资源编辑器主组件。
 *
 * 管理全局状态、事件处理和布局框架，
 * 渲染子组件：ConnectionCardGrid（卡片列表）、ConnectionForm（编辑表单）、
 * ConnectionTestPanel（健康面板）。
 *
 * 公共接口：export function ConnectionStudio —— 仅此一个导出。
 */

import {
  useEffect,
  useMemo,
  useState,
} from 'react';

import type { ConnectionDefinition, JsonValue } from '../types';
import { ExpandTransition } from './app/ExpandTransition';

import {
  listSerialPorts,
  listNetworkInterfaces,
  resetConnectionCircuitBreaker,
  testSerialConnection,
  type SerialPortInfo,
  type TestSerialResult,
  type NetworkInterfaceInfo,
} from '../lib/tauri';

// 从拆分模块导入
import {
  CONNECTION_TEMPLATES,
  connectionKey,
  formatMetadata,
  governanceRecord,
  isSerialConnectionType,
  metadataNumber,
  metadataRecord,
  metadataString,
} from './connection-studio-utils';
import type { ConnectionTemplate } from './connection-studio-utils';

// 从拆分子组件导入
import { ConnectionCardGrid } from './ConnectionCard';
import { ConnectionForm } from './ConnectionForm';
import { ConnectionTestPanel } from './ConnectionTestPanel';
import { DeleteActionIcon } from './app/AppIcons';

import type { ConnectionStudioProps } from './connection-utils';

// ---------------------------------------------------------------------------
// 主组件
// ---------------------------------------------------------------------------

export function ConnectionStudio({
  connections,
  setConnections,
  usageByConnection,
  runtimeConnections,
  devicesByConnectionId,
  focusConnectionId = null,
  onConsumeFocus,
  isLoading = false,
  storageError,
  onStatusMessage,
  hideHeader = false,
}: ConnectionStudioProps) {
  const runtimeById = useMemo(
    () => new Map(runtimeConnections.map((connection) => [connection.id, connection])),
    [runtimeConnections],
  );
  const duplicateConnectionIds = useMemo(() => {
    const counts = new Map<string, number>();
    for (const connection of connections) {
      const id = connection.id.trim();
      if (!id) {
        continue;
      }
      counts.set(id, (counts.get(id) ?? 0) + 1);
    }

    return new Set(
      [...counts.entries()].filter(([, count]) => count > 1).map(([connectionId]) => connectionId),
    );
  }, [connections]);

  const [idDrafts, setIdDrafts] = useState<Record<string, string>>({});
  const [metadataDrafts, setMetadataDrafts] = useState<Record<string, string>>({});
  const [metadataErrors, setMetadataErrors] = useState<Record<string, string>>({});
  const [activeConnectionIndex, setActiveConnectionIndex] = useState<number | null>(null);
  const [pendingDeleteIndex, setPendingDeleteIndex] = useState<number | null>(null);
  const [scannedPorts, setScannedPorts] = useState<SerialPortInfo[]>([]);
  const [isScanningPorts, setIsScanningPorts] = useState(false);
  const [scannedInterfaces, setScannedInterfaces] = useState<NetworkInterfaceInfo[]>([]);
  const [isScanningInterfaces, setIsScanningInterfaces] = useState(false);
  const [testResult, setTestResult] = useState<TestSerialResult | null>(null);
  const [isTesting, setIsTesting] = useState(false);
  const [isAdvancedOpen, setIsAdvancedOpen] = useState(false);
  const [isResettingCircuit, setIsResettingCircuit] = useState(false);

  // ---- 副作用：初始化草案 ----
  useEffect(() => {
    if (isLoading) {
      return;
    }

    setIdDrafts(
      Object.fromEntries(
        connections.map((connection, index) => [connectionKey(index), connection.id]),
      ),
    );
    setMetadataDrafts(
      Object.fromEntries(
        connections.map((connection, index) => [
          connectionKey(index),
          formatMetadata(connection.metadata),
        ]),
      ),
    );
    setMetadataErrors({});
  }, [connections, isLoading]);

  // ---- 副作用：活跃索引越界修正 ----
  useEffect(() => {
    if (isLoading || activeConnectionIndex === null) {
      return;
    }

    if (activeConnectionIndex >= connections.length) {
      setActiveConnectionIndex(connections.length > 0 ? connections.length - 1 : null);
    }
  }, [activeConnectionIndex, connections.length, isLoading]);

  // ---- 副作用：跨 Tab 跳转（设备 Tab "前往连接"） ----
  useEffect(() => {
    if (!focusConnectionId || isLoading || connections.length === 0) {
      return;
    }
    const index = connections.findIndex((c) => c.id === focusConnectionId);
    if (index >= 0) {
      setActiveConnectionIndex(index);
    } else {
      onStatusMessage(`连接 ${focusConnectionId} 尚未在全局连接库中创建。`);
    }
    onConsumeFocus?.();
  }, [focusConnectionId, isLoading, connections, onConsumeFocus, onStatusMessage]);

  // ---- 副作用：切换卡片时清除删除确认 ----
  useEffect(() => {
    if (activeConnectionIndex === null) {
      setPendingDeleteIndex(null);
      return;
    }

    if (pendingDeleteIndex !== null && pendingDeleteIndex !== activeConnectionIndex) {
      setPendingDeleteIndex(null);
    }
  }, [activeConnectionIndex, pendingDeleteIndex]);

  // ---- 副作用：Escape 关闭面板 ----
  useEffect(() => {
    if (activeConnectionIndex === null) {
      return;
    }

    function handleKeyDown(event: KeyboardEvent) {
      if (event.key === 'Escape') {
        setActiveConnectionIndex(null);
      }
    }

    window.addEventListener('keydown', handleKeyDown);
    return () => window.removeEventListener('keydown', handleKeyDown);
  }, [activeConnectionIndex]);

  // ---- 副作用：串口连接打开时自动扫描端口 ----
  useEffect(() => {
    if (activeConnectionIndex === null) {
      setScannedPorts([]);
      setTestResult(null);
      return;
    }

    const connection = connections[activeConnectionIndex];
    if (!connection || !isSerialConnectionType(connection.type)) {
      setScannedPorts([]);
      setTestResult(null);
      return;
    }

    setIsScanningPorts(true);
    listSerialPorts()
      .then((ports) => {
        setScannedPorts(ports);
      })
      .catch(() => {
        setScannedPorts([]);
      })
      .finally(() => {
        setIsScanningPorts(false);
      });

    setTestResult(null);
  }, [activeConnectionIndex, connections]);

  // ---- 事件处理函数 ----

  function buildNextConnectionId(prefix: string): string {
    const existingIds = new Set(connections.map((connection) => connection.id));
    let index = 1;
    while (existingIds.has(`${prefix}_${index}`)) {
      index += 1;
    }

    return `${prefix}_${index}`;
  }

  function saveConnections(definitions: ConnectionDefinition[], message: string) {
    setConnections(definitions);
    onStatusMessage(message);
  }

  function replaceConnectionMetadata(
    index: number,
    nextMetadata: Record<string, JsonValue>,
    message: string,
  ) {
    const draftKey = connectionKey(index);
    const nextConnections = connections.map((connection, connectionIndex) =>
      connectionIndex === index
        ? {
            ...connection,
            metadata: nextMetadata,
          }
        : connection,
    );

    setMetadataDrafts((current) => ({
      ...current,
      [draftKey]: formatMetadata(nextMetadata),
    }));
    setMetadataErrors((current) => {
      const nextErrors = { ...current };
      delete nextErrors[draftKey];
      return nextErrors;
    });

    saveConnections(nextConnections, message);
  }

  function handleAddConnection(template: ConnectionTemplate) {
    const nextConnection: ConnectionDefinition = {
      ...template.definition,
      id: buildNextConnectionId(template.idPrefix),
      metadata: template.definition.metadata ?? {},
    };

    saveConnections([...connections, nextConnection], `已新增 ${template.label} 连接`);
    setActiveConnectionIndex(connections.length);
  }

  function handleRemoveConnection(index: number) {
    const target = connections[index];
    if (!target) {
      return;
    }

    const nextConnections = connections.filter((_, connectionIndex) => connectionIndex !== index);
    const usage = target.id
      ? usageByConnection.get(target.id) ?? { nodeIds: [], projectNames: [] }
      : { nodeIds: [], projectNames: [] };

    if (pendingDeleteIndex !== index) {
      setPendingDeleteIndex(index);
      onStatusMessage(
        usage.nodeIds.length > 0
          ? `连接 ${target.id || '未命名连接'} 仍被 ${usage.projectNames.length} 个工程、${usage.nodeIds.length} 个节点引用，再点一次确认删除。`
          : `再次点击确认删除连接 ${target.id || '未命名连接'}。`,
      );
      return;
    }

    const message = usage.nodeIds.length > 0
      ? `已删除连接 ${target.id}（${usage.projectNames.length} 个工程仍引用此连接）`
      : `已删除连接 ${target.id || '未命名连接'}`;

    saveConnections(nextConnections, message);
    setPendingDeleteIndex(null);
    setActiveConnectionIndex((current) => {
      if (current === null) {
        return null;
      }
      if (current === index) {
        return null;
      }
      return current > index ? current - 1 : current;
    });
  }

  function handleTypeChange(index: number, value: string) {
    const nextConnections = connections.map((connection, connectionIndex) =>
      connectionIndex === index
        ? {
            ...connection,
            type: value,
          }
        : connection,
    );

    saveConnections(nextConnections, '连接类型已更新。');
  }

  function commitConnectionId(index: number) {
    const currentConnection = connections[index];
    if (!currentConnection) {
      return;
    }

    const draftKey = connectionKey(index);
    const nextId = (idDrafts[draftKey] ?? currentConnection.id).trim();
    if (nextId === currentConnection.id) {
      return;
    }

    const nextConnections = connections.map((connection, connectionIndex) =>
      connectionIndex === index
        ? {
            ...connection,
            id: nextId,
          }
        : connection,
    );

    saveConnections(nextConnections, `连接 ID 已更新为 ${nextId || '空值'}。`);
  }

  function handleMetadataChange(index: number, value: string) {
    const draftKey = connectionKey(index);
    setMetadataDrafts((current) => ({
      ...current,
      [draftKey]: value,
    }));

    try {
      const parsed = JSON.parse(value) as JsonValue;
      const nextConnections = connections.map((connection, connectionIndex) =>
        connectionIndex === index
          ? {
              ...connection,
              metadata: parsed,
            }
          : connection,
      );

      setMetadataErrors((current) => {
        const nextErrors = { ...current };
        delete nextErrors[draftKey];
        return nextErrors;
      });

      saveConnections(nextConnections, '连接元数据已更新。');
    } catch (error) {
      setMetadataErrors((current) => ({
        ...current,
        [draftKey]: error instanceof Error ? error.message : 'Metadata JSON 解析失败',
      }));
    }
  }

  function handleMetadataFieldChange(index: number, key: string, value: JsonValue) {
    const currentConnection = connections[index];
    if (!currentConnection) {
      return;
    }

    const nextMetadata = {
      ...metadataRecord(currentConnection.metadata),
      [key]: value,
    };
    replaceConnectionMetadata(index, nextMetadata, '连接参数已更新。');
  }

  function handleGovernanceFieldChange(
    index: number,
    key: string,
    value: number,
  ) {
    const currentConnection = connections[index];
    if (!currentConnection) {
      return;
    }

    const nextMetadata = {
      ...metadataRecord(currentConnection.metadata),
      governance: {
        ...governanceRecord(currentConnection.metadata),
        [key]: value,
      },
    };

    replaceConnectionMetadata(index, nextMetadata, '连接治理策略已更新。');
  }

  async function handleResetCircuitBreaker() {
    if (activeConnectionIndex === null) {
      return;
    }
    const connection = connections[activeConnectionIndex];
    if (!connection?.id) {
      return;
    }

    setIsResettingCircuit(true);
    try {
      await resetConnectionCircuitBreaker(connection.id);
      onStatusMessage(`连接 ${connection.id} 熔断器已手动重置。`);
    } catch (error) {
      onStatusMessage(
        `重置熔断器失败: ${error instanceof Error ? error.message : '未知错误'}`,
      );
    } finally {
      setIsResettingCircuit(false);
    }
  }

  async function handleTestConnection() {
    if (activeConnectionIndex === null) {
      return;
    }

    const connection = connections[activeConnectionIndex];
    if (!connection) {
      return;
    }

    setIsTesting(true);
    setTestResult(null);

    const portPath = metadataString(connection.metadata, 'port_path', '');
    const baudRate = metadataNumber(connection.metadata, 'baud_rate', 9600);
    const dataBits = metadataNumber(connection.metadata, 'data_bits', 8);
    const parity = metadataString(connection.metadata, 'parity', 'none');
    const stopBits = metadataNumber(connection.metadata, 'stop_bits', 1);
    const flowControl = metadataString(connection.metadata, 'flow_control', 'none');

    try {
      const result = await testSerialConnection(
        portPath,
        baudRate,
        dataBits,
        parity,
        stopBits,
        flowControl,
      );
      setTestResult(result);
    } catch (error) {
      setTestResult({
        ok: false,
        message: error instanceof Error ? error.message : '测试连接失败',
      });
    } finally {
      setIsTesting(false);
    }
  }

  function handlePortPathChange(index: number, value: string) {
    handleMetadataFieldChange(index, 'port_path', value);
    setTestResult(null);
  }

  function handleBaudRateChange(index: number, value: number) {
    handleMetadataFieldChange(index, 'baud_rate', value);
    setTestResult(null);
  }

  function handleRefreshPorts() {
    if (activeConnectionIndex === null) {
      return;
    }

    setIsScanningPorts(true);
    listSerialPorts()
      .then((ports) => {
        setScannedPorts(ports);
      })
      .catch(() => {
        setScannedPorts([]);
      })
      .finally(() => {
        setIsScanningPorts(false);
      });
  }

  function handleRefreshInterfaces() {
    setIsScanningInterfaces(true);
    listNetworkInterfaces()
      .then((ifaces) => {
        setScannedInterfaces(ifaces);
      })
      .catch(() => {
        setScannedInterfaces([]);
      })
      .finally(() => {
        setIsScanningInterfaces(false);
      });
  }

  // ---- 派生状态 ----

  const activeConnection =
    activeConnectionIndex !== null ? connections[activeConnectionIndex] : undefined;
  const isDeletePending =
    activeConnectionIndex !== null && pendingDeleteIndex === activeConnectionIndex;
  const activeDraftKey =
    activeConnectionIndex !== null ? connectionKey(activeConnectionIndex) : '';
  const activeUsage = activeConnection?.id
    ? usageByConnection.get(activeConnection.id) ?? { nodeIds: [], projectNames: [] }
    : { nodeIds: [], projectNames: [] };
  const activeMetadataError = activeDraftKey ? metadataErrors[activeDraftKey] : undefined;
  const activeRuntimeConnection = activeConnection?.id
    ? runtimeById.get(activeConnection.id)
    : undefined;

  // ---- 渲染 ----

  return (
    <section className="connection-studio">
      {!hideHeader && (
        <div
          className="panel__header panel__header--desktop window-safe-header"
          data-window-drag-region
        >
          <div className="panel__header__heading">
            <h2>连接资源编辑</h2>
          </div>
          <span className="panel__badge">
            {connections.length > 0 ? `${connections.length} 项` : '未配置'}
          </span>
        </div>
      )}

      {storageError ? <p className="connection-card__error">{storageError}</p> : null}

      <div className="connection-layout">
        <ExpandTransition
          active={activeConnection !== undefined && activeConnectionIndex !== null}
          mode="centered"
          base={
            <div className="connection-resources">
              <div className="connection-toolbar">
                {CONNECTION_TEMPLATES.map((template) => (
                  <button
                    key={template.key}
                    type="button"
                    className="connection-toolbar__button"
                    data-testid="connection-add"
                    onClick={() => handleAddConnection(template)}
                  >
                    <strong>{template.label}</strong>
                    <span>{template.description}</span>
                  </button>
                ))}
              </div>

              {isLoading ? (
                <div className="connection-empty" data-testid="connection-empty-state">
                  <span className="connection-loading-spinner" aria-hidden="true" />
                  <p>正在加载连接资源…</p>
                </div>
              ) : connections.length === 0 ? (
                <div className="connection-empty" data-testid="connection-empty-state">
                  <p>暂无连接</p>
                </div>
              ) : (
                <ConnectionCardGrid
                  connections={connections}
                  activeConnectionIndex={activeConnectionIndex}
                  setActiveConnectionIndex={setActiveConnectionIndex}
                  runtimeById={runtimeById}
                  usageByConnection={usageByConnection}
                  devicesByConnectionId={devicesByConnectionId}
                />
              )}
            </div>
          }
          overlay={activeConnection && activeConnectionIndex !== null ? (
            <section
              className="connection-settings-panel"
              role="dialog"
              aria-modal="true"
              aria-label="连接资源设置"
            >
              <ConnectionTestPanel
                connection={activeConnection}
                connectionIndex={activeConnectionIndex}
                runtimeConnection={activeRuntimeConnection}
                devicesByConnectionId={devicesByConnectionId}
                onClose={() => setActiveConnectionIndex(null)}
                healthCallbacks={{
                  handleTestConnection,
                  handleResetCircuitBreaker,
                  isTesting,
                  isResettingCircuit,
                  testResult,
                }}
              />

              <ConnectionForm
                connection={activeConnection}
                connectionIndex={activeConnectionIndex}
                draftKey={activeDraftKey}
                idDrafts={idDrafts}
                metadataDrafts={metadataDrafts}
                isAdvancedOpen={isAdvancedOpen}
                setIsAdvancedOpen={setIsAdvancedOpen}
                callbacks={{
                  setIdDrafts,
                  commitConnectionId,
                  handleTypeChange,
                  handleMetadataFieldChange,
                  handleGovernanceFieldChange,
                  handleMetadataChange,
                  handlePortPathChange,
                  handleBaudRateChange,
                  handleRefreshPorts,
                  handleRefreshInterfaces,
                  scannedPorts,
                  isScanningPorts,
                  scannedInterfaces,
                  isScanningInterfaces,
                }}
              />

              {duplicateConnectionIds.has(activeConnection.id.trim()) ? (
                <p className="connection-card__error">
                  当前连接 ID 与其他连接重复，部署前建议修正为唯一值。
                </p>
              ) : null}

              {activeMetadataError ? (
                <p className="connection-card__error">
                  Metadata JSON 暂未同步: {activeMetadataError}
                </p>
              ) : (
                <p className="connection-card__hint">
                  {activeUsage.nodeIds.length > 0
                    ? `引用工程: ${activeUsage.projectNames.join(', ')} · 节点: ${activeUsage.nodeIds.join(', ')}`
                    : '还没有节点通过 connection_id 绑定到这个连接。'}
                </p>
              )}

              {isSerialConnectionType(activeConnection.type) && testResult !== null ? (
                <p className={`serial-test-result ${testResult.ok ? 'is-ok' : 'is-error'}`}>
                  {testResult.message}
                </p>
              ) : null}

              <div className="connection-settings-panel__footer">
                {isDeletePending ? (
                  <button
                    type="button"
                    className="ghost"
                    onClick={() => setPendingDeleteIndex(null)}
                  >
                    取消
                  </button>
                ) : null}
                <button
                  type="button"
                  className={`ghost connection-card__delete ${isDeletePending ? 'is-pending' : ''}`}
                  data-testid="connection-delete"
                  onClick={() => handleRemoveConnection(activeConnectionIndex)}
                >
                  <DeleteActionIcon />
                  {isDeletePending ? '确认删除' : '删除连接'}
                </button>
              </div>
            </section>
          ) : <div />}
        />
      </div>
    </section>
  );
}
