import { useCallback, useEffect, useMemo, useState } from 'react';

import { type FlowNodeEntity, useClientContext } from '@flowgram.ai/free-layout-editor';
import { type PanelFactory, usePanelManager } from '@flowgram.ai/panel-manager-plugin';

import {
  getLogicNodeBranchDefinitions,
  inferHttpWebhookKind,
  normalizeHttpBodyMode,
  parseTimeoutMs,
  resolveNodeDisplayLabel,
  getFallbackNodeLabel,
  type NazhNodeKind,
  getNodeDefinition,
} from './flowgram-node-library';
import type { NodeValidationContext } from './nodes/shared';

import {
  type SelectedNodeDraft,
  type NodeValidation,
  type NodeSettingsProps,
  type FieldValidatorResult,
  isRecord,
  readConnectionMetadataString,
  supportsConnectionBinding,
  connectionMatchesNodeType,
  isScriptNode,
  usesDynamicPorts,
  validateConnectionBinding,
} from './nodes/settings-shared';

import {
  type FlowgramNodeSettingsPanelProps,
  type NodeConfigMap,
  FLOWGRAM_NODE_SETTINGS_PANEL_KEY,
} from './node-settings-types';
import { readNodeDraft, buildNodeConfig } from './node-settings-helpers';

import { NativeNodeSettings } from './nodes/native/settings';
import { CodeNodeSettings } from './nodes/code/settings';
import { TimerNodeSettings } from './nodes/timer/settings';
import { SerialTriggerNodeSettings } from './nodes/serialTrigger/settings';
import { ModbusReadNodeSettings } from './nodes/modbusRead/settings';
import { CanReadNodeSettings } from './nodes/canRead/settings';
import { CanWriteNodeSettings } from './nodes/canWrite/settings';
import { EthercatPdoNodeSettings } from './nodes/ethercatPdoRead/settings';
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
import { CapabilityCallNodeSettings } from './nodes/capabilityCall/settings';

export { type FlowgramNodeSettingsPanelProps, FLOWGRAM_NODE_SETTINGS_PANEL_KEY } from './node-settings-types';

const NODE_SETTINGS_MAP: Record<string, React.FC<NodeSettingsProps>> = {
  native: NativeNodeSettings,
  code: CodeNodeSettings,
  timer: TimerNodeSettings,
  serialTrigger: SerialTriggerNodeSettings,
  modbusRead: ModbusReadNodeSettings,
  canRead: CanReadNodeSettings,
  canWrite: CanWriteNodeSettings,
  ethercatPdoRead: EthercatPdoNodeSettings,
  ethercatPdoWrite: EthercatPdoNodeSettings,
  mqttClient: MqttClientNodeSettings,
  if: IfNodeSettings,
  switch: SwitchNodeSettings,
  tryCatch: TryCatchNodeSettings,
  loop: LoopNodeSettings,
  httpClient: HttpClientNodeSettings,
  barkPush: BarkPushNodeSettings,
  sqlWriter: SqlWriterNodeSettings,
  debugConsole: DebugConsoleNodeSettings,
  capabilityCall: CapabilityCallNodeSettings,
  lookup: LookupNodeSettings,
  subgraph: SubgraphNodeSettings,
  humanLoop: HumanLoopNodeSettings,
};

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
  };

  return (
    <section className="flowgram-floating-panel flowgram-floating-panel--node">
      <div className="flowgram-floating-panel__header">
        <div className="flowgram-floating-panel__header-left">
          <h3>{resolveNodeDisplayLabel(draft.nodeType, draft.label)}</h3>
          <span className={`flowgram-node-badge flowgram-node-badge--${draft.nodeType}`}>
            {getFallbackNodeLabel(draft.nodeType as NazhNodeKind)}
          </span>
        </div>
        <div className="flowgram-floating-panel__header-right">
          {stats ? (
            <span className="flowgram-header-stats">
              <span>↑{stats.incoming}</span>
              <span>↓{stats.outgoing}</span>
            </span>
          ) : null}
          <span
            className="flowgram-header-node-id"
            title="点击复制节点 ID"
            onClick={() => { void navigator.clipboard.writeText(draft.id); }}
          >
            ID: {draft.id}
          </span>
        </div>
      </div>

      <div className="flowgram-panel-scroll">
      <div className="flowgram-form">
        <label>
          <span>显示名称</span>
          <input value={draft.label} onChange={(event) => updateDraft({ label: event.target.value })} />
        </label>

        <hr className="flowgram-form__divider" />

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

        <details className="flowgram-advanced-section" open={Boolean(draft.timeoutMs.trim())}>
          <summary className="flowgram-advanced-section__toggle">高级设置</summary>
          <div className="flowgram-advanced-section__body">
            <label>
              <span>超时 ms</span>
              <input
                value={draft.timeoutMs}
                onChange={(event) => updateDraft({ timeoutMs: event.target.value })}
                placeholder="留空表示不限"
              />
            </label>
          </div>
        </details>

        <hr className="flowgram-form__divider" />

        {draft.nodeType !== 'switch' && NodeSettingsComponent ? <NodeSettingsComponent {...settingsProps} /> : null}
      </div>

      {draft.nodeType === 'switch' ? <SwitchNodeSettings {...settingsProps} /> : null}

      {branchSummary.length > 0 ? (
        <section className="flowgram-panel--branches">
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
      </div>
    </section>
  );
}

export const flowgramNodeSettingsPanelFactory: PanelFactory<FlowgramNodeSettingsPanelProps> = {
  key: FLOWGRAM_NODE_SETTINGS_PANEL_KEY,
  defaultSize: 360,
  render: (props) => <FlowgramNodeSettingsPanel key={props.nodeId} {...props} />,
};
