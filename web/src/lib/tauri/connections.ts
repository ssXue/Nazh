import { invoke } from '@tauri-apps/api/core';

import type {
  ConnectionDiagnosticResult,
  ConnectionRecord,
  JsonValue,
} from '../../types';

export interface ConnectionAssetSummary {
  id: string;
  protocolType: string;
  description?: string | null;
  version: number;
  updatedAt: string;
}

export interface ConnectionAssetDetail {
  id: string;
  protocolType: string;
  description?: string | null;
  version: number;
  specJson: JsonValue;
  specYaml: string;
  yamlFilePath?: string | null;
  createdAt: string;
  updatedAt: string;
}

export async function listConnections(): Promise<ConnectionRecord[]> {
  return invoke<ConnectionRecord[]>('list_connections');
}

export async function listConnectionAssets(
  workspacePath: string,
): Promise<ConnectionAssetSummary[]> {
  return invoke<ConnectionAssetSummary[]>('list_connection_assets', {
    workspacePath: workspacePath.trim() || null,
  });
}

export async function loadConnectionAsset(
  id: string,
  workspacePath: string,
): Promise<ConnectionAssetDetail | null> {
  return invoke<ConnectionAssetDetail | null>('load_connection_asset', {
    id,
    workspacePath: workspacePath.trim() || null,
  });
}

export async function saveConnectionAsset(
  id: string,
  specYaml: string,
  workspacePath: string,
): Promise<void> {
  return invoke<void>('save_connection_asset', {
    id,
    specYaml,
    workspacePath: workspacePath.trim() || null,
  });
}

export async function deleteConnectionAsset(id: string, workspacePath: string): Promise<void> {
  return invoke<void>('delete_connection_asset', {
    id,
    workspacePath: workspacePath.trim() || null,
  });
}

export async function saveConnectionSecret(
  connectionId: string,
  secretKey: string,
  value: string,
): Promise<void> {
  return invoke<void>('save_connection_secret', {
    connectionId,
    secretKey,
    value,
  });
}

export async function deleteConnectionSecret(
  connectionId: string,
  secretKey: string,
): Promise<void> {
  return invoke<void>('delete_connection_secret', {
    connectionId,
    secretKey,
  });
}

export async function resetConnectionCircuitBreaker(
  connectionId: string,
): Promise<void> {
  return invoke<void>('reset_connection_circuit_breaker', {
    connectionId,
  });
}

export async function testConnectionAsset(
  connectionId: string,
  workspacePath?: string,
): Promise<ConnectionDiagnosticResult> {
  return invoke<ConnectionDiagnosticResult>('test_connection_asset', {
    connectionId,
    workspacePath: workspacePath?.trim() || null,
  });
}

export interface SerialPortInfo {
  path: string;
  portType: string;
  description: string;
}

export async function listSerialPorts(): Promise<SerialPortInfo[]> {
  return invoke<SerialPortInfo[]>('list_serial_ports');
}

export async function testSerialConnection(
  portPath: string,
  baudRate: number,
  dataBits: number,
  parity: string,
  stopBits: number,
  flowControl: string,
): Promise<TestSerialResult> {
  return invoke<TestSerialResult>('test_serial_connection', {
    portPath,
    baudRate,
    dataBits,
    parity,
    stopBits,
    flowControl,
  });
}

export interface NetworkInterfaceInfo {
  name: string;
  description: string;
  mac: string | null;
  ipv4: string[];
  isLoopback: boolean;
  isUp: boolean;
}

export async function listNetworkInterfaces(): Promise<NetworkInterfaceInfo[]> {
  return invoke<NetworkInterfaceInfo[]>('list_network_interfaces');
}

export interface TestSerialResult {
  ok: boolean;
  message: string;
}
