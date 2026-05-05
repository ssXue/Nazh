import {
  useEffect,
  useMemo,
  useState,
  type Dispatch,
  type SetStateAction,
} from 'react';

import type {
  ConnectionDefinition,
  ConnectionRecord,
  JsonValue,
} from '../types';
import {
  DeleteActionIcon,
} from './app/AppIcons';
import { ExpandTransition } from './app/ExpandTransition';
import {
  listSerialPorts,
  testSerialConnection,
  type SerialPortInfo,
  type TestSerialResult,
} from '../lib/tauri';

// 从拆分模块导入
import {
  DEFAULT_CONNECTION_GOVERNANCE,
  CONNECTION_TEMPLATES,
  BAUD_RATE_OPTIONS,
  CAN_BITRATE_OPTIONS,
  DEFAULT_PORT_PATH,
  connectionKey,
  formatMetadata,
  metadataRecord,
  metadataString,
  metadataNumber,
  metadataBoolean,
  governanceRecord,
  governanceNumber,
  parseMetadataNumber,
  isSerialConnectionType,
  isCanConnectionType,
  isEthercatConnectionType,
  isHttpConnectionType,
  isBarkConnectionType,
  connectionIconFor,
  connectionParameterBrief,
  connectionRuntimeState,
  formatHealthTimestamp,
} from './connection-studio-utils';
import type { ConnectionUsageSummary, ConnectionTemplate } from './connection-studio-utils';

interface ConnectionStudioProps {
  connections: ConnectionDefinition[];
  setConnections: Dispatch<SetStateAction<ConnectionDefinition[]>>;
  usageByConnection: Map<string, ConnectionUsageSummary>;
  runtimeConnections: ConnectionRecord[];
  isLoading?: boolean;
  storageError?: string | null;
  onStatusMessage: (msg: string) => void;
}

export function ConnectionStudio({
  connections,
  setConnections,
  usageByConnection,
  runtimeConnections,
  isLoading = false,
  storageError,
  onStatusMessage,
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
  const [testResult, setTestResult] = useState<TestSerialResult | null>(null);
  const [isTesting, setIsTesting] = useState(false);
  const [isAdvancedOpen, setIsAdvancedOpen] = useState(false);

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

  useEffect(() => {
    if (isLoading || activeConnectionIndex === null) {
      return;
    }

    if (activeConnectionIndex >= connections.length) {
      setActiveConnectionIndex(connections.length > 0 ? connections.length - 1 : null);
    }
  }, [activeConnectionIndex, connections.length, isLoading]);

  useEffect(() => {
    if (activeConnectionIndex === null) {
      setPendingDeleteIndex(null);
      return;
    }

    if (pendingDeleteIndex !== null && pendingDeleteIndex !== activeConnectionIndex) {
      setPendingDeleteIndex(null);
    }
  }, [activeConnectionIndex, pendingDeleteIndex]);

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
    key: keyof typeof DEFAULT_CONNECTION_GOVERNANCE,
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
  const ActiveConnectionIcon = activeConnection
    ? connectionIconFor(activeConnection.type)
    : connectionIconFor('');
  const activeRuntimeConnection = activeConnection?.id
    ? runtimeById.get(activeConnection.id)
    : undefined;
  const activeRuntimeState = connectionRuntimeState(activeRuntimeConnection);

  return (
    <section className="connection-studio">
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
                <div className="connection-grid">
                  {connections.map((connection, index) => {
                    const runtimeConnection = connection.id
                      ? runtimeById.get(connection.id)
                      : undefined;
                    const usage = connection.id
                      ? usageByConnection.get(connection.id) ?? { nodeIds: [], projectNames: [] }
                      : { nodeIds: [], projectNames: [] };
                    const runtimeState = connectionRuntimeState(runtimeConnection);
                    const ConnectionIcon = connectionIconFor(connection.type);
                    const isActive = activeConnectionIndex === index;

                    return (
                      <article
                        key={`${connection.id || 'connection'}-${index}`}
                        className={`connection-card ${isActive ? 'is-active' : ''}`}
                        data-testid="connection-card"
                        role="button"
                        tabIndex={0}
                        aria-label={`编辑 ${connection.id || `connection_${index + 1}`}`}
                        onClick={() => setActiveConnectionIndex(index)}
                        onKeyDown={(event) => {
                          if (event.key === 'Enter' || event.key === ' ') {
                            event.preventDefault();
                            setActiveConnectionIndex(index);
                          }
                        }}
                      >
                        <div className="connection-card__main">
                          <div className="connection-card__icon">
                            <ConnectionIcon />
                          </div>
                          <div className="connection-card__identity">
                            <strong>{connection.id || `connection_${index + 1}`}</strong>
                            <span className="connection-card__brief">
                              {connectionParameterBrief(connection)}
                            </span>
                            <div className="connection-card__meta">
                              <span className="connection-card__tag">
                                {connection.type || 'custom'}
                              </span>
                              <span className="connection-card__tag">
                                {usage.nodeIds.length > 0 ? `${usage.nodeIds.length} 节点` : '未绑定'}
                              </span>
                              <span className="connection-card__tag">
                                {usage.projectNames.length > 0
                                  ? `${usage.projectNames.length} 工程`
                                  : '全局可用'}
                              </span>
                            </div>
                          </div>
                        </div>

                        <div className="connection-card__footer">
                          <span className={`connection-status is-${runtimeState.state}`}>
                            <span className="connection-status__dot" />
                            {runtimeState.label}
                          </span>
                        </div>

                        {runtimeState.detail ? (
                          <p className="connection-card__hint">{runtimeState.detail}</p>
                        ) : null}

                        {runtimeState.failureReason ? (
                          <p className="connection-card__error">{runtimeState.failureReason}</p>
                        ) : null}
                      </article>
                    );
                  })}
                </div>
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
              <div className="connection-settings-panel__header">
                <div className="connection-settings-panel__icon">
                  <ActiveConnectionIcon />
                </div>
                <div>
                  <strong>
                    {activeConnection.id || `connection_${activeConnectionIndex + 1}`}
                  </strong>
                  <span>{connectionParameterBrief(activeConnection)}</span>
                </div>
                <div className="connection-settings-panel__actions">
                  {isSerialConnectionType(activeConnection.type) ? (
                    <button
                      type="button"
                      className={`ghost ${testResult !== null && testResult.ok ? 'is-success' : ''} ${testResult !== null && !testResult.ok ? 'is-error' : ''}`}
                      onClick={handleTestConnection}
                      disabled={isTesting}
                    >
                      {isTesting ? '测试中...' : '测试连接'}
                    </button>
                  ) : null}
                  <button
                    type="button"
                    className="connection-settings-panel__close"
                    onClick={() => setActiveConnectionIndex(null)}
                  >
                    完成
                  </button>
                </div>
              </div>

              <section className="connection-health-panel">
                <div className="connection-health-panel__headline">
                  <span className={`connection-status is-${activeRuntimeState.state}`}>
                    <span className="connection-status__dot" />
                    {activeRuntimeState.label}
                  </span>
                  {activeRuntimeState.health?.lastLatencyMs ? (
                    <span className="connection-card__tag">
                      最近链路 {activeRuntimeState.health.lastLatencyMs} ms
                    </span>
                  ) : null}
                  {activeRuntimeState.health?.consecutiveFailures ? (
                    <span className="connection-card__tag">
                      连续失败 {activeRuntimeState.health.consecutiveFailures}
                    </span>
                  ) : null}
                </div>

                <p className="connection-health-panel__summary">
                  {activeRuntimeState.detail ?? '当前没有可用的连接健康诊断。'}
                </p>

                <div className="connection-health-panel__metrics">
                  <span className="connection-card__tag">
                    最近心跳 {formatHealthTimestamp(activeRuntimeState.health?.lastHeartbeatAt) ?? '--'}
                  </span>
                  <span className="connection-card__tag">
                    最近失败 {formatHealthTimestamp(activeRuntimeState.health?.lastFailureAt) ?? '--'}
                  </span>
                  <span className="connection-card__tag">
                    超时 {activeRuntimeState.health?.timeoutCount ?? 0}
                  </span>
                  <span className="connection-card__tag">
                    限流 {activeRuntimeState.health?.rateLimitHits ?? 0}
                  </span>
                  <span className="connection-card__tag">
                    重连 {activeRuntimeState.health?.reconnectAttempts ?? 0}
                  </span>
                  {activeRuntimeState.health?.nextRetryAt ? (
                    <span className="connection-card__tag">
                      下次重试 {formatHealthTimestamp(activeRuntimeState.health.nextRetryAt)}
                    </span>
                  ) : null}
                  {activeRuntimeState.health?.circuitOpenUntil ? (
                    <span className="connection-card__tag">
                      熔断至 {formatHealthTimestamp(activeRuntimeState.health.circuitOpenUntil)}
                    </span>
                  ) : null}
                </div>

                {activeRuntimeState.health?.recommendedAction ? (
                  <p className="connection-health-panel__action">
                    {activeRuntimeState.health.recommendedAction}
                  </p>
                ) : null}

                {activeRuntimeState.failureReason ? (
                  <p className="connection-card__error">{activeRuntimeState.failureReason}</p>
                ) : null}
              </section>

              <div className="connection-form connection-settings-panel__form">
                <label>
                  <span>连接 ID</span>
                  <input
                    value={idDrafts[activeDraftKey] ?? activeConnection.id}
                    onChange={(event) =>
                      setIdDrafts((current) => ({
                        ...current,
                        [activeDraftKey]: event.target.value,
                      }))
                    }
                    onBlur={() => commitConnectionId(activeConnectionIndex)}
                    onKeyDown={(event) => {
                      if (event.key === 'Enter') {
                        event.preventDefault();
                        commitConnectionId(activeConnectionIndex);
                        event.currentTarget.blur();
                      }
                    }}
                    placeholder="例如 plc_main"
                  />
                </label>

                <label>
                  <span>协议类型</span>
                  <input
                    value={activeConnection.type}
                    onChange={(event) =>
                      handleTypeChange(activeConnectionIndex, event.target.value)
                    }
                    placeholder="例如 modbus / mqtt / http"
                  />
                </label>

                {isSerialConnectionType(activeConnection.type) ? (
                  <>
                    <label className="serial-port-field">
                      <span>串口路径</span>
                      <div className="serial-port-select">
                        <select
                          value={metadataString(
                            activeConnection.metadata,
                            'port_path',
                            DEFAULT_PORT_PATH[navigator.platform.startsWith('Win') ? 'win32' : navigator.platform.startsWith('Mac') ? 'darwin' : 'linux'] ?? '/dev/ttyUSB0',
                          )}
                          onChange={(event) => handlePortPathChange(activeConnectionIndex, event.target.value)}
                        >
                          <option value="">-- 选择端口 --</option>
                          {scannedPorts.map((port) => (
                            <option key={port.path} value={port.path}>
                              {port.path}
                              {port.description ? ` (${port.description})` : ''}
                            </option>
                          ))}
                        </select>
                        <button
                          type="button"
                          className="ghost"
                          onClick={handleRefreshPorts}
                          disabled={isScanningPorts}
                          title="刷新端口列表"
                        >
                          {isScanningPorts ? '扫描中...' : '刷新'}
                        </button>
                      </div>
                    </label>
                    <label>
                      <span>波特率</span>
                      <select
                        value={metadataNumber(activeConnection.metadata, 'baud_rate', 9600)}
                        onChange={(event) =>
                          handleBaudRateChange(activeConnectionIndex, parseInt(event.target.value, 10))
                        }
                      >
                        {BAUD_RATE_OPTIONS.map((rate) => (
                          <option key={rate} value={rate}>
                            {rate}
                          </option>
                        ))}
                      </select>
                    </label>
                    <label>
                      <span>数据位</span>
                      <select
                        value={String(metadataNumber(activeConnection.metadata, 'data_bits', 8))}
                        onChange={(event) =>
                          handleMetadataFieldChange(
                            activeConnectionIndex,
                            'data_bits',
                            parseMetadataNumber(event.target.value, 8),
                          )
                        }
                      >
                        <option value="8">8</option>
                        <option value="7">7</option>
                        <option value="6">6</option>
                        <option value="5">5</option>
                      </select>
                    </label>
                    <label>
                      <span>校验位</span>
                      <select
                        value={metadataString(activeConnection.metadata, 'parity', 'none')}
                        onChange={(event) =>
                          handleMetadataFieldChange(
                            activeConnectionIndex,
                            'parity',
                            event.target.value,
                          )
                        }
                      >
                        <option value="none">None</option>
                        <option value="odd">Odd</option>
                        <option value="even">Even</option>
                      </select>
                    </label>
                    <label>
                      <span>停止位</span>
                      <select
                        value={String(metadataNumber(activeConnection.metadata, 'stop_bits', 1))}
                        onChange={(event) =>
                          handleMetadataFieldChange(
                            activeConnectionIndex,
                            'stop_bits',
                            parseMetadataNumber(event.target.value, 1),
                          )
                        }
                      >
                        <option value="1">1</option>
                        <option value="2">2</option>
                      </select>
                    </label>
                    <label>
                      <span>流控</span>
                      <select
                        value={metadataString(activeConnection.metadata, 'flow_control', 'none')}
                        onChange={(event) =>
                          handleMetadataFieldChange(
                            activeConnectionIndex,
                            'flow_control',
                            event.target.value,
                          )
                        }
                      >
                        <option value="none">None</option>
                        <option value="software">Software</option>
                        <option value="hardware">Hardware</option>
                      </select>
                    </label>
                    <label>
                      <span>主数据格式</span>
                      <select
                        value={metadataString(activeConnection.metadata, 'encoding', 'ascii')}
                        onChange={(event) =>
                          handleMetadataFieldChange(
                            activeConnectionIndex,
                            'encoding',
                            event.target.value,
                          )
                        }
                      >
                        <option value="ascii">ASCII</option>
                        <option value="hex">HEX</option>
                      </select>
                    </label>

                    <div className="serial-advanced-toggle">
                      <button
                        type="button"
                        className="ghost"
                        onClick={() => setIsAdvancedOpen(!isAdvancedOpen)}
                      >
                        {isAdvancedOpen ? '收起高级设置' : '高级设置'}
                      </button>
                    </div>

                    {isAdvancedOpen ? (
                      <>
                        <label>
                          <span>帧分隔符</span>
                          <input
                            value={metadataString(activeConnection.metadata, 'delimiter', '\\n')}
                            onChange={(event) =>
                              handleMetadataFieldChange(
                                activeConnectionIndex,
                                'delimiter',
                                event.target.value,
                              )
                            }
                            placeholder="\\n、\\r\\n 或 hex:0D0A"
                          />
                        </label>
                        <label>
                          <span>读超时 ms</span>
                          <input
                            type="number"
                            value={metadataNumber(activeConnection.metadata, 'read_timeout_ms', 100)}
                            onChange={(event) =>
                              handleMetadataFieldChange(
                                activeConnectionIndex,
                                'read_timeout_ms',
                                parseMetadataNumber(event.target.value, 100),
                              )
                            }
                          />
                        </label>
                        <label>
                          <span>空闲提交 ms</span>
                          <input
                            type="number"
                            value={metadataNumber(activeConnection.metadata, 'idle_gap_ms', 80)}
                            onChange={(event) =>
                              handleMetadataFieldChange(
                                activeConnectionIndex,
                                'idle_gap_ms',
                                parseMetadataNumber(event.target.value, 80),
                              )
                            }
                          />
                        </label>
                        <label>
                          <span>最大帧字节</span>
                          <input
                            type="number"
                            value={metadataNumber(activeConnection.metadata, 'max_frame_bytes', 512)}
                            onChange={(event) =>
                              handleMetadataFieldChange(
                                activeConnectionIndex,
                                'max_frame_bytes',
                                parseMetadataNumber(event.target.value, 512),
                              )
                            }
                          />
                        </label>
                        <label>
                          <span>裁剪空白</span>
                          <select
                            value={
                              metadataBoolean(activeConnection.metadata, 'trim', true)
                                ? 'true'
                                : 'false'
                            }
                            onChange={(event) =>
                              handleMetadataFieldChange(
                                activeConnectionIndex,
                                'trim',
                                event.target.value === 'true',
                              )
                            }
                          >
                            <option value="true">是</option>
                            <option value="false">否</option>
                          </select>
                        </label>
                      </>
                    ) : null}
                  </>
                ) : null}

                {isCanConnectionType(activeConnection.type) ? (
                  <>
                    <label>
                      <span>CAN 接口</span>
                      <select
                        value={metadataString(activeConnection.metadata, 'interface', 'slcan')}
                        onChange={(event) =>
                          handleMetadataFieldChange(activeConnectionIndex, 'interface', event.target.value)
                        }
                      >
                        <option value="slcan">SLCAN</option>
                        <option value="mock">Mock</option>
                      </select>
                    </label>
                    <label className="serial-port-field">
                      <span>串口通道</span>
                      <div className="serial-port-select">
                        <select
                          value={metadataString(
                            activeConnection.metadata,
                            'channel',
                            DEFAULT_PORT_PATH[navigator.platform.startsWith('Win') ? 'win32' : navigator.platform.startsWith('Mac') ? 'darwin' : 'linux'] ?? '/dev/ttyUSB0',
                          )}
                          onChange={(event) =>
                            handleMetadataFieldChange(activeConnectionIndex, 'channel', event.target.value)
                          }
                        >
                          <option value="">-- 选择通道 --</option>
                          {scannedPorts.map((port) => (
                            <option key={port.path} value={port.path}>
                              {port.path}
                              {port.description ? ` (${port.description})` : ''}
                            </option>
                          ))}
                        </select>
                        <button
                          type="button"
                          className="ghost"
                          onClick={handleRefreshPorts}
                          disabled={isScanningPorts}
                          title="刷新端口列表"
                        >
                          {isScanningPorts ? '扫描中...' : '刷新'}
                        </button>
                      </div>
                    </label>
                    <label>
                      <span>串口波特率</span>
                      <select
                        value={metadataNumber(activeConnection.metadata, 'baud_rate', 115200)}
                        onChange={(event) =>
                          handleMetadataFieldChange(
                            activeConnectionIndex,
                            'baud_rate',
                            parseInt(event.target.value, 10),
                          )
                        }
                      >
                        {BAUD_RATE_OPTIONS.map((rate) => (
                          <option key={rate} value={rate}>
                            {rate}
                          </option>
                        ))}
                      </select>
                    </label>
                    <label>
                      <span>CAN bitrate</span>
                      <select
                        value={metadataNumber(activeConnection.metadata, 'bitrate', 500000)}
                        onChange={(event) =>
                          handleMetadataFieldChange(
                            activeConnectionIndex,
                            'bitrate',
                            parseInt(event.target.value, 10),
                          )
                        }
                      >
                        {CAN_BITRATE_OPTIONS.map((rate) => (
                          <option key={rate} value={rate}>
                            {rate / 1000} kbps
                          </option>
                        ))}
                      </select>
                    </label>
                  </>
                ) : null}

                {isEthercatConnectionType(activeConnection.type) ? (
                  <>
                    <label>
                      <span>后端</span>
                      <select
                        value={metadataString(activeConnection.metadata, 'backend', 'ethercrab')}
                        onChange={(event) =>
                          handleMetadataFieldChange(activeConnectionIndex, 'backend', event.target.value)
                        }
                      >
                        <option value="ethercrab">ethercrab（真实）</option>
                        <option value="mock">Mock（模拟）</option>
                      </select>
                    </label>
                    <label>
                      <span>网络接口</span>
                      <input
                        value={metadataString(activeConnection.metadata, 'interface', 'eth0')}
                        onChange={(event) =>
                          handleMetadataFieldChange(activeConnectionIndex, 'interface', event.target.value)
                        }
                        placeholder="eth0"
                      />
                    </label>
                    <label>
                      <span>周期 (ms)</span>
                      <input
                        type="number"
                        value={metadataNumber(activeConnection.metadata, 'cycle_time_ms', 10)}
                        onChange={(event) =>
                          handleMetadataFieldChange(
                            activeConnectionIndex,
                            'cycle_time_ms',
                            parseInt(event.target.value, 10) || 10,
                          )
                        }
                        min={1}
                      />
                    </label>
                    <label>
                      <span>OP 超时 (ms)</span>
                      <input
                        type="number"
                        value={metadataNumber(activeConnection.metadata, 'op_timeout_ms', 15000)}
                        onChange={(event) =>
                          handleMetadataFieldChange(
                            activeConnectionIndex,
                            'op_timeout_ms',
                            parseInt(event.target.value, 10) || 15000,
                          )
                        }
                        min={1}
                      />
                    </label>
                  </>
                ) : null}

                {isHttpConnectionType(activeConnection.type) ? (
                  <>
                    <label>
                      <span>请求地址</span>
                      <input
                        value={metadataString(activeConnection.metadata, 'url', '')}
                        onChange={(event) =>
                          handleMetadataFieldChange(activeConnectionIndex, 'url', event.target.value)
                        }
                        placeholder="https://example.com/webhook"
                      />
                    </label>
                    <label>
                      <span>请求方法</span>
                      <select
                        value={metadataString(activeConnection.metadata, 'method', 'POST').toUpperCase()}
                        onChange={(event) =>
                          handleMetadataFieldChange(activeConnectionIndex, 'method', event.target.value)
                        }
                      >
                        <option value="POST">POST</option>
                        <option value="PUT">PUT</option>
                        <option value="PATCH">PATCH</option>
                        <option value="GET">GET</option>
                      </select>
                    </label>
                    <label>
                      <span>Webhook 类型</span>
                      <select
                        value={metadataString(activeConnection.metadata, 'webhook_kind', 'generic')}
                        onChange={(event) =>
                          handleMetadataFieldChange(
                            activeConnectionIndex,
                            'webhook_kind',
                            event.target.value,
                          )
                        }
                      >
                        <option value="generic">通用 Webhook</option>
                        <option value="dingtalk">钉钉机器人</option>
                      </select>
                    </label>
                    {metadataString(activeConnection.metadata, 'webhook_kind', 'generic') === 'dingtalk' ? (
                      <>
                        <label>
                          <span>@ 手机号</span>
                          <input
                            value={metadataString(activeConnection.metadata, 'at_mobiles', '')}
                            onChange={(event) =>
                              handleMetadataFieldChange(
                                activeConnectionIndex,
                                'at_mobiles',
                                event.target.value,
                              )
                            }
                            placeholder="13800000000,13900000000"
                          />
                        </label>
                        <label>
                          <span>@ 所有人</span>
                          <select
                            value={
                              metadataBoolean(activeConnection.metadata, 'at_all', false)
                                ? 'true'
                                : 'false'
                            }
                            onChange={(event) =>
                              handleMetadataFieldChange(
                                activeConnectionIndex,
                                'at_all',
                                event.target.value === 'true',
                              )
                            }
                          >
                            <option value="false">否</option>
                            <option value="true">是</option>
                          </select>
                        </label>
                      </>
                    ) : null}
                    <label>
                      <span>内容类型</span>
                      <input
                        value={metadataString(
                          activeConnection.metadata,
                          'content_type',
                          'application/json',
                        )}
                        onChange={(event) =>
                          handleMetadataFieldChange(
                            activeConnectionIndex,
                            'content_type',
                            event.target.value,
                          )
                        }
                      />
                    </label>
                    <label>
                      <span>请求超时 ms</span>
                      <input
                        type="number"
                        value={metadataNumber(activeConnection.metadata, 'request_timeout_ms', 4000)}
                        onChange={(event) =>
                          handleMetadataFieldChange(
                            activeConnectionIndex,
                            'request_timeout_ms',
                            parseMetadataNumber(event.target.value, 4000),
                          )
                        }
                      />
                    </label>
                  </>
                ) : null}

                {isBarkConnectionType(activeConnection.type) ? (
                  <>
                    <label>
                      <span>服务地址</span>
                      <input
                        value={metadataString(activeConnection.metadata, 'server_url', 'https://api.day.app')}
                        onChange={(event) =>
                          handleMetadataFieldChange(
                            activeConnectionIndex,
                            'server_url',
                            event.target.value,
                          )
                        }
                        placeholder="https://api.day.app"
                      />
                    </label>
                    <label>
                      <span>设备 Key / 推送 URL</span>
                      <input
                        value={metadataString(activeConnection.metadata, 'device_key', '')}
                        onChange={(event) =>
                          handleMetadataFieldChange(
                            activeConnectionIndex,
                            'device_key',
                            event.target.value,
                          )
                        }
                        placeholder="填写 device_key，或粘贴 https://api.day.app/{key}"
                      />
                    </label>
                    <label>
                      <span>请求超时 ms</span>
                      <input
                        type="number"
                        value={metadataNumber(activeConnection.metadata, 'request_timeout_ms', 4000)}
                        onChange={(event) =>
                          handleMetadataFieldChange(
                            activeConnectionIndex,
                            'request_timeout_ms',
                            parseMetadataNumber(event.target.value, 4000),
                          )
                        }
                      />
                    </label>
                  </>
                ) : null}

                <div className="connection-form__section connection-form__section--full">
                  <strong className="connection-form__section-title">连接健康治理</strong>
                </div>

                <label>
                  <span>建连超时 ms</span>
                  <input
                    type="number"
                    value={governanceNumber(activeConnection.metadata, 'connect_timeout_ms')}
                    onChange={(event) =>
                      handleGovernanceFieldChange(
                        activeConnectionIndex,
                        'connect_timeout_ms',
                        parseMetadataNumber(
                          event.target.value,
                          DEFAULT_CONNECTION_GOVERNANCE.connect_timeout_ms,
                        ),
                      )
                    }
                  />
                </label>
                <label>
                  <span>操作超时 ms</span>
                  <input
                    type="number"
                    value={governanceNumber(activeConnection.metadata, 'operation_timeout_ms')}
                    onChange={(event) =>
                      handleGovernanceFieldChange(
                        activeConnectionIndex,
                        'operation_timeout_ms',
                        parseMetadataNumber(
                          event.target.value,
                          DEFAULT_CONNECTION_GOVERNANCE.operation_timeout_ms,
                        ),
                      )
                    }
                  />
                </label>
                <label>
                  <span>心跳间隔 ms</span>
                  <input
                    type="number"
                    value={governanceNumber(activeConnection.metadata, 'heartbeat_interval_ms')}
                    onChange={(event) =>
                      handleGovernanceFieldChange(
                        activeConnectionIndex,
                        'heartbeat_interval_ms',
                        parseMetadataNumber(
                          event.target.value,
                          DEFAULT_CONNECTION_GOVERNANCE.heartbeat_interval_ms,
                        ),
                      )
                    }
                  />
                </label>
                <label>
                  <span>心跳超时 ms</span>
                  <input
                    type="number"
                    value={governanceNumber(activeConnection.metadata, 'heartbeat_timeout_ms')}
                    onChange={(event) =>
                      handleGovernanceFieldChange(
                        activeConnectionIndex,
                        'heartbeat_timeout_ms',
                        parseMetadataNumber(
                          event.target.value,
                          DEFAULT_CONNECTION_GOVERNANCE.heartbeat_timeout_ms,
                        ),
                      )
                    }
                  />
                </label>
                <label>
                  <span>限流次数</span>
                  <input
                    type="number"
                    value={governanceNumber(activeConnection.metadata, 'rate_limit_max_attempts')}
                    onChange={(event) =>
                      handleGovernanceFieldChange(
                        activeConnectionIndex,
                        'rate_limit_max_attempts',
                        parseMetadataNumber(
                          event.target.value,
                          DEFAULT_CONNECTION_GOVERNANCE.rate_limit_max_attempts,
                        ),
                      )
                    }
                  />
                </label>
                <label>
                  <span>限流窗口 ms</span>
                  <input
                    type="number"
                    value={governanceNumber(activeConnection.metadata, 'rate_limit_window_ms')}
                    onChange={(event) =>
                      handleGovernanceFieldChange(
                        activeConnectionIndex,
                        'rate_limit_window_ms',
                        parseMetadataNumber(
                          event.target.value,
                          DEFAULT_CONNECTION_GOVERNANCE.rate_limit_window_ms,
                        ),
                      )
                    }
                  />
                </label>
                <label>
                  <span>限流冷却 ms</span>
                  <input
                    type="number"
                    value={governanceNumber(activeConnection.metadata, 'rate_limit_cooldown_ms')}
                    onChange={(event) =>
                      handleGovernanceFieldChange(
                        activeConnectionIndex,
                        'rate_limit_cooldown_ms',
                        parseMetadataNumber(
                          event.target.value,
                          DEFAULT_CONNECTION_GOVERNANCE.rate_limit_cooldown_ms,
                        ),
                      )
                    }
                  />
                </label>
                <label>
                  <span>熔断阈值</span>
                  <input
                    type="number"
                    value={governanceNumber(activeConnection.metadata, 'circuit_failure_threshold')}
                    onChange={(event) =>
                      handleGovernanceFieldChange(
                        activeConnectionIndex,
                        'circuit_failure_threshold',
                        parseMetadataNumber(
                          event.target.value,
                          DEFAULT_CONNECTION_GOVERNANCE.circuit_failure_threshold,
                        ),
                      )
                    }
                  />
                </label>
                <label>
                  <span>熔断冷却 ms</span>
                  <input
                    type="number"
                    value={governanceNumber(activeConnection.metadata, 'circuit_open_ms')}
                    onChange={(event) =>
                      handleGovernanceFieldChange(
                        activeConnectionIndex,
                        'circuit_open_ms',
                        parseMetadataNumber(
                          event.target.value,
                          DEFAULT_CONNECTION_GOVERNANCE.circuit_open_ms,
                        ),
                      )
                    }
                  />
                </label>
                <label>
                  <span>重连基准 ms</span>
                  <input
                    type="number"
                    value={governanceNumber(activeConnection.metadata, 'reconnect_base_ms')}
                    onChange={(event) =>
                      handleGovernanceFieldChange(
                        activeConnectionIndex,
                        'reconnect_base_ms',
                        parseMetadataNumber(
                          event.target.value,
                          DEFAULT_CONNECTION_GOVERNANCE.reconnect_base_ms,
                        ),
                      )
                    }
                  />
                </label>
                <label>
                  <span>重连上限 ms</span>
                  <input
                    type="number"
                    value={governanceNumber(activeConnection.metadata, 'reconnect_max_ms')}
                    onChange={(event) =>
                      handleGovernanceFieldChange(
                        activeConnectionIndex,
                        'reconnect_max_ms',
                        parseMetadataNumber(
                          event.target.value,
                          DEFAULT_CONNECTION_GOVERNANCE.reconnect_max_ms,
                        ),
                      )
                    }
                  />
                </label>

                <label className="connection-form__metadata">
                  <span>
                    {isSerialConnectionType(activeConnection.type)
                      ? '高级 Metadata JSON'
                      : 'Metadata JSON'}
                  </span>
                  <textarea
                    value={
                      metadataDrafts[activeDraftKey] ?? formatMetadata(activeConnection.metadata)
                    }
                    onChange={(event) =>
                      handleMetadataChange(activeConnectionIndex, event.target.value)
                    }
                    spellCheck={false}
                  />
                </label>

              </div>

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
