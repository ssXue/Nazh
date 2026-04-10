import { useEffect, useMemo, useState } from 'react';

import { formatWorkflowGraph } from '../lib/flowgram';
import type {
  ConnectionDefinition,
  ConnectionRecord,
  JsonValue,
  WorkflowGraph,
  WorkflowNodeDefinition,
} from '../types';

interface ConnectionStudioProps {
  graph: WorkflowGraph | null;
  astError: string | null;
  runtimeConnections: ConnectionRecord[];
  onGraphChange: (nextAstText: string, statusMessage: string) => void;
}

interface ConnectionTemplate {
  key: string;
  label: string;
  description: string;
  idPrefix: string;
  definition: ConnectionDefinition;
}

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

function isSerialConnectionType(connectionType: string): boolean {
  switch (connectionType.trim().toLowerCase()) {
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

function buildNodeUsageMap(graph: WorkflowGraph | null): Map<string, string[]> {
  const usage = new Map<string, string[]>();
  if (!graph) {
    return usage;
  }

  for (const [nodeId, node] of Object.entries(graph.nodes)) {
    if (!node.connection_id) {
      continue;
    }

    usage.set(node.connection_id, [...(usage.get(node.connection_id) ?? []), nodeId]);
  }

  return usage;
}

function updateNodeBindings(
  nodes: Record<string, WorkflowNodeDefinition>,
  sourceConnectionId: string,
  targetConnectionId: string,
): {
  nodes: Record<string, WorkflowNodeDefinition>;
  affectedNodes: string[];
} {
  const nextNodes: Record<string, WorkflowNodeDefinition> = {};
  const affectedNodes: string[] = [];

  for (const [nodeId, node] of Object.entries(nodes)) {
    if (node.connection_id !== sourceConnectionId) {
      nextNodes[nodeId] = node;
      continue;
    }

    affectedNodes.push(nodeId);
    nextNodes[nodeId] = {
      ...node,
      connection_id: targetConnectionId || undefined,
    };
  }

  return {
    nodes: nextNodes,
    affectedNodes,
  };
}

export function ConnectionStudio({
  graph,
  astError,
  runtimeConnections,
  onGraphChange,
}: ConnectionStudioProps) {
  const connections = graph?.connections ?? [];
  const usageByConnection = useMemo(() => buildNodeUsageMap(graph), [graph]);
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

  useEffect(() => {
    if (!graph) {
      setIdDrafts({});
      setMetadataDrafts({});
      setMetadataErrors({});
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
  }, [graph, connections]);

  function buildNextConnectionId(prefix: string): string {
    const existingIds = new Set(connections.map((connection) => connection.id));
    let index = 1;
    while (existingIds.has(`${prefix}_${index}`)) {
      index += 1;
    }

    return `${prefix}_${index}`;
  }

  function emitGraphUpdate(nextGraph: WorkflowGraph, statusMessage: string) {
    onGraphChange(formatWorkflowGraph(nextGraph), statusMessage);
  }

  function handleAddConnection(template: ConnectionTemplate) {
    if (!graph) {
      return;
    }

    const nextConnection: ConnectionDefinition = {
      ...template.definition,
      id: buildNextConnectionId(template.idPrefix),
      metadata: template.definition.metadata ?? {},
    };

    emitGraphUpdate(
      {
        ...graph,
        connections: [...connections, nextConnection],
      },
      `已新增 ${template.label} 连接，并同步到 AST 文本。`,
    );
  }

  function handleRemoveConnection(index: number) {
    if (!graph) {
      return;
    }

    const target = connections[index];
    if (!target) {
      return;
    }

    const nextConnections = connections.filter((_, connectionIndex) => connectionIndex !== index);
    const usageNodes = target.id ? usageByConnection.get(target.id) ?? [] : [];
    const bindingUpdate =
      target.id && usageNodes.length > 0
        ? updateNodeBindings(graph.nodes, target.id, '')
        : {
            nodes: graph.nodes,
            affectedNodes: [] as string[],
          };

    emitGraphUpdate(
      {
        ...graph,
        connections: nextConnections,
        nodes: bindingUpdate.nodes,
      },
      bindingUpdate.affectedNodes.length > 0
        ? `已删除连接 ${target.id}，并解除 ${bindingUpdate.affectedNodes.length} 个节点的绑定。`
        : `已删除连接 ${target.id || '未命名连接'}。`,
    );
  }

  function handleTypeChange(index: number, value: string) {
    if (!graph) {
      return;
    }

    const nextConnections = connections.map((connection, connectionIndex) =>
      connectionIndex === index
        ? {
            ...connection,
            type: value,
          }
        : connection,
    );

    emitGraphUpdate(
      {
        ...graph,
        connections: nextConnections,
      },
      '连接类型已同步回 AST 文本。',
    );
  }

  function commitConnectionId(index: number) {
    if (!graph) {
      return;
    }

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

    const bindingUpdate =
      currentConnection.id && currentConnection.id !== nextId
        ? updateNodeBindings(graph.nodes, currentConnection.id, nextId)
        : {
            nodes: graph.nodes,
            affectedNodes: [] as string[],
          };

    emitGraphUpdate(
      {
        ...graph,
        connections: nextConnections,
        nodes: bindingUpdate.nodes,
      },
      bindingUpdate.affectedNodes.length > 0
        ? `连接 ID 已更新为 ${nextId || '空值'}，并同步修正了 ${bindingUpdate.affectedNodes.length} 个节点引用。`
        : `连接 ID 已更新为 ${nextId || '空值'}。`,
    );
  }

  function handleMetadataChange(index: number, value: string) {
    if (!graph) {
      return;
    }

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

      emitGraphUpdate(
        {
          ...graph,
          connections: nextConnections,
        },
        '连接元数据已同步回 AST 文本。',
      );
    } catch (error) {
      setMetadataErrors((current) => ({
        ...current,
        [draftKey]: error instanceof Error ? error.message : 'Metadata JSON 解析失败',
      }));
    }
  }

  function handleMetadataFieldChange(index: number, key: string, value: JsonValue) {
    if (!graph) {
      return;
    }

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

    emitGraphUpdate(
      {
        ...graph,
        connections: nextConnections,
      },
      '连接参数已同步回 AST 文本。',
    );
  }

  if (!graph) {
    return (
      <section className="connection-studio">
        <div
          className="panel__header panel__header--dense window-safe-header"
          data-window-drag-region
        >
          <div>
            <h2>连接资源编辑</h2>
          </div>
        </div>
        <p className="panel__error">{astError ?? 'AST 解析失败，暂时无法结构化编辑连接。'}</p>
      </section>
    );
  }

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

      {connections.length === 0 ? (
        <div className="connection-empty">
          <p>暂无连接</p>
        </div>
      ) : (
        <div className="connection-grid">
          {connections.map((connection, index) => {
            const draftKey = connectionKey(index);
            const runtimeConnection = connection.id ? runtimeById.get(connection.id) : undefined;
            const usageNodes = connection.id ? usageByConnection.get(connection.id) ?? [] : [];
            const metadataError = metadataErrors[draftKey];
            const idDraft = idDrafts[draftKey] ?? connection.id;

            return (
              <article key={`${connection.id || 'connection'}-${index}`} className="connection-card">
                <div className="connection-card__header">
                  <div className="connection-card__identity">
                    <strong>{connection.id || `connection_${index + 1}`}</strong>
                    <p>
                      {usageNodes.length > 0
                        ? `当前被 ${usageNodes.length} 个节点引用`
                        : '当前没有节点绑定这个连接'}
                    </p>
                    <div className="connection-card__meta">
                      <span className="connection-card__tag">{connection.type || 'custom'}</span>
                      <span className="connection-card__tag">
                        {usageNodes.length > 0 ? `${usageNodes.length} 节点` : '未绑定'}
                      </span>
                    </div>
                  </div>

                  <div className="connection-card__actions">
                    <span
                      className={`connection-chip ${runtimeConnection ? 'is-runtime' : 'is-local'} ${
                        runtimeConnection?.in_use ? 'is-busy' : ''
                      }`}
                    >
                      {runtimeConnection
                        ? runtimeConnection.in_use
                          ? 'Runtime 借出中'
                          : 'Runtime 已注册'
                        : '仅 AST'}
                    </span>
                    <button
                      type="button"
                      className="ghost connection-card__delete"
                      onClick={() => handleRemoveConnection(index)}
                    >
                      删除
                    </button>
                  </div>
                </div>

                <div className="connection-form">
                  <label>
                    <span>连接 ID</span>
                    <input
                      value={idDraft}
                      onChange={(event) =>
                        setIdDrafts((current) => ({
                          ...current,
                          [draftKey]: event.target.value,
                        }))
                      }
                      onBlur={() => commitConnectionId(index)}
                      onKeyDown={(event) => {
                        if (event.key === 'Enter') {
                          event.preventDefault();
                          commitConnectionId(index);
                          event.currentTarget.blur();
                        }
                      }}
                      placeholder="例如 plc_main"
                    />
                  </label>

                  <label>
                    <span>协议类型</span>
                    <input
                      value={connection.type}
                      onChange={(event) => handleTypeChange(index, event.target.value)}
                      placeholder="例如 modbus / mqtt / http"
                    />
                  </label>

                  {isSerialConnectionType(connection.type) ? (
                    <>
                      <label>
                        <span>串口路径</span>
                        <input
                          value={metadataString(
                            connection.metadata,
                            'port_path',
                            '/dev/tty.usbserial-0001',
                          )}
                          onChange={(event) =>
                            handleMetadataFieldChange(index, 'port_path', event.target.value)
                          }
                          placeholder="/dev/tty.usbserial-0001 或 COM3"
                        />
                      </label>
                      <label>
                        <span>波特率</span>
                        <input
                          type="number"
                          value={metadataNumber(connection.metadata, 'baud_rate', 9600)}
                          onChange={(event) =>
                            handleMetadataFieldChange(
                              index,
                              'baud_rate',
                              parseMetadataNumber(event.target.value, 9600),
                            )
                          }
                        />
                      </label>
                      <label>
                        <span>数据位</span>
                        <select
                          value={String(metadataNumber(connection.metadata, 'data_bits', 8))}
                          onChange={(event) =>
                            handleMetadataFieldChange(
                              index,
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
                          value={metadataString(connection.metadata, 'parity', 'none')}
                          onChange={(event) =>
                            handleMetadataFieldChange(index, 'parity', event.target.value)
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
                          value={String(metadataNumber(connection.metadata, 'stop_bits', 1))}
                          onChange={(event) =>
                            handleMetadataFieldChange(
                              index,
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
                          value={metadataString(connection.metadata, 'flow_control', 'none')}
                          onChange={(event) =>
                            handleMetadataFieldChange(index, 'flow_control', event.target.value)
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
                          value={metadataString(connection.metadata, 'encoding', 'ascii')}
                          onChange={(event) =>
                            handleMetadataFieldChange(index, 'encoding', event.target.value)
                          }
                        >
                          <option value="ascii">ASCII</option>
                          <option value="hex">HEX</option>
                        </select>
                      </label>
                      <label>
                        <span>帧分隔符</span>
                        <input
                          value={metadataString(connection.metadata, 'delimiter', '\\n')}
                          onChange={(event) =>
                            handleMetadataFieldChange(index, 'delimiter', event.target.value)
                          }
                          placeholder="\\n、\\r\\n 或 hex:0D0A"
                        />
                      </label>
                      <label>
                        <span>读超时 ms</span>
                        <input
                          type="number"
                          value={metadataNumber(connection.metadata, 'read_timeout_ms', 100)}
                          onChange={(event) =>
                            handleMetadataFieldChange(
                              index,
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
                          value={metadataNumber(connection.metadata, 'idle_gap_ms', 80)}
                          onChange={(event) =>
                            handleMetadataFieldChange(
                              index,
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
                          value={metadataNumber(connection.metadata, 'max_frame_bytes', 512)}
                          onChange={(event) =>
                            handleMetadataFieldChange(
                              index,
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
                            metadataBoolean(connection.metadata, 'trim', true)
                              ? 'true'
                              : 'false'
                          }
                          onChange={(event) =>
                            handleMetadataFieldChange(index, 'trim', event.target.value === 'true')
                          }
                        >
                          <option value="true">是</option>
                          <option value="false">否</option>
                        </select>
                      </label>
                    </>
                  ) : null}

                  <label className="connection-form__metadata">
                    <span>
                      {isSerialConnectionType(connection.type)
                        ? '高级 Metadata JSON'
                        : 'Metadata JSON'}
                    </span>
                    <textarea
                      value={metadataDrafts[draftKey] ?? formatMetadata(connection.metadata)}
                      onChange={(event) => handleMetadataChange(index, event.target.value)}
                      spellCheck={false}
                    />
                  </label>
                </div>

                {duplicateConnectionIds.has(connection.id.trim()) ? (
                  <p className="connection-card__error">
                    当前连接 ID 与其他连接重复，部署前建议修正为唯一值。
                  </p>
                ) : null}

                {metadataError ? (
                  <p className="connection-card__error">
                    Metadata JSON 暂未同步: {metadataError}
                  </p>
                ) : (
                  <p className="connection-card__hint">
                    {usageNodes.length > 0
                      ? `引用节点: ${usageNodes.join(', ')}`
                      : '还没有节点通过 connection_id 绑定到这个连接。'}
                  </p>
                )}
              </article>
            );
          })}
        </div>
      )}
    </section>
  );
}
