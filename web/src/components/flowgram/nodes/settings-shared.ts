import type { AiGenerationParams, AiProviderView, ConnectionDefinition } from '../../../types';
import type { FlowgramLogicBranch } from './shared';
import { isRecord as _isRecord } from './shared';

export const isRecord = _isRecord;

export interface SelectedNodeDraft {
  id: string;
  nodeType: string;
  label: string;
  connectionId: string;
  timeoutMs: string;
  message: string;
  script: string;
  branches: FlowgramLogicBranch[];
  timerIntervalMs: string;
  timerImmediate: boolean;
  modbusUnitId: string;
  modbusRegister: string;
  modbusQuantity: string;
  modbusRegisterType: string;
  modbusBaseValue: string;
  modbusAmplitude: string;
  mqttMode: string;
  mqttTopic: string;
  mqttQos: string;
  mqttPayloadTemplate: string;
  httpWebhookKind: string;
  httpBodyMode: string;
  httpTitleTemplate: string;
  httpBodyTemplate: string;
  barkContentMode: string;
  barkTitleTemplate: string;
  barkSubtitleTemplate: string;
  barkBodyTemplate: string;
  barkLevel: string;
  barkBadge: string;
  barkSound: string;
  barkIcon: string;
  barkGroup: string;
  barkUrl: string;
  barkCopy: string;
  barkImage: string;
  barkAutoCopy: boolean;
  barkCall: boolean;
  barkArchiveMode: string;
  sqlDatabasePath: string;
  sqlTable: string;
  debugLabel: string;
  debugPretty: boolean;
  parameterBindings: Record<string, string | number | boolean>;
  lookupTable: Record<string, unknown>;
  lookupDefault: string;
  hitlTitle: string;
  hitlDescription: string;
  hitlApprovalTimeoutSec: string;
  hitlDefaultAction: string;
  hitlFormSchemaJson: string;
}

export interface NodeValidation {
  tone: 'info' | 'warning' | 'danger';
  message: string;
  field?: string;
}

export type FieldValidatorResult = string | { message: string; tone: 'info' | 'warning' | 'danger' } | null;

export type FieldValidator = (value: string) => FieldValidatorResult;

export interface NodeSettingsProps {
  draft: SelectedNodeDraft;
  updateDraft: (patch: Partial<SelectedNodeDraft>) => void;
  connections: ConnectionDefinition[];
  selectedConnection: ConnectionDefinition | null;
  compatibleConnections: ConnectionDefinition[];
  resolvedHttpWebhookKind: string;
  resolvedHttpBodyMode: string;
  aiProviders: AiProviderView[];
  activeAiProviderId: string | null;
  resolvedGlobalAiProvider: AiProviderView | null;
  preferredCopilotProvider: AiProviderView | null;
  aiGenerateButtonTitle: string;
  aiGenerating: boolean;
  onOpenAiDialog: () => void;
}

export function readString(value: unknown, fallback = ''): string {
  return typeof value === 'string' ? value : fallback;
}

export function readConnectionMetadataString(
  connection: ConnectionDefinition | undefined,
  key: string,
  fallback = '',
): string {
  if (!connection || !isRecord(connection.metadata)) {
    return fallback;
  }

  return readString(connection.metadata[key], fallback);
}

export function readBoolean(value: unknown, fallback: boolean): boolean {
  return typeof value === 'boolean' ? value : fallback;
}

export function readNumberString(value: unknown, fallback: string): string {
  return typeof value === 'number' && Number.isFinite(value) ? String(value) : fallback;
}

export function parsePositiveInteger(value: string): number | null {
  const normalized = value.trim();
  if (!normalized) {
    return null;
  }

  const parsed = Number(normalized);
  if (!Number.isFinite(parsed) || parsed <= 0) {
    return null;
  }

  return Math.round(parsed);
}

export function parseNonNegativeInteger(value: string): number | null {
  const normalized = value.trim();
  if (!normalized) {
    return null;
  }

  const parsed = Number(normalized);
  if (!Number.isFinite(parsed) || parsed < 0) {
    return null;
  }

  return Math.round(parsed);
}

export function parseFiniteNumber(value: string): number | null {
  const normalized = value.trim();
  if (!normalized) {
    return null;
  }

  const parsed = Number(normalized);
  return Number.isFinite(parsed) ? parsed : null;
}

export function isHttpConnectionType(connectionType: string): boolean {
  switch (connectionType.trim().toLowerCase()) {
    case 'http':
    case 'http_sink':
      return true;
    default:
      return false;
  }
}

export function isBarkConnectionType(connectionType: string): boolean {
  switch (connectionType.trim().toLowerCase()) {
    case 'bark':
    case 'bark_push':
      return true;
    default:
      return false;
  }
}

export function isSerialConnectionType(connectionType: string): boolean {
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

export function supportsConnectionBinding(nodeType: string): boolean {
  return (
    nodeType === 'native' ||
    nodeType === 'modbusRead' ||
    nodeType === 'serialTrigger' ||
    nodeType === 'mqttClient' ||
    nodeType === 'httpClient' ||
    nodeType === 'barkPush'
  );
}

export function connectionMatchesNodeType(nodeType: string, connection: ConnectionDefinition): boolean {
  switch (nodeType) {
    case 'serialTrigger':
      return isSerialConnectionType(connection.type);
    case 'modbusRead':
      return connection.type.trim().toLowerCase() === 'modbus' || connection.type.trim().toLowerCase() === 'modbus_tcp';
    case 'mqttClient':
      return connection.type.trim().toLowerCase() === 'mqtt';
    case 'httpClient':
      return isHttpConnectionType(connection.type);
    case 'barkPush':
      return isBarkConnectionType(connection.type);
    default:
      return true;
  }
}

export function compatibleConnectionHint(nodeType: string): string {
  switch (nodeType) {
    case 'serialTrigger':
      return 'serial / uart';
    case 'modbusRead':
      return 'modbus';
    case 'mqttClient':
      return 'mqtt';
    case 'httpClient':
      return 'http / http_sink';
    case 'barkPush':
      return 'bark';
    default:
      return '任意类型';
  }
}

export function isScriptNode(nodeType: string): boolean {
  return (
    nodeType === 'code' ||
    nodeType === 'if' ||
    nodeType === 'switch' ||
    nodeType === 'tryCatch' ||
    nodeType === 'loop'
  );
}

export function supportsScriptAi(nodeType: string): boolean {
  return nodeType === 'code' || nodeType === 'if' || nodeType === 'loop' || nodeType === 'tryCatch';
}

export function isUsableAiProvider(provider: AiProviderView | null | undefined): provider is AiProviderView {
  return Boolean(provider?.enabled && provider.hasApiKey);
}

export function usesDynamicPorts(nodeType: string): boolean {
  return (
    nodeType === 'if' ||
    nodeType === 'switch' ||
    nodeType === 'tryCatch' ||
    nodeType === 'loop'
  );
}

export function getPrimaryEditorLabel(nodeType: string): string {
  switch (nodeType) {
    case 'native':
      return '消息内容';
    case 'if':
      return '条件脚本';
    case 'switch':
      return '路由脚本';
    case 'tryCatch':
      return 'Try 脚本';
    case 'loop':
      return '迭代脚本';
    case 'code':
      return 'Code Script';
    default:
      return '脚本';
  }
}

export function requiresConnectionBinding(nodeType: string): boolean {
  return nodeType === 'serialTrigger' || nodeType === 'httpClient' || nodeType === 'barkPush';
}

export function validateConnectionBinding(params: {
  draft: SelectedNodeDraft;
  selectedConnection: ConnectionDefinition | null;
  compatibleConnections: ConnectionDefinition[];
  connections: ConnectionDefinition[];
}): NodeValidation[] {
  const { draft, selectedConnection, compatibleConnections, connections } = params;
  const result: NodeValidation[] = [];

  if (!supportsConnectionBinding(draft.nodeType)) {
    return result;
  }

  if (draft.connectionId && !selectedConnection) {
    result.push({ tone: 'danger', message: `连接 ${draft.connectionId} 未注册。` });
    return result;
  }

  if (selectedConnection) {
    const matched = connectionMatchesNodeType(draft.nodeType, selectedConnection);
    result.push({
      tone: matched ? 'info' : 'danger',
      message: matched
        ? `已绑定 ${selectedConnection.id} · ${selectedConnection.type}`
        : `${draft.nodeType} 节点需要绑定 ${compatibleConnectionHint(draft.nodeType)} 类型连接，当前为 ${selectedConnection.type}。`,
    });
    return result;
  }

  if (requiresConnectionBinding(draft.nodeType)) {
    const hint = compatibleConnectionHint(draft.nodeType);
    const noConn = compatibleConnections.length === 0;
    const labels: Record<string, string> = {
      serialTrigger: '串口触发',
      httpClient: 'HTTP Client',
      barkPush: 'Bark Push',
    };
    const label = labels[draft.nodeType] || draft.nodeType;
    result.push({
      tone: 'danger',
      message: noConn
        ? `当前还没有 ${hint} 类型连接，请先在 Connection Studio 中新增并绑定。`
        : `${label} 节点必须绑定 Connection Studio 中的 ${hint} 连接。`,
    });
    return result;
  }

  if (connections.length > 0) {
    result.push({ tone: 'warning', message: '当前节点未绑定连接资源。' });
  }

  return result;
}
