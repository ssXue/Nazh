import { useCallback, useEffect, useMemo, useRef, useState } from 'react';

import { type FlowNodeEntity, useClientContext } from '@flowgram.ai/free-layout-editor';
import { type PanelFactory, usePanelManager } from '@flowgram.ai/panel-manager-plugin';

import {
  getDefaultBarkBodyTemplate,
  getDefaultBarkTitleTemplate,
  getLogicNodeBranchDefinitions,
  getDefaultHttpAlarmBodyTemplate,
  getDefaultHttpAlarmTitleTemplate,
  inferHttpWebhookKind,
  normalizeNodeKind,
  normalizeHttpBodyMode,
  parseTimeoutMs,
  type FlowgramLogicBranch,
} from './flowgram-node-library';
import { generateScriptStream, getNodeContext } from '../../lib/script-generation';
import type { AiGenerationParams, AiProviderView, ConnectionDefinition } from '../../types';

export interface FlowgramNodeSettingsPanelProps {
  nodeId: string;
  connections: ConnectionDefinition[];
  aiProviders: AiProviderView[];
  activeAiProviderId: string | null;
  copilotParams: AiGenerationParams;
}

interface SelectedNodeDraft {
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
  httpUrl: string;
  httpMethod: string;
  httpHeaders: string;
  httpWebhookKind: string;
  httpBodyMode: string;
  httpContentType: string;
  httpRequestTimeoutMs: string;
  httpTitleTemplate: string;
  httpBodyTemplate: string;
  httpAtMobiles: string;
  httpAtAll: boolean;
  barkServerUrl: string;
  barkDeviceKey: string;
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
  barkRequestTimeoutMs: string;
  sqlDatabasePath: string;
  sqlTable: string;
  debugLabel: string;
  debugPretty: boolean;
}

interface NodeValidation {
  tone: 'info' | 'warning' | 'danger';
  message: string;
}

type NodeConfigMap = Record<string, unknown>;

export const FLOWGRAM_NODE_SETTINGS_PANEL_KEY = 'nazh-flowgram-node-settings';

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null && !Array.isArray(value);
}

function readString(value: unknown, fallback = ''): string {
  return typeof value === 'string' ? value : fallback;
}

function readBoolean(value: unknown, fallback: boolean): boolean {
  return typeof value === 'boolean' ? value : fallback;
}

function readNumberString(value: unknown, fallback: string): string {
  return typeof value === 'number' && Number.isFinite(value) ? String(value) : fallback;
}

function parsePositiveInteger(value: string): number | null {
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

function parseNonNegativeInteger(value: string): number | null {
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

function parseFiniteNumber(value: string): number | null {
  const normalized = value.trim();
  if (!normalized) {
    return null;
  }

  const parsed = Number(normalized);
  return Number.isFinite(parsed) ? parsed : null;
}

function parseHeadersJson(text: string): Record<string, unknown> | null {
  const normalized = text.trim();
  if (!normalized) {
    return {};
  }

  try {
    const value = JSON.parse(normalized);
    return isRecord(value) ? value : null;
  } catch {
    return null;
  }
}

function stringifyStringList(value: unknown): string {
  if (!Array.isArray(value)) {
    return '';
  }

  return value
    .filter((item): item is string => typeof item === 'string' && item.trim().length > 0)
    .join(', ');
}

function parseStringList(value: string): string[] {
  return value
    .split(',')
    .map((item) => item.trim())
    .filter((item) => item.length > 0);
}

function isConnectionNode(nodeType: string): boolean {
  return nodeType === 'native' || nodeType === 'modbusRead' || nodeType === 'serialTrigger' || nodeType === 'mqttClient';
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

function isScriptNode(nodeType: string): boolean {
  return (
    nodeType === 'code' ||
    nodeType === 'if' ||
    nodeType === 'switch' ||
    nodeType === 'tryCatch' ||
    nodeType === 'loop'
  );
}

function supportsScriptAi(nodeType: string): boolean {
  return nodeType === 'code';
}

function isUsableAiProvider(provider: AiProviderView | null | undefined): provider is AiProviderView {
  return Boolean(provider?.enabled && provider.hasApiKey);
}

function usesDynamicPorts(nodeType: string): boolean {
  return (
    nodeType === 'if' ||
    nodeType === 'switch' ||
    nodeType === 'tryCatch' ||
    nodeType === 'loop'
  );
}

function readEditableBranches(nodeType: string, config: unknown): FlowgramLogicBranch[] {
  if (nodeType !== 'switch') {
    return [];
  }

  return getLogicNodeBranchDefinitions(nodeType, config).filter((branch) => !branch.fixed);
}

function readNodeDraft(node: FlowNodeEntity): SelectedNodeDraft {
  const rawData = (node.getExtInfo() ?? {}) as {
    label?: string;
    nodeType?: string;
    connectionId?: string | null;
    timeoutMs?: number | null;
    config?: unknown;
  };
  const config = isRecord(rawData.config) ? rawData.config : {};
  const nodeType = normalizeNodeKind(rawData.nodeType ?? node.flowNodeType);
  const httpUrl = readString(config.url);
  const httpWebhookKind = readString(config.webhook_kind, inferHttpWebhookKind(httpUrl));
  const httpBodyMode = normalizeHttpBodyMode(config.body_mode, httpWebhookKind);

  return {
    id: node.id,
    nodeType,
    label: rawData.label ?? node.id,
    connectionId: rawData.connectionId ?? '',
    timeoutMs: rawData.timeoutMs ? String(rawData.timeoutMs) : '',
    message: readString(config.message),
    script: readString(config.script),
    branches: readEditableBranches(nodeType, config),
    timerIntervalMs: readNumberString(config.interval_ms, '5000'),
    timerImmediate: readBoolean(config.immediate, true),
    modbusUnitId: readNumberString(config.unit_id, '1'),
    modbusRegister: readNumberString(config.register, '40001'),
    modbusQuantity: readNumberString(config.quantity, '1'),
    modbusRegisterType: readString(config.register_type, 'holding'),
    modbusBaseValue: readNumberString(config.base_value, '64'),
    modbusAmplitude: readNumberString(config.amplitude, '6'),
    mqttMode: readString(config.mode, 'publish'),
    mqttTopic: readString(config.topic, ''),
    mqttQos: typeof config.qos === 'number' ? String(config.qos) : '0',
    mqttPayloadTemplate: readString(config.payload_template, ''),
    httpUrl,
    httpMethod: readString(config.method, 'POST'),
    httpHeaders: JSON.stringify(isRecord(config.headers) ? config.headers : {}, null, 2),
    httpWebhookKind,
    httpBodyMode,
    httpContentType: readString(config.content_type, 'application/json'),
    httpRequestTimeoutMs: readNumberString(config.request_timeout_ms, '4000'),
    httpTitleTemplate: readString(config.title_template, getDefaultHttpAlarmTitleTemplate()),
    httpBodyTemplate: readString(
      config.body_template,
      httpBodyMode === 'dingtalk_markdown' ? getDefaultHttpAlarmBodyTemplate() : '',
    ),
    httpAtMobiles: stringifyStringList(config.at_mobiles),
    httpAtAll: readBoolean(config.at_all, false),
    barkServerUrl: readString(config.server_url, 'https://api.day.app'),
    barkDeviceKey: readString(config.device_key),
    barkContentMode: readString(config.content_mode, 'body'),
    barkTitleTemplate: readString(config.title_template, getDefaultBarkTitleTemplate()),
    barkSubtitleTemplate: readString(config.subtitle_template, ''),
    barkBodyTemplate: readString(config.body_template, getDefaultBarkBodyTemplate()),
    barkLevel: readString(config.level, 'active'),
    barkBadge:
      typeof config.badge === 'number'
        ? String(config.badge)
        : readString(config.badge, ''),
    barkSound: readString(config.sound, ''),
    barkIcon: readString(config.icon, ''),
    barkGroup: readString(config.group, ''),
    barkUrl: readString(config.url, ''),
    barkCopy: readString(config.copy, ''),
    barkImage: readString(config.image, ''),
    barkAutoCopy: readBoolean(config.auto_copy, false),
    barkCall: readBoolean(config.call, false),
    barkArchiveMode: readString(config.archive_mode, 'inherit'),
    barkRequestTimeoutMs: readNumberString(config.request_timeout_ms, '4000'),
    sqlDatabasePath: readString(config.database_path, './nazh-local.sqlite3'),
    sqlTable: readString(config.table, 'workflow_logs'),
    debugLabel: readString(config.label),
    debugPretty: readBoolean(config.pretty, true),
  };
}

function getPrimaryEditorLabel(nodeType: string): string {
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

function buildNodeConfig(draft: SelectedNodeDraft, currentConfig: NodeConfigMap): NodeConfigMap {
  if (draft.nodeType === 'native') {
    return {
      ...currentConfig,
      message: draft.message,
    };
  }

  if (draft.nodeType === 'timer') {
    return {
      ...currentConfig,
      interval_ms: parsePositiveInteger(draft.timerIntervalMs) ?? 5000,
      immediate: draft.timerImmediate,
      inject: isRecord(currentConfig.inject) ? currentConfig.inject : {},
    };
  }

  if (draft.nodeType === 'serialTrigger') {
    return {
      inject: isRecord(currentConfig.inject) ? currentConfig.inject : {},
    };
  }

  if (draft.nodeType === 'modbusRead') {
    return {
      ...currentConfig,
      unit_id: parsePositiveInteger(draft.modbusUnitId) ?? 1,
      register: parsePositiveInteger(draft.modbusRegister) ?? 40001,
      quantity: parsePositiveInteger(draft.modbusQuantity) ?? 1,
      register_type: draft.modbusRegisterType || 'holding',
      base_value: parseFiniteNumber(draft.modbusBaseValue) ?? 64,
      amplitude: parseFiniteNumber(draft.modbusAmplitude) ?? 6,
    };
  }

  if (draft.nodeType === 'mqttClient') {
    return {
      ...currentConfig,
      mode: draft.mqttMode === 'subscribe' ? 'subscribe' : 'publish',
      topic: draft.mqttTopic.trim(),
      qos: [0, 1, 2].includes(Number(draft.mqttQos)) ? Number(draft.mqttQos) : 0,
      payload_template: draft.mqttPayloadTemplate,
    };
  }

  if (draft.nodeType === 'switch') {
    return {
      ...currentConfig,
      script: draft.script,
      branches: draft.branches.map((branch) => ({
        key: branch.key,
        label: branch.label,
      })),
    };
  }

  if (draft.nodeType === 'httpClient') {
    return {
      ...currentConfig,
      url: draft.httpUrl.trim(),
      method: draft.httpMethod.trim().toUpperCase() || 'POST',
      headers: parseHeadersJson(draft.httpHeaders) ?? (isRecord(currentConfig.headers) ? currentConfig.headers : {}),
      webhook_kind: draft.httpWebhookKind,
      body_mode: normalizeHttpBodyMode(draft.httpBodyMode, draft.httpWebhookKind),
      content_type: draft.httpContentType.trim() || 'application/json',
      request_timeout_ms: parsePositiveInteger(draft.httpRequestTimeoutMs) ?? 4000,
      title_template: draft.httpTitleTemplate,
      body_template: draft.httpBodyTemplate,
      at_mobiles: parseStringList(draft.httpAtMobiles),
      at_all: draft.httpAtAll,
    };
  }

  if (draft.nodeType === 'barkPush') {
    return {
      ...currentConfig,
      server_url: draft.barkServerUrl.trim() || 'https://api.day.app',
      device_key: draft.barkDeviceKey.trim(),
      content_mode: draft.barkContentMode === 'markdown' ? 'markdown' : 'body',
      title_template: draft.barkTitleTemplate,
      subtitle_template: draft.barkSubtitleTemplate,
      body_template: draft.barkBodyTemplate,
      level: ['active', 'timeSensitive', 'passive', 'critical'].includes(draft.barkLevel)
        ? draft.barkLevel
        : 'active',
      badge: parseNonNegativeInteger(draft.barkBadge) ?? '',
      sound: draft.barkSound,
      icon: draft.barkIcon,
      group: draft.barkGroup,
      url: draft.barkUrl,
      copy: draft.barkCopy,
      image: draft.barkImage,
      auto_copy: draft.barkAutoCopy,
      call: draft.barkCall,
      archive_mode: ['inherit', 'archive', 'skip'].includes(draft.barkArchiveMode)
        ? draft.barkArchiveMode
        : 'inherit',
      request_timeout_ms: parsePositiveInteger(draft.barkRequestTimeoutMs) ?? 4000,
    };
  }

  if (draft.nodeType === 'sqlWriter') {
    return {
      ...currentConfig,
      database_path: draft.sqlDatabasePath.trim() || './nazh-local.sqlite3',
      table: draft.sqlTable.trim() || 'workflow_logs',
    };
  }

  if (draft.nodeType === 'debugConsole') {
    return {
      ...currentConfig,
      label: draft.debugLabel.trim(),
      pretty: draft.debugPretty,
    };
  }

  if (supportsScriptAi(draft.nodeType)) {
    const { ai: _currentAi, ...restConfig } = currentConfig;

    return {
      ...restConfig,
      script: draft.script,
    };
  }

  if (isScriptNode(draft.nodeType)) {
    return {
      ...currentConfig,
      script: draft.script,
    };
  }

  return currentConfig;
}

function ThinkingBody({ text }: { text: string }) {
  const ref = useRef<HTMLPreElement>(null);

  useEffect(() => {
    const el = ref.current;
    if (el) {
      el.scrollTop = el.scrollHeight;
    }
  }, [text]);

  return (
    <pre ref={ref} className="flowgram-ai-dialog__thinking-body"><code>{text}</code></pre>
  );
}

function FlowgramNodeSettingsPanel({
  nodeId,
  connections,
  aiProviders,
  activeAiProviderId,
  copilotParams,
}: FlowgramNodeSettingsPanelProps) {
  const panelManager = usePanelManager();
  const { document, playground } = useClientContext();
  const node = document.getNode(nodeId) as FlowNodeEntity | undefined;
  const [draft, setDraft] = useState<SelectedNodeDraft | null>(() => (node ? readNodeDraft(node) : null));
  const [aiDialogOpen, setAiDialogOpen] = useState(false);
  const [aiDialogRequirement, setAiDialogRequirement] = useState('');
  const [aiGenerating, setAiGenerating] = useState(false);
  const [aiGenerateError, setAiGenerateError] = useState<string | null>(null);
  const [aiStreamPreview, setAiStreamPreview] = useState<string | null>(null);
  const [aiThinkingPreview, setAiThinkingPreview] = useState<string | null>(null);

  const closePanel = useCallback(() => {
    panelManager.close(FLOWGRAM_NODE_SETTINGS_PANEL_KEY, 'right');
  }, [panelManager]);

  useEffect(() => {
    if (!node) {
      setDraft(null);
      return;
    }

    setDraft(readNodeDraft(node));
  }, [node, nodeId]);

  useEffect(() => {
    if (!node) {
      return () => {};
    }

    const dispose = node.onExtInfoChange(() => {
      setDraft(readNodeDraft(node));
    });

    return () => dispose.dispose();
  }, [node]);

  useEffect(() => {
    const dispose = playground.config.onReadonlyOrDisabledChange(() => {
      if (playground.config.readonly) {
        closePanel();
      }
    });

    return () => dispose.dispose();
  }, [closePanel, playground]);

  useEffect(() => {
    if (!node) {
      return () => {};
    }

    const dispose = node.onDispose(() => {
      closePanel();
    });

    return () => dispose.dispose();
  }, [closePanel, node]);

  const stats = useMemo(() => {
    if (!node) {
      return null;
    }

    return {
      incoming: node.lines.inputNodes.length,
      outgoing: node.lines.outputNodes.length,
    };
  }, [node]);

  const activeCopilotProvider = useMemo(
    () =>
      activeAiProviderId
        ? aiProviders.find((provider) => provider.id === activeAiProviderId) ?? null
        : null,
    [activeAiProviderId, aiProviders],
  );

  const resolvedGlobalAiProvider = useMemo(
    () =>
      activeCopilotProvider ??
      aiProviders.find((provider) => provider.enabled) ??
      aiProviders[0] ??
      null,
    [activeCopilotProvider, aiProviders],
  );

  const preferredCopilotProvider = useMemo(() => {
    if (isUsableAiProvider(resolvedGlobalAiProvider)) {
      return resolvedGlobalAiProvider;
    }

    return aiProviders.find((provider) => isUsableAiProvider(provider)) ?? null;
  }, [aiProviders, resolvedGlobalAiProvider]);

  const aiGenerateButtonTitle = useMemo(() => {
    if (preferredCopilotProvider) {
      return `使用 ${preferredCopilotProvider.name} 生成 Rhai 脚本`;
    }

    if (aiProviders.length === 0) {
      return '请先在 AI 配置中添加提供商';
    }

    if (activeCopilotProvider && !activeCopilotProvider.enabled) {
      return `全局 AI ${activeCopilotProvider.name} 已被禁用`;
    }

    if (activeCopilotProvider && !activeCopilotProvider.hasApiKey) {
      return `请先为全局 AI ${activeCopilotProvider.name} 配置 API Key`;
    }

    if (aiProviders.some((provider) => provider.enabled)) {
      return '请先为可用的 AI 提供商配置 API Key';
    }

    return '请先启用一个 AI 提供商';
  }, [activeCopilotProvider, aiProviders, preferredCopilotProvider]);

  const diagnostics = useMemo<NodeValidation[]>(() => {
    if (!draft) {
      return [];
    }

    const nextDiagnostics: NodeValidation[] = [];
    const selectedConnection = connections.find((connection) => connection.id === draft.connectionId);
    const parsedTimeoutMs = parseTimeoutMs(draft.timeoutMs);
    const parsedHeaders = parseHeadersJson(draft.httpHeaders);
    const parsedRequestTimeoutMs = parsePositiveInteger(draft.httpRequestTimeoutMs);
    const parsedBarkRequestTimeoutMs = parsePositiveInteger(draft.barkRequestTimeoutMs);
    const parsedBarkBadge = draft.barkBadge.trim()
      ? parseNonNegativeInteger(draft.barkBadge)
      : 0;

    if (stats) {
      if (stats.incoming === 0 && stats.outgoing === 0) {
        nextDiagnostics.push({
          tone: 'warning',
          message: '当前节点是孤立节点。',
        });
      } else if (stats.incoming === 0) {
        nextDiagnostics.push({
          tone: 'info',
          message: '当前节点是入口节点。',
        });
      } else if (stats.outgoing === 0) {
        nextDiagnostics.push({
          tone: 'info',
          message: '当前节点位于流程末端。',
        });
      } else {
        nextDiagnostics.push({
          tone: 'info',
          message: `上游 ${stats.incoming} 条，下游 ${stats.outgoing} 条。`,
        });
      }
    }

    if (isConnectionNode(draft.nodeType)) {
      if (draft.connectionId && !selectedConnection) {
        nextDiagnostics.push({
          tone: 'danger',
          message: `连接 ${draft.connectionId} 未注册。`,
        });
      } else if (selectedConnection) {
        nextDiagnostics.push({
          tone:
            draft.nodeType === 'serialTrigger' && !isSerialConnectionType(selectedConnection.type)
              ? 'danger'
              : 'info',
          message:
            draft.nodeType === 'serialTrigger' && !isSerialConnectionType(selectedConnection.type)
              ? `串口触发节点需要绑定 serial / uart 类型连接，当前为 ${selectedConnection.type}。`
              : `已绑定 ${selectedConnection.id} · ${selectedConnection.type}`,
        });
      } else if (draft.nodeType === 'serialTrigger') {
        nextDiagnostics.push({
          tone: 'danger',
          message: '串口触发节点需要在连接资源中绑定一个串口连接。',
        });
      } else if (connections.length > 0) {
        nextDiagnostics.push({
          tone: 'warning',
          message: '当前节点未绑定连接资源。',
        });
      }
    }

    if (draft.timeoutMs.trim() && parsedTimeoutMs === null) {
      nextDiagnostics.push({
        tone: 'danger',
        message: '超时值必须是大于 0 的数字。',
      });
    }

    if (draft.nodeType === 'native' && !draft.message.trim()) {
      nextDiagnostics.push({
        tone: 'warning',
        message: '消息内容为空。',
      });
    }

    if (isScriptNode(draft.nodeType) && !draft.script.trim()) {
      nextDiagnostics.push({
        tone: 'danger',
        message: '脚本为空。',
      });
    }

    if (supportsScriptAi(draft.nodeType)) {
      if (aiProviders.length === 0) {
        nextDiagnostics.push({
          tone: 'warning',
          message: '当前尚未配置全局 AI，运行时将无法完成 AI 调用。',
        });
      } else if (activeAiProviderId && !activeCopilotProvider) {
        nextDiagnostics.push({
          tone: 'danger',
          message: `全局 AI ${activeAiProviderId} 未在配置中找到。`,
        });
      } else if (!resolvedGlobalAiProvider) {
        nextDiagnostics.push({
          tone: 'warning',
          message: '当前还没有选中全局 AI，请先前往 AI 配置页设置。',
        });
      } else if (!resolvedGlobalAiProvider.enabled) {
        nextDiagnostics.push({
          tone: 'danger',
          message: `全局 AI ${resolvedGlobalAiProvider.name} 已被禁用。`,
        });
      } else if (!resolvedGlobalAiProvider.hasApiKey) {
        nextDiagnostics.push({
          tone: 'danger',
          message: `全局 AI ${resolvedGlobalAiProvider.name} 尚未配置 API Key。`,
        });
      } else {
        nextDiagnostics.push({
          tone: 'info',
          message: `默认使用全局 AI · ${resolvedGlobalAiProvider.name}${resolvedGlobalAiProvider.defaultModel.trim() ? ` · ${resolvedGlobalAiProvider.defaultModel.trim()}` : ' · 使用提供商默认模型'}`,
        });
      }
    }

    if (draft.nodeType === 'switch' && draft.branches.length === 0) {
      nextDiagnostics.push({
        tone: 'warning',
        message: 'Switch 节点至少建议保留一个自定义分支。',
      });
    }

    if (draft.nodeType === 'timer' && parsePositiveInteger(draft.timerIntervalMs) === null) {
      nextDiagnostics.push({
        tone: 'danger',
        message: '定时间隔必须是大于 0 的毫秒数。',
      });
    }

    if (
      draft.nodeType === 'modbusRead' &&
      (
        parsePositiveInteger(draft.modbusUnitId) === null ||
        parsePositiveInteger(draft.modbusRegister) === null ||
        parsePositiveInteger(draft.modbusQuantity) === null ||
        parseFiniteNumber(draft.modbusBaseValue) === null ||
        parseFiniteNumber(draft.modbusAmplitude) === null
      )
    ) {
      nextDiagnostics.push({
        tone: 'danger',
        message: 'Modbus 参数必须是有效数字。',
      });
    }

    if (draft.nodeType === 'mqttClient' && !draft.mqttTopic.trim()) {
      nextDiagnostics.push({
        tone: 'danger',
        message: 'MQTT 主题不能为空。',
      });
    }

    if (draft.nodeType === 'httpClient') {
      if (!draft.httpUrl.trim()) {
        nextDiagnostics.push({
          tone: 'danger',
          message: 'HTTP Client 需要配置 URL。',
        });
      }

      if (parsedHeaders === null) {
        nextDiagnostics.push({
          tone: 'danger',
          message: '请求头 JSON 必须是对象。',
        });
      }

      if (parsedRequestTimeoutMs === null) {
        nextDiagnostics.push({
          tone: 'danger',
          message: '请求超时必须是大于 0 的毫秒数。',
        });
      }

      if (draft.httpBodyMode === 'template' && !draft.httpBodyTemplate.trim()) {
        nextDiagnostics.push({
          tone: 'warning',
          message: '自定义模板模式下建议填写消息模板。',
        });
      }

      if (draft.httpBodyMode === 'dingtalk_markdown' && !draft.httpTitleTemplate.trim()) {
        nextDiagnostics.push({
          tone: 'warning',
          message: '钉钉 Markdown 模式建议填写标题模板。',
        });
      }
    }

    if (draft.nodeType === 'barkPush') {
      if (!draft.barkDeviceKey.trim()) {
        nextDiagnostics.push({
          tone: 'danger',
          message: 'Bark 节点需要配置设备 Key 或推送 URL。',
        });
      }

      if (parsedBarkRequestTimeoutMs === null) {
        nextDiagnostics.push({
          tone: 'danger',
          message: 'Bark 请求超时必须是大于 0 的毫秒数。',
        });
      }

      if (draft.barkBadge.trim() && parsedBarkBadge === null) {
        nextDiagnostics.push({
          tone: 'danger',
          message: 'Bark badge 必须是大于等于 0 的整数。',
        });
      }

      if (!draft.barkTitleTemplate.trim() && !draft.barkBodyTemplate.trim()) {
        nextDiagnostics.push({
          tone: 'warning',
          message: '建议至少填写标题模板或消息模板。',
        });
      }
    }

    if (draft.nodeType === 'sqlWriter') {
      if (!draft.sqlDatabasePath.trim()) {
        nextDiagnostics.push({
          tone: 'warning',
          message: '数据库路径为空。',
        });
      }

      if (!draft.sqlTable.trim()) {
        nextDiagnostics.push({
          tone: 'danger',
          message: '表名不能为空。',
        });
      }
    }

    if (draft.nodeType === 'debugConsole') {
      nextDiagnostics.push({
        tone: 'info',
        message: draft.debugPretty ? '当前以格式化 JSON 输出。' : '当前以紧凑 JSON 输出。',
      });
    }

    return nextDiagnostics;
  }, [activeAiProviderId, activeCopilotProvider, aiProviders, connections, draft, resolvedGlobalAiProvider, stats]);

  const branchSummary = useMemo(
    () =>
      draft
        ? getLogicNodeBranchDefinitions(draft.nodeType, {
            branches: draft.branches,
          })
        : [],
    [draft],
  );

  const updateDraft = useCallback(
    (patch: Partial<SelectedNodeDraft>) => {
      if (!node) {
        return;
      }

      setDraft((currentDraft) => {
        const baseDraft = currentDraft ?? readNodeDraft(node);
        const nextDraft = {
          ...baseDraft,
          ...patch,
        };
        const currentExtInfo = (node.getExtInfo() ?? {}) as { config?: unknown };
        const currentConfig = isRecord(currentExtInfo.config)
          ? (currentExtInfo.config as NodeConfigMap)
          : {};

        const nextExtInfo = {
          ...currentExtInfo,
          label: nextDraft.label || nextDraft.id,
          nodeType: nextDraft.nodeType,
          connectionId: nextDraft.connectionId.trim() || null,
          timeoutMs: parseTimeoutMs(nextDraft.timeoutMs),
          config: buildNodeConfig(nextDraft, currentConfig),
        };

        node.updateExtInfo(nextExtInfo);

        if (usesDynamicPorts(nextDraft.nodeType)) {
          window.requestAnimationFrame(() => {
            node.ports.updateDynamicPorts();
          });
        }

        return readNodeDraft(node);
      });
    },
    [node],
  );

  const handleAiGenerate = useCallback(
    async () => {
      if (!node || !draft || !preferredCopilotProvider) {
        return;
      }

      const requirement = aiDialogRequirement.trim();
      if (!requirement) {
        setAiGenerateError('请输入生成需求。');
        return;
      }

      setAiGenerating(true);
      setAiGenerateError(null);
      setAiStreamPreview('');
      setAiThinkingPreview('');
      try {
        const context = getNodeContext(node);
        const script = await generateScriptStream(
          requirement,
          context,
          {
            providerId: preferredCopilotProvider.id,
            model: preferredCopilotProvider.defaultModel,
            params: copilotParams,
          },
          (rawText) => setAiStreamPreview(rawText),
          (thinkingText) => setAiThinkingPreview(thinkingText),
        );
        if (!script) {
          setAiGenerateError('AI 未返回有效代码。');
          return;
        }
        updateDraft({ script });
        setAiGenerateError(null);
        setAiStreamPreview(null);
        setAiThinkingPreview(null);
        setAiDialogOpen(false);
        setAiDialogRequirement('');
      } catch (err) {
        const message = err instanceof Error ? err.message : String(err);
        setAiGenerateError(message || '生成失败，请重试。');
      } finally {
        setAiGenerating(false);
      }
    },
    [aiDialogRequirement, copilotParams, draft, node, preferredCopilotProvider, updateDraft],
  );

  if (!node || !draft || playground.config.readonly) {
    return null;
  }

  return (
    <div className="flowgram-settings-host">
      <section className="flowgram-floating-panel flowgram-floating-panel--node">
      <div className="flowgram-floating-panel__header">
        <h3>节点设置</h3>
        <button type="button" className="ghost flowgram-floating-panel__close" onClick={closePanel}>
          关闭
        </button>
      </div>

      <div className="flowgram-form">
        <label>
          <span>节点 ID</span>
          <input value={draft.id} readOnly />
        </label>
        <label>
          <span>显示名称</span>
          <input value={draft.label} onChange={(event) => updateDraft({ label: event.target.value })} />
        </label>
        <label>
          <span>节点类型</span>
          <input value={draft.nodeType} readOnly />
        </label>
        {isConnectionNode(draft.nodeType) ? (
          <label>
            <span>连接资源</span>
            <select
              value={
                draft.connectionId && !connections.some((connection) => connection.id === draft.connectionId)
                  ? `__missing__:${draft.connectionId}`
                  : draft.connectionId || '__none__'
              }
              onChange={(event) => {
                const value = event.target.value;

                if (value === '__none__') {
                  updateDraft({ connectionId: '' });
                  return;
                }

                if (value.startsWith('__missing__:')) {
                  updateDraft({ connectionId: value.replace('__missing__:', '') });
                  return;
                }

                updateDraft({ connectionId: value });
              }}
              disabled={connections.length === 0 && !draft.connectionId}
            >
              <option value="__none__">未绑定</option>
              {draft.connectionId && !connections.some((connection) => connection.id === draft.connectionId) ? (
                <option value={`__missing__:${draft.connectionId}`}>未注册连接: {draft.connectionId}</option>
              ) : null}
              {connections.map((connection) => (
                <option key={connection.id} value={connection.id}>
                  {connection.id} · {connection.type}
                </option>
              ))}
            </select>
          </label>
        ) : null}
        <label>
          <span>超时 ms</span>
          <input
            value={draft.timeoutMs}
            onChange={(event) => updateDraft({ timeoutMs: event.target.value })}
            placeholder="留空表示不限"
          />
        </label>

        {draft.nodeType === 'native' ? (
          <label>
            <span>{getPrimaryEditorLabel(draft.nodeType)}</span>
            <textarea value={draft.message} onChange={(event) => updateDraft({ message: event.target.value })} />
          </label>
        ) : null}

        {isScriptNode(draft.nodeType) ? (
          <label>
            <span>
              {getPrimaryEditorLabel(draft.nodeType)}
              {draft.nodeType === 'code' ? (
                <button
                  type="button"
                  className="ghost flowgram-btn-ai"
                  disabled={!preferredCopilotProvider || aiGenerating}
                  onClick={() => {
                    setAiGenerateError(null);
                    setAiDialogOpen(true);
                  }}
                  title={aiGenerateButtonTitle}
                >
                  {aiGenerating ? '生成中...' : 'AI 生成'}
                </button>
              ) : null}
            </span>
            <textarea value={draft.script} onChange={(event) => updateDraft({ script: event.target.value })} />
          </label>
        ) : null}

        {draft.nodeType === 'timer' ? (
          <>
            <label>
              <span>触发间隔 ms</span>
              <input
                value={draft.timerIntervalMs}
                onChange={(event) => updateDraft({ timerIntervalMs: event.target.value })}
              />
            </label>
            <label>
              <span>部署后立即触发</span>
              <select
                value={draft.timerImmediate ? 'true' : 'false'}
                onChange={(event) =>
                  updateDraft({ timerImmediate: event.target.value === 'true' })
                }
              >
                <option value="true">是</option>
                <option value="false">否</option>
              </select>
            </label>
          </>
        ) : null}

        {draft.nodeType === 'modbusRead' ? (
          <>
            <label>
              <span>设备单元 ID</span>
              <input
                value={draft.modbusUnitId}
                onChange={(event) => updateDraft({ modbusUnitId: event.target.value })}
              />
            </label>
            <label>
              <span>寄存器地址</span>
              <input
                value={draft.modbusRegister}
                onChange={(event) => updateDraft({ modbusRegister: event.target.value })}
              />
            </label>
            <label>
              <span>读取数量</span>
              <input
                value={draft.modbusQuantity}
                onChange={(event) => updateDraft({ modbusQuantity: event.target.value })}
              />
            </label>
            <label>
              <span>寄存器类型</span>
              <select value={draft.modbusRegisterType} onChange={(event) => updateDraft({ modbusRegisterType: event.target.value })}>
                <option value="holding">Holding Register (03)</option>
                <option value="input">Input Register (04)</option>
                <option value="coil">Coil (01)</option>
                <option value="discrete">Discrete Input (02)</option>
              </select>
            </label>
            <label>
              <span>基准值</span>
              <input
                value={draft.modbusBaseValue}
                onChange={(event) => updateDraft({ modbusBaseValue: event.target.value })}
              />
            </label>
            <label>
              <span>波动幅度</span>
              <input
                value={draft.modbusAmplitude}
                onChange={(event) => updateDraft({ modbusAmplitude: event.target.value })}
              />
            </label>
          </>
        ) : null}

        {draft.nodeType === 'mqttClient' ? (
          <>
            <label>
              <span>工作模式</span>
              <select value={draft.mqttMode} onChange={(event) => updateDraft({ mqttMode: event.target.value })}>
                <option value="publish">发布 (Publish)</option>
                <option value="subscribe">订阅 (Subscribe)</option>
              </select>
            </label>
            <label>
              <span>主题</span>
              <input
                value={draft.mqttTopic}
                onChange={(event) => updateDraft({ mqttTopic: event.target.value })}
                placeholder="sensors/temperature"
              />
            </label>
            <label>
              <span>QoS</span>
              <select value={draft.mqttQos} onChange={(event) => updateDraft({ mqttQos: event.target.value })}>
                <option value="0">0 - 最多一次</option>
                <option value="1">1 - 至少一次</option>
                <option value="2">2 - 恰好一次</option>
              </select>
            </label>
            {draft.mqttMode === 'publish' ? (
              <label>
                <span>载荷模板</span>
                <textarea
                  value={draft.mqttPayloadTemplate}
                  onChange={(event) => updateDraft({ mqttPayloadTemplate: event.target.value })}
                />
              </label>
            ) : null}
          </>
        ) : null}

        {draft.nodeType === 'httpClient' ? (
          <>
            <label>
              <span>Webhook 类型</span>
              <select
                value={draft.httpWebhookKind}
                onChange={(event) => {
                  const webhookKind = event.target.value;
                  updateDraft({
                    httpWebhookKind: webhookKind,
                    httpBodyMode: normalizeHttpBodyMode(draft.httpBodyMode, webhookKind),
                    httpTitleTemplate:
                      webhookKind === 'dingtalk' && !draft.httpTitleTemplate.trim()
                        ? getDefaultHttpAlarmTitleTemplate()
                        : draft.httpTitleTemplate,
                    httpBodyTemplate:
                      webhookKind === 'dingtalk' &&
                      normalizeHttpBodyMode(draft.httpBodyMode, webhookKind) === 'dingtalk_markdown' &&
                      !draft.httpBodyTemplate.trim()
                        ? getDefaultHttpAlarmBodyTemplate()
                        : draft.httpBodyTemplate,
                  });
                }}
              >
                <option value="generic">通用 Webhook</option>
                <option value="dingtalk">钉钉机器人</option>
              </select>
            </label>
            <label>
              <span>请求地址</span>
              <input value={draft.httpUrl} onChange={(event) => updateDraft({ httpUrl: event.target.value })} />
            </label>
            <label>
              <span>请求方法</span>
              <select
                value={draft.httpMethod || 'POST'}
                onChange={(event) => updateDraft({ httpMethod: event.target.value })}
              >
                <option value="POST">POST</option>
                <option value="PUT">PUT</option>
                <option value="PATCH">PATCH</option>
                <option value="GET">GET</option>
              </select>
            </label>
            <label>
              <span>载荷模式</span>
              <select
                value={draft.httpBodyMode}
                onChange={(event) =>
                  updateDraft({
                    httpBodyMode: event.target.value,
                    httpTitleTemplate:
                      event.target.value === 'dingtalk_markdown' && !draft.httpTitleTemplate.trim()
                        ? getDefaultHttpAlarmTitleTemplate()
                        : draft.httpTitleTemplate,
                    httpBodyTemplate:
                      event.target.value === 'dingtalk_markdown' && !draft.httpBodyTemplate.trim()
                        ? getDefaultHttpAlarmBodyTemplate()
                        : draft.httpBodyTemplate,
                  })
                }
              >
                <option value="json">JSON Payload</option>
                <option value="template">自定义模板</option>
                <option value="dingtalk_markdown">钉钉 Markdown</option>
              </select>
            </label>
            <label>
              <span>内容类型</span>
              <input
                value={draft.httpContentType}
                onChange={(event) => updateDraft({ httpContentType: event.target.value })}
              />
            </label>
            <label>
              <span>请求超时 ms</span>
              <input
                value={draft.httpRequestTimeoutMs}
                onChange={(event) => updateDraft({ httpRequestTimeoutMs: event.target.value })}
              />
            </label>
            {draft.httpBodyMode === 'dingtalk_markdown' ? (
              <label>
                <span>标题模板</span>
                <textarea
                  value={draft.httpTitleTemplate}
                  onChange={(event) => updateDraft({ httpTitleTemplate: event.target.value })}
                />
              </label>
            ) : null}
            {draft.httpBodyMode !== 'json' ? (
              <label>
                <span>{draft.httpBodyMode === 'dingtalk_markdown' ? '消息模板' : '请求体模板'}</span>
                <textarea
                  value={draft.httpBodyTemplate}
                  onChange={(event) => updateDraft({ httpBodyTemplate: event.target.value })}
                />
              </label>
            ) : null}
            {draft.httpWebhookKind === 'dingtalk' ? (
              <>
                <label>
                  <span>@ 手机号</span>
                  <input
                    value={draft.httpAtMobiles}
                    onChange={(event) => updateDraft({ httpAtMobiles: event.target.value })}
                    placeholder="13800000000, 13900000000"
                  />
                </label>
                <label>
                  <span>@ 所有人</span>
                  <select
                    value={draft.httpAtAll ? 'true' : 'false'}
                    onChange={(event) => updateDraft({ httpAtAll: event.target.value === 'true' })}
                  >
                    <option value="false">否</option>
                    <option value="true">是</option>
                  </select>
                </label>
              </>
            ) : null}
            <label>
              <span>请求头 JSON</span>
              <textarea
                value={draft.httpHeaders}
                onChange={(event) => updateDraft({ httpHeaders: event.target.value })}
              />
            </label>
          </>
        ) : null}

        {draft.nodeType === 'barkPush' ? (
          <>
            <label>
              <span>服务地址</span>
              <input
                value={draft.barkServerUrl}
                onChange={(event) => updateDraft({ barkServerUrl: event.target.value })}
                placeholder="https://api.day.app"
              />
            </label>
            <label>
              <span>设备 Key / 推送 URL</span>
              <input
                value={draft.barkDeviceKey}
                onChange={(event) => updateDraft({ barkDeviceKey: event.target.value })}
                placeholder="填写 device_key，或粘贴 https://api.day.app/{key}"
              />
            </label>
            <label>
              <span>内容模式</span>
              <select
                value={draft.barkContentMode}
                onChange={(event) => updateDraft({ barkContentMode: event.target.value })}
              >
                <option value="body">普通文本</option>
                <option value="markdown">Markdown</option>
              </select>
            </label>
            <label>
              <span>中断级别</span>
              <select
                value={draft.barkLevel}
                onChange={(event) => updateDraft({ barkLevel: event.target.value })}
              >
                <option value="active">active</option>
                <option value="timeSensitive">timeSensitive</option>
                <option value="passive">passive</option>
                <option value="critical">critical</option>
              </select>
            </label>
            <label>
              <span>标题模板</span>
              <input
                value={draft.barkTitleTemplate}
                onChange={(event) => updateDraft({ barkTitleTemplate: event.target.value })}
              />
            </label>
            <label>
              <span>副标题模板</span>
              <input
                value={draft.barkSubtitleTemplate}
                onChange={(event) => updateDraft({ barkSubtitleTemplate: event.target.value })}
              />
            </label>
            <label>
              <span>{draft.barkContentMode === 'markdown' ? 'Markdown 模板' : '消息模板'}</span>
              <textarea
                value={draft.barkBodyTemplate}
                onChange={(event) => updateDraft({ barkBodyTemplate: event.target.value })}
              />
            </label>
            <label>
              <span>分组</span>
              <input
                value={draft.barkGroup}
                onChange={(event) => updateDraft({ barkGroup: event.target.value })}
                placeholder="nazh-alert"
              />
            </label>
            <label>
              <span>点击跳转 URL</span>
              <input
                value={draft.barkUrl}
                onChange={(event) => updateDraft({ barkUrl: event.target.value })}
                placeholder="支持 URL Scheme 或 https://"
              />
            </label>
            <label>
              <span>铃声</span>
              <input
                value={draft.barkSound}
                onChange={(event) => updateDraft({ barkSound: event.target.value })}
                placeholder="minuet"
              />
            </label>
            <label>
              <span>Badge</span>
              <input
                value={draft.barkBadge}
                onChange={(event) => updateDraft({ barkBadge: event.target.value })}
                placeholder="0"
              />
            </label>
            <label>
              <span>图标 URL</span>
              <input
                value={draft.barkIcon}
                onChange={(event) => updateDraft({ barkIcon: event.target.value })}
              />
            </label>
            <label>
              <span>图片 URL</span>
              <input
                value={draft.barkImage}
                onChange={(event) => updateDraft({ barkImage: event.target.value })}
              />
            </label>
            <label>
              <span>复制内容</span>
              <input
                value={draft.barkCopy}
                onChange={(event) => updateDraft({ barkCopy: event.target.value })}
                placeholder="留空时不附带 copy 字段"
              />
            </label>
            <label>
              <span>自动复制</span>
              <select
                value={draft.barkAutoCopy ? 'true' : 'false'}
                onChange={(event) => updateDraft({ barkAutoCopy: event.target.value === 'true' })}
              >
                <option value="false">否</option>
                <option value="true">是</option>
              </select>
            </label>
            <label>
              <span>重复响铃</span>
              <select
                value={draft.barkCall ? 'true' : 'false'}
                onChange={(event) => updateDraft({ barkCall: event.target.value === 'true' })}
              >
                <option value="false">否</option>
                <option value="true">是</option>
              </select>
            </label>
            <label>
              <span>历史归档</span>
              <select
                value={draft.barkArchiveMode}
                onChange={(event) => updateDraft({ barkArchiveMode: event.target.value })}
              >
                <option value="inherit">跟随 Bark App 设置</option>
                <option value="archive">强制保存</option>
                <option value="skip">不保存</option>
              </select>
            </label>
            <label>
              <span>请求超时 ms</span>
              <input
                value={draft.barkRequestTimeoutMs}
                onChange={(event) => updateDraft({ barkRequestTimeoutMs: event.target.value })}
              />
            </label>
          </>
        ) : null}

        {draft.nodeType === 'sqlWriter' ? (
          <>
            <label>
              <span>数据库路径</span>
              <input
                value={draft.sqlDatabasePath}
                onChange={(event) => updateDraft({ sqlDatabasePath: event.target.value })}
              />
            </label>
            <label>
              <span>表名</span>
              <input value={draft.sqlTable} onChange={(event) => updateDraft({ sqlTable: event.target.value })} />
            </label>
          </>
        ) : null}

        {draft.nodeType === 'debugConsole' ? (
          <>
            <label>
              <span>输出标签</span>
              <input
                value={draft.debugLabel}
                onChange={(event) => updateDraft({ debugLabel: event.target.value })}
              />
            </label>
            <label>
              <span>输出格式</span>
              <select
                value={draft.debugPretty ? 'pretty' : 'compact'}
                onChange={(event) => updateDraft({ debugPretty: event.target.value === 'pretty' })}
              >
                <option value="pretty">格式化 JSON</option>
                <option value="compact">紧凑 JSON</option>
              </select>
            </label>
          </>
        ) : null}
      </div>

      {draft.nodeType === 'switch' ? (
        <section className="flowgram-panel flowgram-panel--branches">
          <div className="flowgram-panel__header">
            <h4>分支设置</h4>
          </div>

          <div className="flowgram-branch-editor">
            {draft.branches.map((branch, index) => (
              <div key={`${branch.key}:${index}`} className="flowgram-branch-editor__row">
                <input
                  value={branch.key}
                  onChange={(event) => {
                    const nextBranches = draft.branches.map((item, itemIndex) =>
                      itemIndex === index
                        ? { ...item, key: event.target.value }
                        : item,
                    );
                    updateDraft({ branches: nextBranches });
                  }}
                  placeholder="branch_key"
                />
                <input
                  value={branch.label}
                  onChange={(event) => {
                    const nextBranches = draft.branches.map((item, itemIndex) =>
                      itemIndex === index
                        ? { ...item, label: event.target.value }
                        : item,
                    );
                    updateDraft({ branches: nextBranches });
                  }}
                  placeholder="显示名称"
                />
                <button
                  type="button"
                  className="ghost"
                  onClick={() =>
                    updateDraft({
                      branches: draft.branches.filter((_, itemIndex) => itemIndex !== index),
                    })
                  }
                >
                  删除
                </button>
              </div>
            ))}

            <button
              type="button"
              className="ghost"
              onClick={() =>
                updateDraft({
                  branches: [
                    ...draft.branches,
                    {
                      key: `branch_${draft.branches.length + 1}`,
                      label: `Branch ${draft.branches.length + 1}`,
                    },
                  ],
                })
              }
            >
              添加分支
            </button>
          </div>
        </section>
      ) : null}

      {branchSummary.length > 0 ? (
        <section className="flowgram-panel flowgram-panel--branches">
          <div className="flowgram-panel__header">
            <h4>输出分支</h4>
          </div>
          <div className="flowgram-branch-list">
            {branchSummary.map((branch) => (
              <span key={branch.key} className="flowgram-branch-pill">
                {branch.label}
              </span>
            ))}
          </div>
        </section>
      ) : null}

      {stats ? (
        <div className="flowgram-stats">
          <article>
            <span>上游</span>
            <strong>{stats.incoming}</strong>
          </article>
          <article>
            <span>下游</span>
            <strong>{stats.outgoing}</strong>
          </article>
        </div>
      ) : null}

      <div className="flowgram-notes">
        {diagnostics.map((note) => (
          <article
            key={`${note.tone}:${note.message}`}
            className={`flowgram-note flowgram-note--${note.tone}`}
          >
            {note.message}
          </article>
        ))}
        {aiGenerateError ? (
          <article className="flowgram-note flowgram-note--danger">{aiGenerateError}</article>
        ) : null}
      </div>
    </section>

      {aiDialogOpen ? (
        <div
          className="flowgram-ai-dialog-layer"
          onClick={() => {
            if (!aiGenerating) {
              setAiDialogOpen(false);
              setAiDialogRequirement('');
              setAiGenerateError(null);
              setAiStreamPreview(null);
              setAiThinkingPreview(null);
            }
          }}
        >
          <div
            className="flowgram-ai-dialog"
            role="dialog"
            aria-modal="true"
            aria-labelledby="flowgram-ai-dialog-title"
            onClick={(event) => event.stopPropagation()}
          >
            <strong id="flowgram-ai-dialog-title">AI 脚本生成</strong>
            <p className="flowgram-ai-dialog__hint">描述你希望脚本实现的功能，AI 将生成 Rhai 代码。</p>
            <textarea
              className="flowgram-ai-dialog__textarea"
              value={aiDialogRequirement}
              onChange={(event) => {
                setAiGenerateError(null);
                setAiDialogRequirement(event.target.value);
              }}
              placeholder="例如：将摄氏温度转为华氏温度，并添加严重级别字段"
              disabled={aiGenerating}
              autoFocus
            />
            {aiGenerateError ? (
              <article className="flowgram-note flowgram-note--danger">{aiGenerateError}</article>
            ) : null}
            {aiThinkingPreview !== null && aiThinkingPreview.length > 0 ? (
              <details className="flowgram-ai-dialog__thinking" open={aiStreamPreview === '' || aiStreamPreview === null}>
                <summary className="flowgram-ai-dialog__thinking-toggle">
                  <span>思考过程</span>
                  <span className="flowgram-ai-dialog__thinking-badge">{aiStreamPreview ? '完成' : '思考中...'}</span>
                </summary>
                <ThinkingBody text={aiThinkingPreview} />
              </details>
            ) : null}
            {aiStreamPreview !== null && aiStreamPreview.length > 0 ? (
              <pre className="flowgram-ai-dialog__preview"><code>{aiStreamPreview}</code></pre>
            ) : null}
            <div className="flowgram-ai-dialog__actions">
              <button
                type="button"
                className="flowgram-ai-dialog__action"
                disabled={aiGenerating}
                onClick={() => {
                  setAiDialogOpen(false);
                  setAiDialogRequirement('');
                  setAiGenerateError(null);
                  setAiStreamPreview(null);
                  setAiThinkingPreview(null);
                }}
              >
                取消
              </button>
              <button
                type="button"
                className="flowgram-ai-dialog__action flowgram-ai-dialog__action--primary"
                disabled={aiGenerating || !aiDialogRequirement.trim()}
                onClick={() => {
                  void handleAiGenerate();
                }}
              >
                {aiGenerating ? '生成中...' : '生成'}
              </button>
            </div>
          </div>
        </div>
      ) : null}
    </div>
  );
}

export const flowgramNodeSettingsPanelFactory: PanelFactory<FlowgramNodeSettingsPanelProps> = {
  key: FLOWGRAM_NODE_SETTINGS_PANEL_KEY,
  render: (props) => <FlowgramNodeSettingsPanel key={props.nodeId} {...props} />,
};
