import {
  useEffect,
  useMemo,
  useState,
  type ComponentType,
  type Dispatch,
  type SVGProps,
  type SetStateAction,
} from 'react';

import type {
  ConnectionDefinition,
  ConnectionRecord,
  JsonValue,
} from '../types';
import {
  ConnectionsIcon,
  DeleteActionIcon,
  HttpClientNodeIcon,
  ModbusNodeIcon,
  SerialNodeIcon,
  SettingsIcon,
} from './app/AppIcons';
import {
  listSerialPorts,
  testSerialConnection,
  type SerialPortInfo,
  type TestSerialResult,
} from '../lib/tauri';

export interface ConnectionUsageSummary {
  nodeIds: string[];
  projectNames: string[];
}

interface ConnectionStudioProps {
  connections: ConnectionDefinition[];
  setConnections: Dispatch<SetStateAction<ConnectionDefinition[]>>;
  usageByConnection: Map<string, ConnectionUsageSummary>;
  runtimeConnections: ConnectionRecord[];
  isLoading?: boolean;
  storageError?: string | null;
  onStatusMessage: (msg: string) => void;
}

interface ConnectionTemplate {
  key: string;
  label: string;
  description: string;
  idPrefix: string;
  definition: ConnectionDefinition;
}

type ConnectionIconComponent = ComponentType<SVGProps<SVGSVGElement>>;

const CONNECTION_TEMPLATES: ConnectionTemplate[] = [
  {
    key: 'modbus',
    label: 'Modbus TCP',
    description: '适合 PLC、变频器、温控器等现场设备。',
    idPrefix: 'modbus',
    definition: {
      id: 'modbus',
      type: 'modbus',
      metadata: {
        host: '192.168.10.11',
        port: 502,
        unit_id: 1,
      },
    },
  },
  {
    key: 'serial',
    label: '串口设备',
    description: '适合扫码枪、RFID 读卡器等主动上报 ASCII/HEX 的外设。',
    idPrefix: 'serial',
    definition: {
      id: 'serial',
      type: 'serial',
      metadata: {
        port_path: '/dev/tty.usbserial-0001',
        baud_rate: 9600,
        data_bits: 8,
        parity: 'none',
        stop_bits: 1,
        flow_control: 'none',
        encoding: 'ascii',
        delimiter: '\\n',
        read_timeout_ms: 100,
        idle_gap_ms: 80,
        max_frame_bytes: 512,
        trim: true,
      },
    },
  },
  {
    key: 'mqtt',
    label: 'MQTT Broker',
    description: '适合边缘网关上报、告警推送和云侧订阅。',
    idPrefix: 'mqtt',
    definition: {
      id: 'mqtt',
      type: 'mqtt',
      metadata: {
        host: 'broker.local',
        port: 1883,
        topic: 'factory/line-a/events',
      },
    },
  },
  {
    key: 'http',
    label: 'HTTP Sink',
    description: '适合作为流程末端的 Web API 或采集平台出口。',
    idPrefix: 'http_sink',
    definition: {
      id: 'http_sink',
      type: 'http',
      metadata: {
        url: 'https://example.com/ingest',
        method: 'POST',
      },
    },
  },
  {
    key: 'custom',
    label: '空白连接',
    description: '保留完全自定义空间，适配专有协议或插件节点。',
    idPrefix: 'connection',
    definition: {
      id: 'connection',
      type: 'custom',
      metadata: {},
    },
  },
];

const BAUD_RATE_OPTIONS = [
  1200, 2400, 4800, 9600, 19200, 38400, 57600, 115200, 230400, 460800, 921600,
];

const DEFAULT_PORT_PATH: Record<string, string> = {
  darwin: '/dev/cu.usbserial',
  linux: '/dev/ttyUSB0',
  win32: 'COM3',
};

function connectionKey(index: number): string {
  return String(index);
}

function formatMetadata(metadata: JsonValue | undefined): string {
  return JSON.stringify(metadata ?? {}, null, 2);
}

function isRecord(value: unknown): value is Record<string, JsonValue> {
  return typeof value === 'object' && value !== null && !Array.isArray(value);
}

function metadataRecord(metadata: JsonValue | undefined): Record<string, JsonValue> {
  return isRecord(metadata) ? metadata : {};
}

function metadataString(
  metadata: JsonValue | undefined,
  key: string,
  fallback: string,
): string {
  const value = metadataRecord(metadata)[key];
  return typeof value === 'string' ? value : fallback;
}

function metadataNumber(
  metadata: JsonValue | undefined,
  key: string,
  fallback: number,
): number {
  const value = metadataRecord(metadata)[key];
  return typeof value === 'number' && Number.isFinite(value) ? value : fallback;
}

function metadataBoolean(
  metadata: JsonValue | undefined,
  key: string,
  fallback: boolean,
): boolean {
  const value = metadataRecord(metadata)[key];
  return typeof value === 'boolean' ? value : fallback;
}

function parseMetadataNumber(value: string, fallback: number): number {
  const parsed = Number(value);
  return Number.isFinite(parsed) ? parsed : fallback;
}

function normalizedConnectionType(connectionType: string): string {
  return connectionType.trim().toLowerCase();
}

function isSerialConnectionType(connectionType: string): boolean {
  switch (normalizedConnectionType(connectionType)) {
    case 'serial':
    case 'serialport':
    case 'serial_port':
    case 'uart':
    case 'rs232':
    case 'rs485':
      return true;
    default:
      return false;
  }
}

function connectionIconFor(connectionType: string): ConnectionIconComponent {
  const type = normalizedConnectionType(connectionType);
  if (isSerialConnectionType(type)) {
    return SerialNodeIcon;
  }

  switch (type) {
    case 'modbus':
    case 'modbus_tcp':
      return ModbusNodeIcon;
    case 'http':
    case 'http_sink':
      return HttpClientNodeIcon;
    default:
      return ConnectionsIcon;
  }
}

function compactConnectionValue(value: string, fallback: string): string {
  const normalized = value.trim();
  if (!normalized) {
    return fallback;
  }

  return normalized.length > 38
    ? `${normalized.slice(0, 22)}...${normalized.slice(-10)}`
    : normalized;
}

function connectionParameterBrief(connection: ConnectionDefinition): string {
  const type = normalizedConnectionType(connection.type);
  if (isSerialConnectionType(type)) {
    const portPath = metadataString(connection.metadata, 'port_path', '未配置端口');
    const baudRate = metadataNumber(connection.metadata, 'baud_rate', 9600);
    const encoding = metadataString(connection.metadata, 'encoding', 'ascii').toUpperCase();
    return `${compactConnectionValue(portPath, '未配置端口')} · ${baudRate} · ${encoding}`;
  }

  if (type === 'modbus' || type === 'modbus_tcp') {
    const host = metadataString(connection.metadata, 'host', '未配置主机');
    const port = metadataNumber(connection.metadata, 'port', 502);
    const unitId = metadataNumber(connection.metadata, 'unit_id', 1);
    return `${compactConnectionValue(host, '未配置主机')}:${port} · Unit ${unitId}`;
  }

  if (type === 'mqtt') {
    const host = metadataString(connection.metadata, 'host', '未配置 Broker');
    const port = metadataNumber(connection.metadata, 'port', 1883);
    const topic = metadataString(connection.metadata, 'topic', '未配置 Topic');
    return `${compactConnectionValue(host, '未配置 Broker')}:${port} · ${compactConnectionValue(
      topic,
      '未配置 Topic',
    )}`;
  }

  if (type === 'http' || type === 'http_sink') {
    const method = metadataString(connection.metadata, 'method', 'POST').toUpperCase();
    const url = metadataString(connection.metadata, 'url', '未配置 URL');
    return `${method} · ${compactConnectionValue(url, '未配置 URL')}`;
  }

  const metadataKeys = Object.keys(metadataRecord(connection.metadata));
  return metadataKeys.length > 0 ? `${metadataKeys.length} 个参数` : '未配置参数';
}

function connectionRuntimeState(runtimeConnection: ConnectionRecord | undefined): {
  label: string;
  state: 'busy' | 'local' | 'runtime';
} {
  if (!runtimeConnection) {
    return { label: '等待部署', state: 'local' };
  }

  if (runtimeConnection.in_use) {
    return { label: '运行占用', state: 'busy' };
  }

  return { label: '运行可用', state: 'runtime' };
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

    const draftKey = connectionKey(index);
    const nextMetadata = {
      ...metadataRecord(currentConnection.metadata),
      [key]: value,
    };
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

    saveConnections(nextConnections, '连接参数已更新。');
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
    : ConnectionsIcon;

  return (
    <section className="connection-studio">
      <div
        className="panel__header panel__header--dense window-safe-header"
        data-window-drag-region
      >
        <div>
          <h2>连接资源编辑</h2>
        </div>
        <span className="panel__badge">
          {connections.length > 0 ? `${connections.length} 项` : '未配置'}
        </span>
      </div>

      {storageError ? <p className="connection-card__error">{storageError}</p> : null}

      <div className="connection-layout">
        <div className="connection-resources">
          <div className="connection-toolbar">
            {CONNECTION_TEMPLATES.map((template) => (
              <button
                key={template.key}
                type="button"
                className="connection-toolbar__button"
                onClick={() => handleAddConnection(template)}
              >
                <strong>{template.label}</strong>
                <span>{template.description}</span>
              </button>
            ))}
          </div>

          {isLoading ? (
            <div className="connection-empty">
              <p>正在加载连接资源…</p>
            </div>
          ) : connections.length === 0 ? (
            <div className="connection-empty">
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
                      <button
                        type="button"
                        className="connection-card__settings"
                        aria-label={`设置 ${connection.id || `connection_${index + 1}`}`}
                        onClick={() => setActiveConnectionIndex(index)}
                      >
                        <SettingsIcon />
                      </button>
                    </div>
                  </article>
                );
              })}
            </div>
          )}
        </div>

        {activeConnection && activeConnectionIndex !== null ? (
          <div
            className="connection-settings-dialog"
            role="presentation"
            onMouseDown={(event) => {
              if (event.target === event.currentTarget) {
                setActiveConnectionIndex(null);
              }
            }}
          >
            <section
              className="connection-settings-panel"
              role="dialog"
              aria-modal="true"
              aria-label="连接资源设置"
              onMouseDown={(event) => event.stopPropagation()}
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
                  onClick={() => handleRemoveConnection(activeConnectionIndex)}
                >
                  <DeleteActionIcon />
                  {isDeletePending ? '确认删除' : '删除连接'}
                </button>
              </div>
            </section>
          </div>
        ) : null}
      </div>
    </section>
  );
}
