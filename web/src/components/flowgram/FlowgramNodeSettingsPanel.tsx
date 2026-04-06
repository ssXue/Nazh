import { useCallback, useEffect, useMemo, useState } from 'react';

import { type FlowNodeEntity, useClientContext } from '@flowgram.ai/free-layout-editor';
import { type PanelFactory, usePanelManager } from '@flowgram.ai/panel-manager-plugin';

import { parseTimeoutMs } from './flowgram-node-library';
import type { ConnectionDefinition } from '../../types';

export interface FlowgramNodeSettingsPanelProps {
  nodeId: string;
  connections: ConnectionDefinition[];
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
}

interface NodeValidation {
  tone: 'info' | 'warning' | 'danger';
  message: string;
}

export const FLOWGRAM_NODE_SETTINGS_PANEL_KEY = 'nazh-flowgram-node-settings';

function readNodeDraft(node: FlowNodeEntity): SelectedNodeDraft {
  const rawData = (node.getExtInfo() ?? {}) as {
    label?: string;
    nodeType?: string;
    connectionId?: string | null;
    aiDescription?: string | null;
    timeoutMs?: number | null;
    config?: {
      message?: string;
      script?: string;
    };
  };

  return {
    id: node.id,
    nodeType: String(rawData.nodeType ?? node.flowNodeType),
    label: rawData.label ?? node.id,
    connectionId: rawData.connectionId ?? '',
    aiDescription: rawData.aiDescription ?? '',
    timeoutMs: rawData.timeoutMs ? String(rawData.timeoutMs) : '',
    message: rawData.config?.message ?? '',
    script: rawData.config?.script ?? '',
  };
}

function FlowgramNodeSettingsPanel({
  nodeId,
  connections,
}: FlowgramNodeSettingsPanelProps) {
  const panelManager = usePanelManager();
  const { document, playground } = useClientContext();
  const node = document.getNode(nodeId) as FlowNodeEntity | undefined;
  const [draft, setDraft] = useState<SelectedNodeDraft | null>(() => (node ? readNodeDraft(node) : null));

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

  const diagnostics = useMemo<NodeValidation[]>(() => {
    if (!draft) {
      return [];
    }

    const nextDiagnostics: NodeValidation[] = [];
    const selectedConnection = connections.find((connection) => connection.id === draft.connectionId);
    const trimmedMessage = draft.message.trim();
    const trimmedScript = draft.script.trim();
    const parsedTimeoutMs = parseTimeoutMs(draft.timeoutMs);

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

    if (draft.connectionId && !selectedConnection) {
      nextDiagnostics.push({
        tone: 'danger',
        message: `连接 ${draft.connectionId} 未注册。`,
      });
    } else if (selectedConnection) {
      nextDiagnostics.push({
        tone: 'info',
        message: `已绑定 ${selectedConnection.id} · ${selectedConnection.type}`,
      });
    } else if (draft.nodeType === 'native' && connections.length > 0) {
      nextDiagnostics.push({
        tone: 'warning',
        message: '原生节点未绑定连接。',
      });
    }

    if (draft.timeoutMs.trim() && parsedTimeoutMs === null) {
      nextDiagnostics.push({
        tone: 'danger',
        message: '超时值必须是大于 0 的数字。',
      });
    }

    if (draft.nodeType === 'native' && !trimmedMessage) {
      nextDiagnostics.push({
        tone: 'warning',
        message: 'Native Message 为空。',
      });
    }

    if (draft.nodeType === 'rhai' && !trimmedScript) {
      nextDiagnostics.push({
        tone: 'danger',
        message: 'Rhai Script 为空。',
      });
    }

    return nextDiagnostics;
  }, [connections, draft, stats]);

  const updateDraft = useCallback(
    (patch: Partial<SelectedNodeDraft>) => {
      if (!node || !draft) {
        return;
      }

      const nextDraft = {
        ...draft,
        ...patch,
      };

      const nextExtInfo = {
        ...(node.getExtInfo() ?? {}),
        label: nextDraft.label || nextDraft.id,
        nodeType: nextDraft.nodeType,
        connectionId: nextDraft.connectionId.trim() || null,
        aiDescription: nextDraft.aiDescription.trim() || null,
        timeoutMs: parseTimeoutMs(nextDraft.timeoutMs),
        config:
          nextDraft.nodeType === 'native'
            ? {
                ...(((node.getExtInfo() ?? {}) as { config?: Record<string, unknown> }).config ?? {}),
                message: nextDraft.message,
              }
            : {
                ...(((node.getExtInfo() ?? {}) as { config?: Record<string, unknown> }).config ?? {}),
                script: nextDraft.script,
              },
      };

      node.updateExtInfo(nextExtInfo);
      setDraft(nextDraft);
    },
    [draft, node],
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
            <option value="__none__">纯逻辑节点</option>
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
            <span>Native Message</span>
            <textarea value={draft.message} onChange={(event) => updateDraft({ message: event.target.value })} />
          </label>
        ) : (
          <label>
            <span>Rhai Script</span>
            <textarea value={draft.script} onChange={(event) => updateDraft({ script: event.target.value })} />
          </label>
        )}
      </div>

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
    </section>
  );
}

export const flowgramNodeSettingsPanelFactory: PanelFactory<FlowgramNodeSettingsPanelProps> = {
  key: FLOWGRAM_NODE_SETTINGS_PANEL_KEY,
  render: (props) => <FlowgramNodeSettingsPanel {...props} />,
};
