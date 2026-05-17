import { useEffect, useMemo, useRef, useState, type Dispatch, type SetStateAction } from 'react';

import type { ConnectionDefinition, JsonValue } from '../types';
import {
  deleteConnectionAsset,
  hasTauriRuntime,
  listConnectionAssets,
  loadConnectionAsset,
  saveConnectionAsset,
  saveConnectionSecret,
} from '../lib/tauri';
import { buildDefaultConnectionDefinitions } from '../lib/projects';

export interface ConnectionLibraryStorageState {
  isReady: boolean;
  isSyncing: boolean;
  error: string | null;
}

export interface UseConnectionLibraryResult {
  connections: ConnectionDefinition[];
  setConnections: Dispatch<SetStateAction<ConnectionDefinition[]>>;
  storage: ConnectionLibraryStorageState;
  refreshConnections: () => Promise<void>;
}

type JsonRecord = Record<string, unknown>;

const DEFAULT_CONNECTION_GOVERNANCE = {
  connect_timeout_ms: 3000,
  operation_timeout_ms: 5000,
  heartbeat_interval_ms: 3000,
  heartbeat_timeout_ms: 12000,
  rate_limit_max_attempts: 8,
  rate_limit_window_ms: 10000,
  rate_limit_cooldown_ms: 4000,
  circuit_failure_threshold: 3,
  circuit_open_ms: 15000,
  reconnect_base_ms: 800,
  reconnect_max_ms: 8000,
} satisfies JsonRecord;

function isRecord(value: unknown): value is JsonRecord {
  return typeof value === 'object' && value !== null && !Array.isArray(value);
}

function metadataRecord(metadata: JsonValue | undefined): JsonRecord {
  return isRecord(metadata) ? metadata : {};
}

function stringField(record: JsonRecord, key: string, fallback = ''): string {
  const value = record[key];
  return typeof value === 'string' ? value : fallback;
}

function numberField(record: JsonRecord, key: string, fallback: number): number {
  const value = record[key];
  return typeof value === 'number' && Number.isFinite(value) ? value : fallback;
}

function boolField(record: JsonRecord, key: string, fallback: boolean): boolean {
  const value = record[key];
  return typeof value === 'boolean' ? value : fallback;
}

function optionalString(record: JsonRecord, key: string): string | undefined {
  const value = stringField(record, key).trim();
  return value ? value : undefined;
}

function optionalNumber(record: JsonRecord, key: string): number | undefined {
  const value = record[key];
  return typeof value === 'number' && Number.isFinite(value) ? value : undefined;
}

function governanceFromMetadata(metadata: JsonRecord): JsonRecord {
  const governance = metadata.governance;
  return isRecord(governance) ? governance : DEFAULT_CONNECTION_GOVERNANCE;
}

function protocolTypeFromConnectionType(type: string): string {
  if (type === 'modbus') return 'modbus-tcp';
  return type;
}

function connectionTypeFromProtocolType(type: string): string {
  if (type === 'modbus-tcp') return 'modbus';
  return type;
}

function connectionFromSpecJson(specJson: JsonValue): ConnectionDefinition | null {
  if (!isRecord(specJson) || !isRecord(specJson.protocol) || typeof specJson.id !== 'string') {
    return null;
  }

  const protocol = specJson.protocol;
  const type = stringField(protocol, 'type');
  const metadata: JsonRecord = { ...protocol };
  delete metadata.type;
  metadata.governance = isRecord(specJson.governance)
    ? specJson.governance
    : DEFAULT_CONNECTION_GOVERNANCE;

  if (isRecord(specJson.secrets)) {
    for (const [key, value] of Object.entries(specJson.secrets)) {
      if (typeof value === 'string') {
        metadata[key] = value;
      }
    }
  }

  if (type === 'can-slcan') {
    metadata.interface = 'slcan';
  }

  return {
    id: specJson.id,
    type: connectionTypeFromProtocolType(type),
    metadata: metadata as JsonValue,
  };
}

function protocolFromConnection(connection: ConnectionDefinition): JsonRecord {
  const metadata = metadataRecord(connection.metadata);
  const type = protocolTypeFromConnectionType(connection.type);

  switch (type) {
    case 'modbus-tcp':
      return {
        type,
        host: stringField(metadata, 'host'),
        port: numberField(metadata, 'port', 502),
        unit_id: numberField(metadata, 'unit_id', 1),
      };
    case 'serial':
      return {
        type,
        port_path: stringField(metadata, 'port_path'),
        baud_rate: numberField(metadata, 'baud_rate', 9600),
        data_bits: numberField(metadata, 'data_bits', 8),
        parity: stringField(metadata, 'parity', 'none'),
        stop_bits: numberField(metadata, 'stop_bits', 1),
        flow_control: stringField(metadata, 'flow_control', 'none'),
        encoding: optionalString(metadata, 'encoding'),
        delimiter: optionalString(metadata, 'delimiter'),
        read_timeout_ms: optionalNumber(metadata, 'read_timeout_ms'),
        idle_gap_ms: optionalNumber(metadata, 'idle_gap_ms'),
        max_frame_bytes: optionalNumber(metadata, 'max_frame_bytes'),
        trim: boolField(metadata, 'trim', true),
      };
    case 'mqtt':
      return {
        type,
        host: stringField(metadata, 'host'),
        port: numberField(metadata, 'port', 1883),
        topic: stringField(metadata, 'topic'),
        client_id: optionalString(metadata, 'client_id'),
      };
    case 'http':
      return {
        type,
        url: stringField(metadata, 'url'),
        method: stringField(metadata, 'method', 'POST'),
      };
    case 'bark':
      return {
        type,
        server_url: stringField(metadata, 'server_url', 'https://api.day.app'),
        request_timeout_ms: optionalNumber(metadata, 'request_timeout_ms'),
      };
    case 'can-slcan':
      return {
        type,
        channel: stringField(metadata, 'channel'),
        baud_rate: numberField(metadata, 'baud_rate', 115200),
        bitrate: numberField(metadata, 'bitrate', 500000),
      };
    case 'ethercat':
      return {
        type,
        backend: stringField(metadata, 'backend', 'ethercrab'),
        interface: stringField(metadata, 'interface'),
        cycle_time_ms: numberField(metadata, 'cycle_time_ms', 10),
        op_timeout_ms: numberField(metadata, 'op_timeout_ms', 15000),
      };
    default:
      throw new Error(`连接类型 ${connection.type} 尚未映射到 Connection DSL。`);
  }
}

function pruneUndefined(value: unknown): JsonValue | undefined {
  if (value === undefined) {
    return undefined;
  }
  if (Array.isArray(value)) {
    return value.map(pruneUndefined).filter((item) => item !== undefined) as JsonValue;
  }
  if (!isRecord(value)) {
    return value as JsonValue;
  }
  return Object.fromEntries(
    Object.entries(value)
      .filter(([, entry]) => entry !== undefined)
      .map(([key, entry]) => [key, pruneUndefined(entry)]),
  ) as JsonValue;
}

function isScalar(value: unknown): boolean {
  return value === null || ['string', 'number', 'boolean'].includes(typeof value);
}

function yamlScalar(value: unknown): string {
  if (typeof value === 'string') {
    return JSON.stringify(value);
  }
  if (typeof value === 'number' || typeof value === 'boolean') {
    return String(value);
  }
  return 'null';
}

function appendYamlEntry(lines: string[], key: string, value: unknown, indent: number) {
  const prefix = ' '.repeat(indent);
  if (isScalar(value)) {
    lines.push(`${prefix}${key}: ${yamlScalar(value)}`);
    return;
  }
  if (Array.isArray(value)) {
    if (value.length === 0) {
      lines.push(`${prefix}${key}: []`);
      return;
    }
    lines.push(`${prefix}${key}:`);
    appendYamlArray(lines, value, indent + 2);
    return;
  }
  if (isRecord(value)) {
    const entries = Object.entries(value);
    if (entries.length === 0) {
      lines.push(`${prefix}${key}: {}`);
      return;
    }
    lines.push(`${prefix}${key}:`);
    appendYamlObject(lines, value, indent + 2);
  }
}

function appendYamlArray(lines: string[], values: unknown[], indent: number) {
  const prefix = ' '.repeat(indent);
  for (const value of values) {
    if (isScalar(value)) {
      lines.push(`${prefix}- ${yamlScalar(value)}`);
    } else {
      lines.push(`${prefix}-`);
      if (Array.isArray(value)) {
        appendYamlArray(lines, value, indent + 2);
      } else if (isRecord(value)) {
        appendYamlObject(lines, value, indent + 2);
      }
    }
  }
}

function appendYamlObject(lines: string[], value: JsonRecord, indent: number) {
  for (const [key, entry] of Object.entries(value)) {
    appendYamlEntry(lines, key, entry, indent);
  }
}

function yamlFromRecord(value: JsonRecord): string {
  const lines: string[] = [];
  appendYamlObject(lines, value, 0);
  return `${lines.join('\n')}\n`;
}

async function persistConnectionSecrets(connection: ConnectionDefinition) {
  const metadata = metadataRecord(connection.metadata);
  const deviceKey = stringField(metadata, 'device_key').trim();
  if (connection.type === 'bark' && deviceKey && !deviceKey.startsWith('secret://')) {
    await saveConnectionSecret(connection.id, 'device_key', deviceKey);
  }
}

function specYamlFromConnection(connection: ConnectionDefinition): string {
  const metadata = metadataRecord(connection.metadata);
  const secrets: JsonRecord = {};
  if (connection.type === 'bark') {
    const deviceKey = stringField(metadata, 'device_key').trim();
    if (deviceKey) {
      secrets.device_key = 'secret://device_key';
    }
  }

  const spec = pruneUndefined({
    id: connection.id,
    protocol: protocolFromConnection(connection),
    governance: governanceFromMetadata(metadata),
    secrets: Object.keys(secrets).length ? secrets : undefined,
  } as JsonRecord);

  return yamlFromRecord(spec as JsonRecord);
}

function describeConnectionStorageError(error: unknown): string {
  if (typeof error === 'string' && error.trim()) {
    return error;
  }

  if (error instanceof Error && error.message.trim()) {
    return error.message;
  }

  return '连接资源同步失败。';
}

async function loadConnectionDefinitionsFromAssets(workspacePath: string) {
  const summaries = await listConnectionAssets(workspacePath);
  const definitions: ConnectionDefinition[] = [];

  for (const summary of summaries) {
    const detail = await loadConnectionAsset(summary.id, workspacePath);
    const definition = detail ? connectionFromSpecJson(detail.specJson) : null;
    if (definition) {
      definitions.push(definition);
    }
  }

  return definitions;
}

async function saveConnectionDefinitionsToAssets(
  workspacePath: string,
  definitions: ConnectionDefinition[],
) {
  const previousAssets = await listConnectionAssets(workspacePath);
  const nextIds = new Set(definitions.map((definition) => definition.id));

  for (const definition of definitions) {
    await persistConnectionSecrets(definition);
    await saveConnectionAsset(definition.id, specYamlFromConnection(definition), workspacePath);
  }

  for (const asset of previousAssets) {
    if (!nextIds.has(asset.id)) {
      await deleteConnectionAsset(asset.id, workspacePath);
    }
  }
}

export function useConnectionLibrary(workspacePath = ''): UseConnectionLibraryResult {
  const desktopStorageEnabled = hasTauriRuntime();
  const normalizedWorkspacePath = workspacePath.trim();
  const [connections, setConnections] = useState<ConnectionDefinition[]>(() =>
    desktopStorageEnabled ? [] : buildDefaultConnectionDefinitions(),
  );
  const [storage, setStorage] = useState<ConnectionLibraryStorageState>(() => ({
    isReady: !desktopStorageEnabled,
    isSyncing: false,
    error: null,
  }));
  const [hydratedWorkspacePath, setHydratedWorkspacePath] = useState<string | null>(
    desktopStorageEnabled ? null : normalizedWorkspacePath,
  );
  const lastSyncedSignatureRef = useRef<string | null>(null);
  const lastFailedSignatureRef = useRef<string | null>(null);
  const saveQueueRef = useRef(Promise.resolve());

  const connectionSignature = useMemo(() => JSON.stringify(connections), [connections]);

  useEffect(() => {
    if (!desktopStorageEnabled) {
      const definitions = buildDefaultConnectionDefinitions();
      setConnections(definitions);
      setStorage({
        isReady: true,
        isSyncing: false,
        error: null,
      });
      setHydratedWorkspacePath(normalizedWorkspacePath);
      lastSyncedSignatureRef.current = JSON.stringify(definitions);
      lastFailedSignatureRef.current = null;
      return;
    }

    let cancelled = false;

    setStorage({
      isReady: false,
      isSyncing: true,
      error: null,
    });

    void loadConnectionDefinitionsFromAssets(normalizedWorkspacePath)
      .then((definitions) => {
        if (cancelled) {
          return;
        }

        const hydrated = definitions.length ? definitions : buildDefaultConnectionDefinitions();
        setConnections(hydrated);
        setStorage({
          isReady: true,
          isSyncing: false,
          error: null,
        });
        setHydratedWorkspacePath(normalizedWorkspacePath);
        lastSyncedSignatureRef.current = definitions.length ? JSON.stringify(hydrated) : null;
        lastFailedSignatureRef.current = null;
      })
      .catch((error) => {
        if (cancelled) {
          return;
        }

        setConnections([]);
        setStorage({
          isReady: true,
          isSyncing: false,
          error: describeConnectionStorageError(error),
        });
      });

    return () => {
      cancelled = true;
    };
  }, [desktopStorageEnabled, normalizedWorkspacePath]);

  useEffect(() => {
    if (!desktopStorageEnabled) {
      lastSyncedSignatureRef.current = connectionSignature;
      return;
    }

    if (!storage.isReady || hydratedWorkspacePath !== normalizedWorkspacePath) {
      return;
    }

    if (lastSyncedSignatureRef.current === connectionSignature) {
      return;
    }

    if (lastFailedSignatureRef.current === connectionSignature) {
      return;
    }

    let cancelled = false;
    const pendingConnections = JSON.parse(connectionSignature) as ConnectionDefinition[];

    setStorage((current) => ({
      ...current,
      isSyncing: true,
      error: null,
    }));

    saveQueueRef.current = saveQueueRef.current
      .catch(() => undefined)
      .then(async () => {
        await saveConnectionDefinitionsToAssets(normalizedWorkspacePath, pendingConnections);
      });

    void saveQueueRef.current
      .then(() => {
        if (cancelled) {
          return;
        }

        lastSyncedSignatureRef.current = connectionSignature;
        lastFailedSignatureRef.current = null;
        setStorage({
          isReady: true,
          isSyncing: false,
          error: null,
        });
      })
      .catch((error) => {
        if (cancelled) {
          return;
        }

        lastFailedSignatureRef.current = connectionSignature;
        setStorage((current) => ({
          ...current,
          isSyncing: false,
          error: describeConnectionStorageError(error),
        }));
      });

    return () => {
      cancelled = true;
    };
  }, [
    connectionSignature,
    desktopStorageEnabled,
    hydratedWorkspacePath,
    normalizedWorkspacePath,
    storage.isReady,
  ]);

  async function refreshConnections() {
    if (!desktopStorageEnabled) {
      const definitions = buildDefaultConnectionDefinitions();
      setConnections(definitions);
      setStorage({
        isReady: true,
        isSyncing: false,
        error: null,
      });
      lastSyncedSignatureRef.current = JSON.stringify(definitions);
      lastFailedSignatureRef.current = null;
      return;
    }

    setStorage((current) => ({
      ...current,
      isSyncing: true,
      error: null,
    }));

    try {
      const definitions = await loadConnectionDefinitionsFromAssets(normalizedWorkspacePath);
      const hydrated = definitions.length ? definitions : buildDefaultConnectionDefinitions();
      setConnections(hydrated);
      setStorage({
        isReady: true,
        isSyncing: false,
        error: null,
      });
      setHydratedWorkspacePath(normalizedWorkspacePath);
      lastSyncedSignatureRef.current = definitions.length ? JSON.stringify(hydrated) : null;
      lastFailedSignatureRef.current = null;
    } catch (error) {
      setStorage((current) => ({
        ...current,
        isSyncing: false,
        error: describeConnectionStorageError(error),
      }));
    }
  }

  return {
    connections,
    setConnections,
    storage,
    refreshConnections,
  };
}
