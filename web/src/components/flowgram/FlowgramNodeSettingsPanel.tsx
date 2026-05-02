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
  resolveNodeDisplayLabel,
  type FlowgramLogicBranch,
  type NazhNodeKind,
  getNodeDefinition,
} from './flowgram-node-library';
import type { NodeValidationContext } from './nodes/shared';
import { generateScriptStream, getNodeContext } from '../../lib/script-generation';
import type { AiGenerationParams, AiProviderView, ConnectionDefinition } from '../../types';

import {
  type SelectedNodeDraft,
  type NodeValidation,
  type NodeSettingsProps,
  type FieldValidatorResult,
  isRecord,
  readString,
  readBoolean,
  readNumberString,
  parsePositiveInteger,
  parseNonNegativeInteger,
  parseFiniteNumber,
  readConnectionMetadataString,
  supportsConnectionBinding,
  connectionMatchesNodeType,
  compatibleConnectionHint,
  isScriptNode,
  supportsScriptAi,
  isUsableAiProvider,
  usesDynamicPorts,
  validateConnectionBinding,
} from './nodes/settings-shared';

import { NativeNodeSettings } from './nodes/native/settings';
import { CodeNodeSettings } from './nodes/code/settings';
import { TimerNodeSettings } from './nodes/timer/settings';
import { SerialTriggerNodeSettings } from './nodes/serialTrigger/settings';
import { ModbusReadNodeSettings } from './nodes/modbusRead/settings';
import { MqttClientNodeSettings } from './nodes/mqttClient/settings';
import { IfNodeSettings } from './nodes/if/settings';
import { SwitchNodeSettings } from './nodes/switch/settings';
import { TryCatchNodeSettings } from './nodes/tryCatch/settings';
import { LoopNodeSettings } from './nodes/loop/settings';
import { HttpClientNodeSettings } from './nodes/httpClient/settings';
import { BarkPushNodeSettings } from './nodes/barkPush/settings';
import { SqlWriterNodeSettings } from './nodes/sqlWriter/settings';
import { DebugConsoleNodeSettings } from './nodes/debugConsole/settings';
import { LookupNodeSettings } from './nodes/lookup/settings';
import { SubgraphNodeSettings } from './nodes/subgraph/settings';
import { HumanLoopNodeSettings } from './nodes/humanLoop/settings';

export interface FlowgramNodeSettingsPanelProps {
  nodeId: string;
  connections: ConnectionDefinition[];
  aiProviders: AiProviderView[];
  activeAiProviderId: string | null;
  copilotParams: AiGenerationParams;
}

type NodeConfigMap = Record<string, unknown>;

export const FLOWGRAM_NODE_SETTINGS_PANEL_KEY = 'nazh-flowgram-node-settings';

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
  const httpWebhookKind = readString(
    config.webhook_kind,
    inferHttpWebhookKind(readString(config.url)),
  );
  const httpBodyMode = normalizeHttpBodyMode(config.body_mode, httpWebhookKind);

  return {
    id: node.id,
    nodeType,
    label: resolveNodeDisplayLabel(nodeType, rawData.label),
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
    httpWebhookKind,
    httpBodyMode,
    httpTitleTemplate: readString(config.title_template, getDefaultHttpAlarmTitleTemplate()),
    httpBodyTemplate: readString(
      config.body_template,
      httpBodyMode === 'dingtalk_markdown' ? getDefaultHttpAlarmBodyTemplate() : '',
    ),
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
    sqlDatabasePath: readString(config.database_path, './nazh-local.sqlite3'),
    sqlTable: readString(config.table, 'workflow_logs'),
    debugLabel: readString(config.label),
    debugPretty: readBoolean(config.pretty, true),
    parameterBindings: (() => {
      const raw = config.parameterBindings;
      if (typeof raw === 'object' && raw !== null && !Array.isArray(raw)) {
        return raw as Record<string, string | number | boolean>;
      }
      return {};
    })(),
    lookupTable: (() => {
      const raw = config.table;
      if (typeof raw === 'object' && raw !== null && !Array.isArray(raw)) {
        return raw as Record<string, unknown>;
      }
      return {};
    })(),
    lookupDefault: (() => {
      const raw = config.default;
      return raw === null || raw === undefined ? '' : JSON.stringify(raw);
    })(),
    hitlTitle: readString(config.title),
    hitlDescription: readString(config.description),
    hitlApprovalTimeoutSec: (() => {
      const raw = config.approval_timeout_ms;
      return typeof raw === 'number' && raw > 0 ? String(Math.round(raw / 1000)) : '';
    })(),
    hitlDefaultAction: (() => {
      const raw = config.default_action;
      return typeof raw === 'string' && ['autoApprove', 'autoReject'].includes(raw) ? raw : 'autoReject';
    })(),
    hitlFormSchemaJson: (() => {
      const raw = config.form_schema;
      if (Array.isArray(raw) && raw.length > 0) return JSON.stringify(raw, null, 2);
      return '';
    })(),
  };
}

function buildNodeConfig(draft: SelectedNodeDraft, currentConfig: NodeConfigMap): NodeConfigMap {
  if (draft.nodeType === 'native') {
    return { ...currentConfig, message: draft.message };
  }

  if (draft.nodeType === 'timer') {
    return { ...currentConfig, interval_ms: parsePositiveInteger(draft.timerIntervalMs) ?? 5000, immediate: draft.timerImmediate, inject: isRecord(currentConfig.inject) ? currentConfig.inject : {} };
  }

  if (draft.nodeType === 'serialTrigger') {
    return { inject: isRecord(currentConfig.inject) ? currentConfig.inject : {} };
  }

  if (draft.nodeType === 'modbusRead') {
    return { ...currentConfig, unit_id: parsePositiveInteger(draft.modbusUnitId) ?? 1, register: parsePositiveInteger(draft.modbusRegister) ?? 40001, quantity: parsePositiveInteger(draft.modbusQuantity) ?? 1, register_type: draft.modbusRegisterType || 'holding', base_value: parseFiniteNumber(draft.modbusBaseValue) ?? 64, amplitude: parseFiniteNumber(draft.modbusAmplitude) ?? 6 };
  }

  if (draft.nodeType === 'mqttClient') {
    return { ...currentConfig, mode: draft.mqttMode === 'subscribe' ? 'subscribe' : 'publish', topic: draft.mqttTopic.trim(), qos: [0, 1, 2].includes(Number(draft.mqttQos)) ? Number(draft.mqttQos) : 0, payload_template: draft.mqttPayloadTemplate };
  }

  if (draft.nodeType === 'switch') {
    return { ...currentConfig, script: draft.script, branches: draft.branches.map((branch) => ({ key: branch.key, label: branch.label })) };
  }

  if (draft.nodeType === 'httpClient') {
    const { url: _u, method: _m, headers: _h, webhook_kind: _w, content_type: _c, request_timeout_ms: _r, at_mobiles: _a1, at_all: _a2, ...restConfig } = currentConfig;
    return { ...restConfig, body_mode: normalizeHttpBodyMode(draft.httpBodyMode, draft.httpWebhookKind), title_template: draft.httpTitleTemplate, body_template: draft.httpBodyTemplate };
  }

  if (draft.nodeType === 'barkPush') {
    const { server_url: _s, device_key: _d, request_timeout_ms: _r, ...restConfig } = currentConfig;
    return { ...restConfig, content_mode: draft.barkContentMode === 'markdown' ? 'markdown' : 'body', title_template: draft.barkTitleTemplate, subtitle_template: draft.barkSubtitleTemplate, body_template: draft.barkBodyTemplate, level: ['active', 'timeSensitive', 'passive', 'critical'].includes(draft.barkLevel) ? draft.barkLevel : 'active', badge: parseNonNegativeInteger(draft.barkBadge) ?? '', sound: draft.barkSound, icon: draft.barkIcon, group: draft.barkGroup, url: draft.barkUrl, copy: draft.barkCopy, image: draft.barkImage, auto_copy: draft.barkAutoCopy, call: draft.barkCall, archive_mode: ['inherit', 'archive', 'skip'].includes(draft.barkArchiveMode) ? draft.barkArchiveMode : 'inherit' };
  }

  if (draft.nodeType === 'sqlWriter') {
    return { ...currentConfig, database_path: draft.sqlDatabasePath.trim() || './nazh-local.sqlite3', table: draft.sqlTable.trim() || 'workflow_logs' };
  }

  if (draft.nodeType === 'debugConsole') {
    return { ...currentConfig, label: draft.debugLabel.trim(), pretty: draft.debugPretty };
  }

  if (draft.nodeType === 'subgraph') {
    return { ...currentConfig, parameterBindings: draft.parameterBindings };
  }

  if (draft.nodeType === 'lookup') {
    const defaultVal = draft.lookupDefault.trim() === '' ? null
      : (() => { try { return JSON.parse(draft.lookupDefault); } catch { return draft.lookupDefault; } })();
    return { ...currentConfig, table: draft.lookupTable, default: defaultVal };
  }

  if (draft.nodeType === 'humanLoop') {
    const timeoutSec = parsePositiveInteger(draft.hitlApprovalTimeoutSec);
    return {
      ...currentConfig,
      title: draft.hitlTitle.trim() || null,
      description: draft.hitlDescription.trim() || null,
      approval_timeout_ms: timeoutSec != null ? timeoutSec * 1000 : null,
      default_action: draft.hitlDefaultAction || 'autoReject',
      form_schema: (() => {
        const text = draft.hitlFormSchemaJson.trim();
        if (!text) return [];
        try { return JSON.parse(text); } catch { return []; }
      })(),
    };
  }

  if (supportsScriptAi(draft.nodeType)) {
    const { ai: _ai, ...restConfig } = currentConfig;
    return { ...restConfig, script: draft.script };
  }

  if (isScriptNode(draft.nodeType)) {
    return { ...currentConfig, script: draft.script };
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

const NODE_SETTINGS_MAP: Record<string, React.FC<NodeSettingsProps>> = {
  native: NativeNodeSettings,
  code: CodeNodeSettings,
  timer: TimerNodeSettings,
  serialTrigger: SerialTriggerNodeSettings,
  modbusRead: ModbusReadNodeSettings,
  mqttClient: MqttClientNodeSettings,
  if: IfNodeSettings,
  switch: SwitchNodeSettings,
  tryCatch: TryCatchNodeSettings,
  loop: LoopNodeSettings,
  httpClient: HttpClientNodeSettings,
  barkPush: BarkPushNodeSettings,
  sqlWriter: SqlWriterNodeSettings,
  debugConsole: DebugConsoleNodeSettings,
  lookup: LookupNodeSettings,
  subgraph: SubgraphNodeSettings,
  humanLoop: HumanLoopNodeSettings,
};

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
    panelManager.close(FLOWGRAM_NODE_SETTINGS_PANEL_KEY);
  }, [panelManager]);

  useEffect(() => {
    if (!node) { setDraft(null); return; }
    setDraft(readNodeDraft(node));
  }, [node, nodeId]);

  useEffect(() => {
    if (!node) { return () => {}; }
    const dispose = node.onExtInfoChange(() => { setDraft(readNodeDraft(node)); });
    return () => dispose.dispose();
  }, [node]);

  useEffect(() => {
    const dispose = playground.config.onReadonlyOrDisabledChange(() => {
      if (playground.config.readonly) { closePanel(); }
    });
    return () => dispose.dispose();
  }, [closePanel, playground]);

  useEffect(() => {
    if (!node) { return () => {}; }
    const dispose = node.onDispose(() => { closePanel(); });
    return () => dispose.dispose();
  }, [closePanel, node]);

  const stats = useMemo(() => {
    if (!node) { return null; }
    return { incoming: node.lines.inputNodes.length, outgoing: node.lines.outputNodes.length };
  }, [node]);

  const activeCopilotProvider = useMemo(
    () => activeAiProviderId ? aiProviders.find((p) => p.id === activeAiProviderId) ?? null : null,
    [activeAiProviderId, aiProviders],
  );

  const resolvedGlobalAiProvider = useMemo(
    () => activeCopilotProvider ?? aiProviders.find((p) => p.enabled) ?? aiProviders[0] ?? null,
    [activeCopilotProvider, aiProviders],
  );

  const preferredCopilotProvider = useMemo(() => {
    if (isUsableAiProvider(resolvedGlobalAiProvider)) { return resolvedGlobalAiProvider; }
    return aiProviders.find((p) => isUsableAiProvider(p)) ?? null;
  }, [aiProviders, resolvedGlobalAiProvider]);

  const aiGenerateButtonTitle = useMemo(() => {
    if (preferredCopilotProvider) { return `使用 ${preferredCopilotProvider.name} 生成 Rhai 脚本`; }
    if (aiProviders.length === 0) { return '请先在 AI 配置中添加提供商'; }
    if (activeCopilotProvider && !activeCopilotProvider.enabled) { return `全局 AI ${activeCopilotProvider.name} 已被禁用`; }
    if (activeCopilotProvider && !activeCopilotProvider.hasApiKey) { return `请先为全局 AI ${activeCopilotProvider.name} 配置 API Key`; }
    if (aiProviders.some((p) => p.enabled)) { return '请先为可用的 AI 提供商配置 API Key'; }
    return '请先启用一个 AI 提供商';
  }, [activeCopilotProvider, aiProviders, preferredCopilotProvider]);

  const selectedConnection = useMemo(
    () => draft?.connectionId ? connections.find((c) => c.id === draft.connectionId) ?? null : null,
    [connections, draft?.connectionId],
  );

  const compatibleConnections = useMemo(
    () => draft ? connections.filter((c) => connectionMatchesNodeType(draft.nodeType, c)) : [],
    [connections, draft],
  );

  const usesManagedHttpConnection = Boolean(
    draft?.nodeType === 'httpClient' && selectedConnection && connectionMatchesNodeType('httpClient', selectedConnection),
  );
  const usesManagedBarkConnection = Boolean(
    draft?.nodeType === 'barkPush' && selectedConnection && connectionMatchesNodeType('barkPush', selectedConnection),
  );
  const resolvedHttpWebhookKind =
    usesManagedHttpConnection && selectedConnection
      ? readConnectionMetadataString(selectedConnection, 'webhook_kind', inferHttpWebhookKind(readConnectionMetadataString(selectedConnection, 'url')))
      : draft?.httpWebhookKind ?? 'generic';
  const resolvedHttpBodyMode = draft ? normalizeHttpBodyMode(draft.httpBodyMode, resolvedHttpWebhookKind) : 'json';

  const diagnostics = useMemo<NodeValidation[]>(() => {
    if (!draft) { return []; }

    const nextDiagnostics: NodeValidation[] = [];
    const parsedTimeoutMs = parseTimeoutMs(draft.timeoutMs);

    if (stats) {
      if (stats.incoming === 0 && stats.outgoing === 0) {
        nextDiagnostics.push({ tone: 'warning', message: '当前节点是孤立节点。' });
      } else if (stats.incoming === 0) {
        nextDiagnostics.push({ tone: 'info', message: '当前节点是入口节点。' });
      } else if (stats.outgoing === 0) {
        nextDiagnostics.push({ tone: 'info', message: '当前节点位于流程末端。' });
      } else {
        nextDiagnostics.push({ tone: 'info', message: `上游 ${stats.incoming} 条，下游 ${stats.outgoing} 条。` });
      }
    }

    nextDiagnostics.push(...validateConnectionBinding({
      draft,
      selectedConnection,
      compatibleConnections,
      connections,
    }));

    if (draft.timeoutMs.trim() && parsedTimeoutMs === null) {
      nextDiagnostics.push({ tone: 'danger', message: '超时值必须是大于 0 的数字。', field: 'timeoutMs' });
    }

    if (isScriptNode(draft.nodeType) && !draft.script.trim()) {
      nextDiagnostics.push({ tone: 'danger', message: '脚本为空。', field: 'script' });
    }

    const nodeDef = getNodeDefinition(draft.nodeType as NazhNodeKind);
    if (nodeDef) {
      if (nodeDef.fieldValidators) {
        for (const [field, validator] of Object.entries(nodeDef.fieldValidators)) {
          if (!validator) { continue; }
          const value = (draft as unknown as Record<string, unknown>)[field];
          if (typeof value !== 'string') { continue; }
          const result: FieldValidatorResult = validator(value);
          if (result === null) { continue; }
          if (typeof result === 'string') {
            nextDiagnostics.push({ tone: 'danger', message: result, field });
          } else {
            nextDiagnostics.push({ tone: result.tone, message: result.message, field });
          }
        }
      }

      const validationCtx: NodeValidationContext = {
        draft,
        selectedConnection,
        compatibleConnections,
        connections,
        resolvedHttpWebhookKind,
        resolvedHttpBodyMode,
        aiProviders,
        activeAiProviderId,
        resolvedGlobalAiProvider,
        preferredCopilotProvider,
        usesManagedConnection: draft.nodeType === 'httpClient' ? usesManagedHttpConnection : draft.nodeType === 'barkPush' ? usesManagedBarkConnection : false,
      };
      nextDiagnostics.push(...nodeDef.validate(validationCtx));
    }

    return nextDiagnostics;
  }, [activeAiProviderId, activeCopilotProvider, aiProviders, compatibleConnections.length, connections, draft, resolvedGlobalAiProvider, resolvedHttpBodyMode, selectedConnection, stats, usesManagedBarkConnection, usesManagedHttpConnection]);

  const branchSummary = useMemo(
    () => draft ? getLogicNodeBranchDefinitions(draft.nodeType, { branches: draft.branches }) : [],
    [draft],
  );

  const updateDraft = useCallback(
    (patch: Partial<SelectedNodeDraft>) => {
      if (!node) { return; }

      setDraft((currentDraft) => {
        const baseDraft = currentDraft ?? readNodeDraft(node);
        const nextDraft = { ...baseDraft, ...patch };
        const currentExtInfo = (node.getExtInfo() ?? {}) as { config?: unknown };
        const currentConfig = isRecord(currentExtInfo.config) ? (currentExtInfo.config as NodeConfigMap) : {};

        const nextExtInfo = {
          ...currentExtInfo,
          label: resolveNodeDisplayLabel(nextDraft.nodeType, nextDraft.label),
          nodeType: nextDraft.nodeType,
          connectionId: nextDraft.connectionId.trim() || null,
          timeoutMs: parseTimeoutMs(nextDraft.timeoutMs),
          config: buildNodeConfig(nextDraft, currentConfig),
        };

        node.updateExtInfo(nextExtInfo);

        if (usesDynamicPorts(nextDraft.nodeType)) {
          window.requestAnimationFrame(() => { node.ports.updateDynamicPorts(); });
        }

        return readNodeDraft(node);
      });
    },
    [node],
  );

  const handleAiGenerate = useCallback(
    async () => {
      if (!node || !draft || !preferredCopilotProvider) { return; }

      const requirement = aiDialogRequirement.trim();
      if (!requirement) { setAiGenerateError('请输入生成需求。'); return; }

      setAiGenerating(true);
      setAiGenerateError(null);
      setAiStreamPreview('');
      setAiThinkingPreview('');
      try {
        const context = getNodeContext(node);
        const script = await generateScriptStream(
          requirement,
          context,
          { providerId: preferredCopilotProvider.id, model: preferredCopilotProvider.defaultModel, params: copilotParams },
          (rawText) => setAiStreamPreview(rawText),
          (thinkingText) => setAiThinkingPreview(thinkingText),
        );
        if (!script) { setAiGenerateError('AI 未返回有效代码。'); return; }
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

  const NodeSettingsComponent = NODE_SETTINGS_MAP[draft.nodeType];
  const settingsProps: NodeSettingsProps = {
    draft,
    updateDraft,
    connections,
    selectedConnection,
    compatibleConnections,
    resolvedHttpWebhookKind,
    resolvedHttpBodyMode,
    aiProviders,
    activeAiProviderId,
    resolvedGlobalAiProvider,
    preferredCopilotProvider,
    aiGenerateButtonTitle,
    aiGenerating,
    onOpenAiDialog: () => {
      setAiGenerateError(null);
      setAiDialogOpen(true);
    },
  };

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
        {supportsConnectionBinding(draft.nodeType) ? (
          <label>
            <span>连接资源</span>
            <select
              value={
                draft.connectionId && !connections.some((c) => c.id === draft.connectionId)
                  ? `__missing__:${draft.connectionId}`
                  : draft.connectionId || '__none__'
              }
              onChange={(event) => {
                const value = event.target.value;
                if (value === '__none__') { updateDraft({ connectionId: '' }); return; }
                if (value.startsWith('__missing__:')) { updateDraft({ connectionId: value.replace('__missing__:', '') }); return; }
                updateDraft({ connectionId: value });
              }}
              disabled={compatibleConnections.length === 0 && !draft.connectionId}
            >
              <option value="__none__">未绑定</option>
              {draft.connectionId && !connections.some((c) => c.id === draft.connectionId) ? (
                <option value={`__missing__:${draft.connectionId}`}>未注册连接: {draft.connectionId}</option>
              ) : null}
              {selectedConnection && draft.connectionId && !connectionMatchesNodeType(draft.nodeType, selectedConnection) ? (
                <option value={selectedConnection.id}>不兼容连接: {selectedConnection.id} · {selectedConnection.type}</option>
              ) : null}
              {compatibleConnections.map((c) => (
                <option key={c.id} value={c.id}>{c.id} · {c.type}</option>
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

        {draft.nodeType !== 'switch' && NodeSettingsComponent ? <NodeSettingsComponent {...settingsProps} /> : null}
      </div>

      {draft.nodeType === 'switch' ? <SwitchNodeSettings {...settingsProps} /> : null}

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
                onClick={() => { void handleAiGenerate(); }}
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
  defaultSize: 360,
  render: (props) => <FlowgramNodeSettingsPanel key={props.nodeId} {...props} />,
};
