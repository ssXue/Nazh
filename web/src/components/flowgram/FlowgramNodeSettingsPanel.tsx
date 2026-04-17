import { useCallback, useEffect, useMemo, useState } from 'react';

import { type FlowNodeEntity, useClientContext } from '@flowgram.ai/free-layout-editor';
import { type PanelFactory, usePanelManager } from '@flowgram.ai/panel-manager-plugin';

import {
  getLogicNodeBranchDefinitions,
  getDefaultHttpAlarmBodyTemplate,
  getDefaultHttpAlarmTitleTemplate,
  inferHttpWebhookKind,
  normalizeHttpBodyMode,
  parseTimeoutMs,
  type FlowgramLogicBranch,
} from './flowgram-node-library';
import { AiScriptGenerator } from './AiScriptGenerator';
import { generateScript, getNodeContext } from '../../lib/script-generation';
import type { AiProviderView, ConnectionDefinition } from '../../types';

export interface FlowgramNodeSettingsPanelProps {
  nodeId: string;
  connections: ConnectionDefinition[];
  aiProviders: AiProviderView[];
  activeAiProviderId: string | null;
}

interface SelectedNodeDraft {
  id: string;
  nodeType: string;
  label: string;
  connectionId: string;
  aiDescription: string;
  timeoutMs: string;
  message: string;
  script: string;
  aiEnabled: boolean;
  aiProviderId: string;
  aiModel: string;
  aiSystemPrompt: string;
  aiTemperature: string;
  aiMaxTokens: string;
  aiTopP: string;
  aiTimeoutMs: string;
  branches: FlowgramLogicBranch[];
  timerIntervalMs: string;
  timerImmediate: boolean;
  modbusUnitId: string;
  modbusRegister: string;
  modbusQuantity: string;
  modbusBaseValue: string;
  modbusAmplitude: string;
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
  return nodeType === 'native' || nodeType === 'modbusRead' || nodeType === 'serialTrigger';
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
    nodeType === 'rhai' ||
    nodeType === 'if' ||
    nodeType === 'switch' ||
    nodeType === 'tryCatch' ||
    nodeType === 'loop'
  );
}

function supportsScriptAi(nodeType: string): boolean {
  return nodeType === 'code' || nodeType === 'rhai';
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
    aiDescription?: string | null;
    timeoutMs?: number | null;
    config?: unknown;
  };
  const config = isRecord(rawData.config) ? rawData.config : {};
  const nodeType = String(rawData.nodeType ?? node.flowNodeType);
  const aiConfig = isRecord(config.ai) ? config.ai : null;
  const httpUrl = readString(config.url);
  const httpWebhookKind = readString(config.webhook_kind, inferHttpWebhookKind(httpUrl));
  const httpBodyMode = normalizeHttpBodyMode(config.body_mode, httpWebhookKind);

  return {
    id: node.id,
    nodeType,
    label: rawData.label ?? node.id,
    connectionId: rawData.connectionId ?? '',
    aiDescription: rawData.aiDescription ?? '',
    timeoutMs: rawData.timeoutMs ? String(rawData.timeoutMs) : '',
    message: readString(config.message),
    script: readString(config.script),
    aiEnabled: supportsScriptAi(nodeType) && aiConfig !== null,
    aiProviderId: aiConfig ? readString(aiConfig.providerId) : '',
    aiModel: aiConfig ? readString(aiConfig.model) : '',
    aiSystemPrompt: aiConfig ? readString(aiConfig.systemPrompt) : '',
    aiTemperature: aiConfig ? readNumberString(aiConfig.temperature, '') : '',
    aiMaxTokens: aiConfig ? readNumberString(aiConfig.maxTokens, '') : '',
    aiTopP: aiConfig ? readNumberString(aiConfig.topP, '') : '',
    aiTimeoutMs: aiConfig ? readNumberString(aiConfig.timeoutMs, '') : '',
    branches: readEditableBranches(nodeType, config),
    timerIntervalMs: readNumberString(config.interval_ms, '5000'),
    timerImmediate: readBoolean(config.immediate, true),
    modbusUnitId: readNumberString(config.unit_id, '1'),
    modbusRegister: readNumberString(config.register, '40001'),
    modbusQuantity: readNumberString(config.quantity, '1'),
    modbusBaseValue: readNumberString(config.base_value, '64'),
    modbusAmplitude: readNumberString(config.amplitude, '6'),
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
    case 'rhai':
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
      base_value: parseFiniteNumber(draft.modbusBaseValue) ?? 64,
      amplitude: parseFiniteNumber(draft.modbusAmplitude) ?? 6,
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
    const aiConfig = draft.aiEnabled
      ? {
          providerId: draft.aiProviderId.trim(),
          ...(draft.aiModel.trim() ? { model: draft.aiModel.trim() } : {}),
          ...(draft.aiSystemPrompt.trim() ? { systemPrompt: draft.aiSystemPrompt.trim() } : {}),
          ...(parseFiniteNumber(draft.aiTemperature) !== null
            ? { temperature: parseFiniteNumber(draft.aiTemperature) }
            : {}),
          ...(parsePositiveInteger(draft.aiMaxTokens) !== null
            ? { maxTokens: parsePositiveInteger(draft.aiMaxTokens) }
            : {}),
          ...(parseFiniteNumber(draft.aiTopP) !== null
            ? { topP: parseFiniteNumber(draft.aiTopP) }
            : {}),
          ...(parsePositiveInteger(draft.aiTimeoutMs) !== null
            ? { timeoutMs: parsePositiveInteger(draft.aiTimeoutMs) }
            : {}),
        }
      : null;

    return {
      ...restConfig,
      script: draft.script,
      ...(aiConfig ? { ai: aiConfig } : {}),
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

function FlowgramNodeSettingsPanel({
  nodeId,
  connections,
  aiProviders,
  activeAiProviderId,
}: FlowgramNodeSettingsPanelProps) {
  const panelManager = usePanelManager();
  const { document, playground } = useClientContext();
  const node = document.getNode(nodeId) as FlowNodeEntity | undefined;
  const [draft, setDraft] = useState<SelectedNodeDraft | null>(() => (node ? readNodeDraft(node) : null));
  const [aiGeneratorOpen, setAiGeneratorOpen] = useState(false);
  const [aiGenerating, setAiGenerating] = useState(false);
  const [aiGenerateError, setAiGenerateError] = useState<string | null>(null);

  const closePanel = useCallback(() => {
    panelManager.close(FLOWGRAM_NODE_SETTINGS_PANEL_KEY, 'right');
  }, [panelManager]);

  const hasAiProvider = aiProviders.length > 0 && !!activeAiProviderId;

  const handleAiGeneratorClose = useCallback(() => {
    if (aiGenerating) {
      return;
    }
    setAiGeneratorOpen(false);
    setAiGenerateError(null);
  }, [aiGenerating]);

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
    const dispose = document.selectServices.onSelectionChanged(() => {
      const selectedNodes = document.selectServices.selectedNodes;

      if (selectedNodes.length !== 1 || selectedNodes[0]?.id !== nodeId) {
        closePanel();
      }
    });

    return () => dispose.dispose();
  }, [closePanel, document, nodeId]);

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

  const aiProviderChoices = useMemo(() => {
    if (aiProviders.length === 0) {
      return [];
    }

    const activeProvider = activeAiProviderId
      ? aiProviders.find((provider) => provider.id === activeAiProviderId) ?? null
      : null;
    const remainingProviders = aiProviders.filter((provider) => provider.id !== activeProvider?.id);

    return activeProvider ? [activeProvider, ...remainingProviders] : aiProviders;
  }, [activeAiProviderId, aiProviders]);

  const fallbackAiProvider = useMemo(
    () =>
      aiProviderChoices.find((provider) => provider.id === activeAiProviderId) ??
      aiProviderChoices.find((provider) => provider.enabled) ??
      aiProviderChoices[0] ??
      null,
    [activeAiProviderId, aiProviderChoices],
  );

  const diagnostics = useMemo<NodeValidation[]>(() => {
    if (!draft) {
      return [];
    }

    const nextDiagnostics: NodeValidation[] = [];
    const selectedConnection = connections.find((connection) => connection.id === draft.connectionId);
    const selectedAiProvider = aiProviders.find((provider) => provider.id === draft.aiProviderId) ?? null;
    const parsedTimeoutMs = parseTimeoutMs(draft.timeoutMs);
    const parsedHeaders = parseHeadersJson(draft.httpHeaders);
    const parsedRequestTimeoutMs = parsePositiveInteger(draft.httpRequestTimeoutMs);

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

    if (supportsScriptAi(draft.nodeType) && draft.aiEnabled) {
      if (!draft.aiProviderId.trim()) {
        nextDiagnostics.push({
          tone: 'danger',
          message: '启用 AI 后必须填写 providerId。',
        });
      } else if (aiProviders.length === 0) {
        nextDiagnostics.push({
          tone: 'warning',
          message: '当前尚未配置任何 AI 提供商，运行时将无法完成 AI 调用。',
        });
      } else if (!selectedAiProvider) {
        nextDiagnostics.push({
          tone: 'danger',
          message: `AI 提供商 ${draft.aiProviderId} 未在全局配置中注册。`,
        });
      } else if (!selectedAiProvider.enabled) {
        nextDiagnostics.push({
          tone: 'danger',
          message: `AI 提供商 ${selectedAiProvider.name} 已被禁用。`,
        });
      } else {
        nextDiagnostics.push({
          tone: 'info',
          message: `AI 已启用 · ${selectedAiProvider.name}${draft.aiModel.trim() ? ` · ${draft.aiModel.trim()}` : ' · 使用提供商默认模型'}`,
        });
      }

      if (draft.aiTemperature.trim() && parseFiniteNumber(draft.aiTemperature) === null) {
        nextDiagnostics.push({
          tone: 'danger',
          message: 'Temperature 必须是有效数字。',
        });
      }

      if (draft.aiMaxTokens.trim() && parsePositiveInteger(draft.aiMaxTokens) === null) {
        nextDiagnostics.push({
          tone: 'danger',
          message: 'Max tokens 必须是大于 0 的整数。',
        });
      }

      if (draft.aiTopP.trim() && parseFiniteNumber(draft.aiTopP) === null) {
        nextDiagnostics.push({
          tone: 'danger',
          message: 'Top P 必须是有效数字。',
        });
      }

      if (draft.aiTimeoutMs.trim() && parsePositiveInteger(draft.aiTimeoutMs) === null) {
        nextDiagnostics.push({
          tone: 'danger',
          message: 'AI 超时必须是大于 0 的毫秒数。',
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
  }, [aiProviders, connections, draft, stats]);

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
          aiDescription: nextDraft.aiDescription.trim() || null,
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
    async (requirement: string) => {
      if (!node || !activeAiProviderId) {
        return;
      }
      setAiGenerating(true);
      setAiGenerateError(null);
      try {
        const context = getNodeContext(node);
        const script = await generateScript(requirement, context, {
          providerId: activeAiProviderId,
        });
        if (!script) {
          setAiGenerateError('AI 未返回有效代码。');
          return;
        }
        updateDraft({ script });
        setAiGeneratorOpen(false);
        setAiGenerateError(null);
      } catch (err) {
        const message = err instanceof Error ? err.message : String(err);
        setAiGenerateError(message || '生成失败，请重试。');
      } finally {
        setAiGenerating(false);
      }
    },
    [node, activeAiProviderId, updateDraft],
  );

  if (!node || !draft || playground.config.readonly) {
    return null;
  }

  return (
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
          <span>AI 描述</span>
          <textarea
            value={draft.aiDescription}
            onChange={(event) => updateDraft({ aiDescription: event.target.value })}
          />
        </label>
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
              {(draft.nodeType === 'code' || draft.nodeType === 'rhai') ? (
                <button
                  type="button"
                  className="ghost flowgram-btn-ai"
                  disabled={!hasAiProvider || aiGenerating}
                  onClick={() => {
                    setAiGeneratorOpen(true);
                    setAiGenerateError(null);
                  }}
                  title={!hasAiProvider ? '请先在 AI 配置中添加提供商' : 'AI 生成脚本'}
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

      {supportsScriptAi(draft.nodeType) ? (
        <section className="flowgram-panel flowgram-panel--branches">
          <div className="flowgram-panel__header">
            <h4>AI 能力</h4>
          </div>
          <p className="flowgram-panel__subtle">
            启用后可在脚本里调用 <code>ai_complete(prompt)</code>，返回模型生成的文本。
          </p>

          <div className="flowgram-form">
            <label>
              <span>启用 AI</span>
              <select
                value={draft.aiEnabled ? 'true' : 'false'}
                onChange={(event) => {
                  const nextEnabled = event.target.value === 'true';
                  const nextProvider = nextEnabled && !draft.aiProviderId.trim() ? fallbackAiProvider : null;
                  updateDraft({
                    aiEnabled: nextEnabled,
                    aiProviderId:
                      nextEnabled && !draft.aiProviderId.trim()
                        ? nextProvider?.id ?? ''
                        : draft.aiProviderId,
                    aiModel:
                      nextEnabled && !draft.aiModel.trim()
                        ? nextProvider?.defaultModel ?? draft.aiModel
                        : draft.aiModel,
                  });
                }}
              >
                <option value="false">关闭</option>
                <option value="true">启用</option>
              </select>
            </label>

            {draft.aiEnabled ? (
              <>
                <label>
                  <span>Provider</span>
                  {aiProviderChoices.length > 0 ? (
                    <select
                      value={draft.aiProviderId}
                      onChange={(event) => {
                        const nextProviderId = event.target.value;
                        const nextProvider =
                          aiProviderChoices.find((provider) => provider.id === nextProviderId) ?? null;
                        const currentProvider =
                          aiProviderChoices.find((provider) => provider.id === draft.aiProviderId) ?? null;
                        const shouldAdoptDefaultModel =
                          !draft.aiModel.trim() ||
                          (currentProvider?.defaultModel?.trim() &&
                            draft.aiModel.trim() === currentProvider.defaultModel.trim());

                        updateDraft({
                          aiProviderId: nextProviderId,
                          aiModel:
                            shouldAdoptDefaultModel && nextProvider
                              ? nextProvider.defaultModel
                              : draft.aiModel,
                        });
                      }}
                    >
                      <option value="">请选择提供商</option>
                      {aiProviderChoices.map((provider) => (
                        <option key={provider.id} value={provider.id}>
                          {provider.name} · {provider.id}
                          {provider.enabled ? '' : '（已禁用）'}
                        </option>
                      ))}
                    </select>
                  ) : (
                    <input
                      value={draft.aiProviderId}
                      onChange={(event) => updateDraft({ aiProviderId: event.target.value })}
                      placeholder="provider-id"
                    />
                  )}
                </label>
                <label>
                  <span>模型覆盖</span>
                  <input
                    value={draft.aiModel}
                    onChange={(event) => updateDraft({ aiModel: event.target.value })}
                    placeholder="留空表示使用提供商默认模型"
                  />
                </label>
                <label>
                  <span>System Prompt</span>
                  <textarea
                    value={draft.aiSystemPrompt}
                    onChange={(event) => updateDraft({ aiSystemPrompt: event.target.value })}
                    placeholder="可选：补充角色、风格或输出约束"
                  />
                </label>
                <label>
                  <span>Temperature</span>
                  <input
                    value={draft.aiTemperature}
                    onChange={(event) => updateDraft({ aiTemperature: event.target.value })}
                    placeholder="例如 0.2"
                  />
                </label>
                <label>
                  <span>Max Tokens</span>
                  <input
                    value={draft.aiMaxTokens}
                    onChange={(event) => updateDraft({ aiMaxTokens: event.target.value })}
                    placeholder="留空使用默认值"
                  />
                </label>
                <label>
                  <span>Top P</span>
                  <input
                    value={draft.aiTopP}
                    onChange={(event) => updateDraft({ aiTopP: event.target.value })}
                    placeholder="例如 0.9"
                  />
                </label>
                <label>
                  <span>AI 超时 ms</span>
                  <input
                    value={draft.aiTimeoutMs}
                    onChange={(event) => updateDraft({ aiTimeoutMs: event.target.value })}
                    placeholder="留空使用默认值"
                  />
                </label>
              </>
            ) : null}
          </div>
        </section>
      ) : null}

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
      </div>
      <AiScriptGenerator
        open={aiGeneratorOpen}
        loading={aiGenerating}
        error={aiGenerateError}
        onGenerate={handleAiGenerate}
        onClose={handleAiGeneratorClose}
      />
    </section>
  );
}

export const flowgramNodeSettingsPanelFactory: PanelFactory<FlowgramNodeSettingsPanelProps> = {
  key: FLOWGRAM_NODE_SETTINGS_PANEL_KEY,
  render: (props) => <FlowgramNodeSettingsPanel {...props} />,
};
