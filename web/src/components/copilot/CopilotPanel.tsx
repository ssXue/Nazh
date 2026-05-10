import { useCallback, useEffect, useRef, useState, type RefObject } from 'react';

import { invoke } from '@tauri-apps/api/core';

import type { FlowgramCanvasHandle, CanvasOps } from '../FlowgramCanvas';
import type { CopilotConversationResponse } from '../../generated/CopilotConversationResponse';
import type { CopilotMessageResponse } from '../../generated/CopilotMessageResponse';
import { copilotChatStream } from '../../lib/copilot-stream';
import type { ToolCallInfo, ToolResultInfo } from '../../lib/copilot-stream';
import {
  consumeOperationLines,
  createInitialProtocolSession,
  flushRemainingProtocolLines,
  protocolSessionApplyOp,
  type ProtocolOperation,
  type CopilotProtocolSession,
} from '../../lib/copilot-protocol';
import { hasTauriRuntime } from '../../lib/tauri';
import { CopilotChatView } from './CopilotChatView';

/// 调试日志开关——开发期间保持 true，上线后可关闭。
const DEBUG_PANEL = true;

function panelLog(...args: unknown[]) {
  if (DEBUG_PANEL) console.log('[copilot-panel]', ...args);
}

function panelWarn(...args: unknown[]) {
  if (DEBUG_PANEL) console.warn('[copilot-panel]', ...args);
}

export type CopilotSessionStatus = 'idle' | 'generating';

export interface LocalMessage {
  id: string;
  role: 'user' | 'assistant';
  content: string;
  streaming?: boolean;
  toolCalls?: ToolCallInfo[];
  toolResults?: ToolResultInfo[];
  protocolOps?: ProtocolOperation[];
  protocolDoneSummary?: string;
}

function localConversationId(): string {
  return `local-${Date.now()}-${Math.random().toString(16).slice(2)}`;
}

interface CopilotPanelProps {
  canvasRef: RefObject<FlowgramCanvasHandle | null>;
  onEnsureBoardOpen: () => void;
}

/// 节流更新间隔（ms）。
const FLUSH_INTERVAL = 150;

interface PendingUpdate {
  content: string;
  protocolOps?: ProtocolOperation[];
  protocolDoneSummary?: string;
  toolCalls?: ToolCallInfo[];
  toolResults?: ToolResultInfo[];
}

export function CopilotPanel({ canvasRef, onEnsureBoardOpen }: CopilotPanelProps) {
  const [collapsed, setCollapsed] = useState(false);
  const [conversations, setConversations] = useState<CopilotConversationResponse[]>([]);
  const [activeId, setActiveId] = useState<string | null>(null);
  const [messages, setMessages] = useState<LocalMessage[]>([]);
  const [status, setStatus] = useState<CopilotSessionStatus>('idle');
  const isTauri = hasTauriRuntime();

  // 协议解析状态（per-send）
  const protocolSessionRef = useRef<CopilotProtocolSession>(createInitialProtocolSession());
  const protocolProcessedRef = useRef(0);

  // AbortController
  const abortRef = useRef<AbortController | null>(null);

  // 节流更新
  const pendingUpdateRef = useRef<PendingUpdate | null>(null);
  const flushTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const activeMsgIdRef = useRef<string | null>(null);

  const flushPendingUpdate = useCallback(() => {
    if (flushTimerRef.current !== null) {
      clearTimeout(flushTimerRef.current);
      flushTimerRef.current = null;
    }
    const update = pendingUpdateRef.current;
    const msgId = activeMsgIdRef.current;
    if (!update || !msgId) {
      pendingUpdateRef.current = null;
      return;
    }
    pendingUpdateRef.current = null;
    panelLog('flushPendingUpdate', {
      msgId,
      contentLen: update.content.length,
      hasProtocolOps: !!update.protocolOps,
      opsCount: update.protocolOps?.length ?? 0,
      toolCalls: update.toolCalls?.length ?? 0,
      toolResults: update.toolResults?.length ?? 0,
    });
    setMessages((prev) =>
      prev.map((m) => (m.id === msgId ? {
        ...m,
        content: update.content,
        protocolOps: update.protocolOps,
        protocolDoneSummary: update.protocolDoneSummary,
        toolCalls: update.toolCalls ?? m.toolCalls,
        toolResults: update.toolResults ?? m.toolResults,
      } : m)),
    );
  }, []);

  const scheduleFlush = useCallback(() => {
    if (flushTimerRef.current !== null) return;
    flushTimerRef.current = setTimeout(() => {
      flushPendingUpdate();
    }, FLUSH_INTERVAL);
  }, [flushPendingUpdate]);

  const refreshConversations = useCallback(async () => {
    if (!isTauri) return;
    try {
      const list = await invoke<CopilotConversationResponse[]>('copilot_list_conversations');
      setConversations(list);
    } catch { /* ignore */ }
  }, [isTauri]);

  useEffect(() => {
    void refreshConversations();
  }, [refreshConversations]);

  useEffect(() => {
    return () => {
      if (flushTimerRef.current !== null) {
        clearTimeout(flushTimerRef.current);
      }
    };
  }, []);

  const loadMessages = useCallback(async (convId: string) => {
    if (!isTauri) return;
    try {
      const loaded = await invoke<CopilotMessageResponse[]>('copilot_load_conversation', { id: convId });
      setMessages(loaded.map((m) => ({ id: m.id, role: m.role as 'user' | 'assistant', content: m.content })));
    } catch { /* ignore */ }
  }, [isTauri]);

  const handleSelectConversation = useCallback((convId: string) => {
    setActiveId(convId);
    void loadMessages(convId);
  }, [loadMessages]);

  const handleNewConversation = useCallback(async () => {
    if (isTauri) {
      try {
        const conv = await invoke<CopilotConversationResponse>('copilot_create_conversation');
        setConversations((prev) => [conv, ...prev]);
        setActiveId(conv.id);
        setMessages([]);
        return;
      } catch { /* fall through to local */ }
    }
    const now = new Date().toISOString();
    const local: CopilotConversationResponse = {
      id: localConversationId(),
      title: '新对话',
      createdAt: now,
      updatedAt: now,
    };
    setConversations((prev) => [local, ...prev]);
    setActiveId(local.id);
    setMessages([]);
  }, [isTauri]);

  const handleDeleteConversation = useCallback(async (convId: string) => {
    if (isTauri) {
      try {
        await invoke('copilot_delete_conversation', { id: convId });
      } catch { /* ignore */ }
    }
    setConversations((prev) => prev.filter((c) => c.id !== convId));
    if (activeId === convId) {
      setActiveId(null);
      setMessages([]);
    }
  }, [isTauri, activeId]);

  const handleCancel = useCallback(() => {
    panelLog('handleCancel', { hasAbort: !!abortRef.current, protocolOps: protocolSessionRef.current.operations.length });
    abortRef.current?.abort();
    abortRef.current = null;
    flushPendingUpdate();
    const msgId = activeMsgIdRef.current;
    if (msgId) {
      setMessages((prev) =>
        prev.map((m) => (m.id === msgId ? { ...m, streaming: false } : m)),
      );
    }
    setStatus('idle');
  }, [flushPendingUpdate]);

  /// 标记本次 send 是否已经调用过 onEnsureBoardOpen（防止多次创建工程）。
  const boardEnsuredRef = useRef(false);

  const handleSend = useCallback(async (text: string) => {
    if (!activeId || !text.trim() || status !== 'idle') return;

    panelLog('handleSend 开始', { activeId, text: text.slice(0, 80), status });

    const userMsg: LocalMessage = { id: `local-${Date.now()}`, role: 'user', content: text.trim() };
    const assistantMsgId = `stream-${Date.now()}`;
    const assistantMsg: LocalMessage = { id: assistantMsgId, role: 'assistant', content: '', streaming: true, toolCalls: [], toolResults: [] };

    setMessages((prev) => [...prev, userMsg, assistantMsg]);
    setStatus('generating');
    activeMsgIdRef.current = assistantMsgId;

    // 重置协议解析状态
    protocolSessionRef.current = createInitialProtocolSession();
    protocolProcessedRef.current = 0;
    boardEnsuredRef.current = false;

    // 创建 AbortController
    const abortController = new AbortController();
    abortRef.current = abortController;

    if (!isTauri) {
      setMessages((prev) =>
        prev.map((m) =>
          m.id === assistantMsgId ? { ...m, content: '预览模式：AI 不可用，请在 Tauri 桌面端使用', streaming: false } : m,
        ),
      );
      setStatus('idle');
      return;
    }

    try {
      const result = await copilotChatStream(
        activeId,
        text.trim(),
        (accumulated) => {
          // 增量解析 JSON Lines 操作
          const { nextProcessedLength, operations } = consumeOperationLines(
            accumulated,
            protocolProcessedRef.current,
          );
          protocolProcessedRef.current = nextProcessedLength;

          if (operations.length === 0) return;

          let session = protocolSessionRef.current;
          let doneSummary: string | undefined;
          const pendingNodes: CanvasOps['nodes'] = [];
          const pendingEdges: CanvasOps['edges'] = [];

          for (const op of operations) {
            session = protocolSessionApplyOp(session, op);
            protocolSessionRef.current = session;

            if (op.type === 'create_node') {
              const nodeId = session.nodeRefs[op.ref];
              if (nodeId) {
                pendingNodes.push({
                  id: nodeId,
                  type: op.nodeType,
                  label: op.label,
                  config: op.config as Record<string, unknown> | undefined,
                  connection_id: op.connectionId ?? undefined,
                });
              } else {
                panelWarn('create_node: nodeId 未分配，跳过', { ref: op.ref });
              }
            } else if (op.type === 'create_edge') {
              const fromId = session.nodeRefs[op.fromRef];
              const toId = session.nodeRefs[op.toRef];
              if (fromId && toId) {
                pendingEdges.push({ from: fromId, to: toId, source_port_id: op.sourcePortId, target_port_id: op.targetPortId });
              } else {
                panelWarn('create_edge: ref 未解析，跳过', {
                  fromRef: op.fromRef,
                  toRef: op.toRef,
                  fromId,
                  toId,
                  nodeRefs: session.nodeRefs,
                });
              }
            } else if (op.type === 'done') {
              doneSummary = op.summary;
            }
          }

          // 只在有画布操作时调用 onEnsureBoardOpen，且仅调用一次
          if ((pendingNodes.length > 0 || pendingEdges.length > 0) && !boardEnsuredRef.current) {
            panelLog('首次调用 onEnsureBoardOpen');
            onEnsureBoardOpen();
            boardEnsuredRef.current = true;
          }

          // 一次性提交所有画布操作
          if (pendingNodes.length > 0 || pendingEdges.length > 0) {
            panelLog('批量提交画布操作', { nodes: pendingNodes.length, edges: pendingEdges.length });
            canvasRef.current?.addCanvasOps({ nodes: pendingNodes, edges: pendingEdges });
          }

          // 写入节流队列而非直接 setMessages
          let displayContent = accumulated;
          const allOps = [...(session.operations)];
          if (allOps.length > 0) {
            displayContent = doneSummary ?? '';
          }

          pendingUpdateRef.current = {
            content: displayContent,
            protocolOps: allOps.length > 0 ? allOps : undefined,
            protocolDoneSummary: doneSummary,
          };
          scheduleFlush();
        },
        undefined,
        (toolCallInfo) => {
          // toolCalls 追加到节流队列
          panelLog('toolCalls 回调', { round: toolCallInfo.round, count: toolCallInfo.calls.length, names: toolCallInfo.calls.map((c) => c.name) });
          const current = pendingUpdateRef.current;
          pendingUpdateRef.current = {
            ...current,
            content: current?.content ?? '',
            toolCalls: [...(current?.toolCalls ?? []), toolCallInfo],
          };
          scheduleFlush();
        },
        (toolResultInfo) => {
          panelLog('toolResult 回调', { name: toolResultInfo.name, isError: toolResultInfo.isError });
          const current = pendingUpdateRef.current;
          pendingUpdateRef.current = {
            ...current,
            content: current?.content ?? '',
            toolResults: [...(current?.toolResults ?? []), toolResultInfo],
          };
          scheduleFlush();
        },
        abortController.signal,
      );

      // 流结束，强制 flush 剩余未解析的协议行（最后一行可能无尾部 \n）
      {
        const remaining = flushRemainingProtocolLines(result.text, protocolProcessedRef.current);
        protocolProcessedRef.current = remaining.nextProcessedLength;

        if (remaining.operations.length > 0) {
          let session = protocolSessionRef.current;
          let doneSummary: string | undefined;
          const flushNodes: CanvasOps['nodes'] = [];
          const flushEdges: CanvasOps['edges'] = [];

          for (const op of remaining.operations) {
            session = protocolSessionApplyOp(session, op);
            protocolSessionRef.current = session;

            if (op.type === 'create_node') {
              const nodeId = session.nodeRefs[op.ref];
              if (nodeId) {
                flushNodes.push({
                  id: nodeId,
                  type: op.nodeType,
                  label: op.label,
                  config: op.config as Record<string, unknown> | undefined,
                  connection_id: op.connectionId ?? undefined,
                });
              }
            } else if (op.type === 'create_edge') {
              const fromId = session.nodeRefs[op.fromRef];
              const toId = session.nodeRefs[op.toRef];
              if (fromId && toId) {
                flushEdges.push({ from: fromId, to: toId, source_port_id: op.sourcePortId, target_port_id: op.targetPortId });
              }
            } else if (op.type === 'done') {
              doneSummary = op.summary;
            }
          }

          if ((flushNodes.length > 0 || flushEdges.length > 0) && !boardEnsuredRef.current) {
            panelLog('flush 首次调用 onEnsureBoardOpen');
            onEnsureBoardOpen();
            boardEnsuredRef.current = true;
          }

          if (flushNodes.length > 0 || flushEdges.length > 0) {
            panelLog('flush 批量提交画布操作', { nodes: flushNodes.length, edges: flushEdges.length });
            canvasRef.current?.addCanvasOps({ nodes: flushNodes, edges: flushEdges });
          }

          const allOps = [...session.operations];
          const currentPending = pendingUpdateRef.current;
          const existingSummary = currentPending?.protocolDoneSummary;
          pendingUpdateRef.current = {
            content: currentPending?.content ?? '',
            protocolOps: allOps.length > 0 ? allOps : undefined,
            protocolDoneSummary: doneSummary ?? existingSummary,
          };
        }
      }

      // 立即 flush 最后的状态
      flushPendingUpdate();
      setMessages((prev) =>
        prev.map((m) => (m.id === assistantMsgId ? { ...m, streaming: false } : m)),
      );

      panelLog('流正常结束', {
        textLen: result.text.length,
        aborted: result.aborted,
        finishReason: result.finishReason,
        protocolOpsCount: protocolSessionRef.current.operations.length,
        nodeRefs: protocolSessionRef.current.nodeRefs,
        preview: result.text.slice(0, 200),
      });

      if (result.aborted) {
        // 用户取消，不标记为错误
      }
    } catch (error) {
      flushPendingUpdate();
      panelWarn('流异常结束', { error: error instanceof Error ? error.message : String(error) });
      const errorMessage = error instanceof Error ? error.message : String(error);
      setMessages((prev) =>
        prev.map((m) =>
          m.id === assistantMsgId ? { ...m, content: `错误: ${errorMessage}`, streaming: false } : m,
        ),
      );
    } finally {
      setStatus('idle');
      abortRef.current = null;
      void refreshConversations();
    }
  }, [activeId, status, isTauri, refreshConversations, canvasRef, onEnsureBoardOpen, flushPendingUpdate, scheduleFlush]);

  if (collapsed) {
    return (
      <button
        type="button"
        className="copilot-panel copilot-panel--collapsed"
        title="展开副驾驶"
        onClick={() => setCollapsed(false)}
      >
        <span className="copilot-panel__collapsed-icon">AI</span>
      </button>
    );
  }

  return (
    <section className="copilot-panel">
      <div className="copilot-tabs">
        <div className="copilot-tabs__items">
          {conversations.map((conv) => (
            <button
              key={conv.id}
              type="button"
              className={`copilot-tabs__tab${conv.id === activeId ? ' is-active' : ''}`}
              onClick={() => handleSelectConversation(conv.id)}
            >
              <span className="copilot-tabs__tab-title">{conv.title}</span>
              <span
                className="copilot-tabs__tab-close"
                role="button"
                tabIndex={0}
                onClick={(e) => { e.stopPropagation(); handleDeleteConversation(conv.id); }}
                onKeyDown={(e) => { if (e.key === 'Enter') { e.stopPropagation(); handleDeleteConversation(conv.id); } }}
              >
                &times;
              </span>
            </button>
          ))}
        </div>
        <div className="copilot-tabs__actions">
          <button type="button" className="copilot-btn-icon" title="新建对话" onClick={handleNewConversation}>+</button>
          <button type="button" className="copilot-btn-icon" title="收起面板" onClick={() => setCollapsed(true)}>&laquo;</button>
        </div>
      </div>
      <div className="copilot-panel__main">
        <CopilotChatView
          messages={messages}
          status={status}
          hasConversation={Boolean(activeId)}
          onSend={handleSend}
          onNewConversation={handleNewConversation}
          onCancel={handleCancel}
        />
      </div>
    </section>
  );
}
