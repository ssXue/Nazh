import { useCallback, useEffect, useRef, useState, type RefObject } from 'react';

import { invoke } from '@tauri-apps/api/core';

import type { FlowgramCanvasHandle, CanvasOps } from '../FlowgramCanvas';
import type { CopilotConversationResponse } from '../../generated/CopilotConversationResponse';
import type { CopilotMessageResponse } from '../../generated/CopilotMessageResponse';
import { copilotChatStream } from '../../lib/copilot-stream';
import type { ToolCallInfo, ToolResultInfo, CanvasOpEvent } from '../../lib/copilot-stream';
import { hasTauriRuntime, loadAiConfig } from '../../lib/tauri';
import { resolveGlobalAiProvider } from '../../lib/workflow-ai';
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
  canvasOps?: CanvasOpEvent[];
}

function localConversationId(): string {
  return `local-${Date.now()}-${Math.random().toString(16).slice(2)}`;
}

interface CopilotPanelProps {
  canvasRef: RefObject<FlowgramCanvasHandle | null>;
  onEnsureBoardOpen: (name?: string) => void;
  workspacePath?: string;
}

/// 节流更新间隔（ms）。
const FLUSH_INTERVAL = 150;

interface PendingUpdate {
  content: string;
  toolCalls?: ToolCallInfo[];
  toolResults?: ToolResultInfo[];
  canvasOps?: CanvasOpEvent[];
}

const COPILOT_MIN_WIDTH = 320;
const COPILOT_MAX_WIDTH = 720;
const COPILOT_DEFAULT_WIDTH = 440;

export function CopilotPanel({ canvasRef, onEnsureBoardOpen, workspacePath }: CopilotPanelProps) {
  const [collapsed, setCollapsed] = useState(false);
  const [panelWidth, setPanelWidth] = useState(COPILOT_DEFAULT_WIDTH);
  const [conversations, setConversations] = useState<CopilotConversationResponse[]>([]);
  const [activeId, setActiveId] = useState<string | null>(null);
  const [messages, setMessages] = useState<LocalMessage[]>([]);
  const [status, setStatus] = useState<CopilotSessionStatus>('idle');
  const [historyOpen, setHistoryOpen] = useState(false);
  const isTauri = hasTauriRuntime();
  const panelRef = useRef<HTMLElement | null>(null);
  const draggingRef = useRef(false);

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
      canvasOpsCount: update.canvasOps?.length ?? 0,
      toolCalls: update.toolCalls?.length ?? 0,
      toolResults: update.toolResults?.length ?? 0,
    });
    setMessages((prev) =>
      prev.map((m) => (m.id === msgId ? {
        ...m,
        content: update.content,
        canvasOps: update.canvasOps ?? m.canvasOps,
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

  const handleResizeStart = useCallback((e: React.MouseEvent) => {
    e.preventDefault();
    draggingRef.current = true;
    const startX = e.clientX;
    const startWidth = panelWidth;

    const onMouseMove = (ev: MouseEvent) => {
      if (!draggingRef.current) return;
      const delta = startX - ev.clientX;
      const next = Math.min(COPILOT_MAX_WIDTH, Math.max(COPILOT_MIN_WIDTH, startWidth + delta));
      setPanelWidth(next);
    };

    const onMouseUp = () => {
      draggingRef.current = false;
      document.body.style.cursor = '';
      document.body.style.userSelect = '';
      window.removeEventListener('mousemove', onMouseMove);
      window.removeEventListener('mouseup', onMouseUp);
    };

    document.body.style.cursor = 'col-resize';
    document.body.style.userSelect = 'none';
    window.addEventListener('mousemove', onMouseMove);
    window.addEventListener('mouseup', onMouseUp);
  }, [panelWidth]);

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
    setHistoryOpen(false);
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
    panelLog('handleCancel', { hasAbort: !!abortRef.current });
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
    if (!text.trim() || status !== 'idle') return;

    // 没有活跃对话时自动创建
    let convId = activeId;
    if (!convId) {
      if (isTauri) {
        try {
          const conv = await invoke<CopilotConversationResponse>('copilot_create_conversation');
          setConversations((prev) => [conv, ...prev]);
          setActiveId(conv.id);
          convId = conv.id;
        } catch { /* fall through */ }
      }
      if (!convId) {
        const now = new Date().toISOString();
        const local: CopilotConversationResponse = {
          id: localConversationId(),
          title: '新对话',
          createdAt: now,
          updatedAt: now,
        };
        setConversations((prev) => [local, ...prev]);
        setActiveId(local.id);
        convId = local.id;
      }
      setMessages([]);
    }

    panelLog('handleSend 开始', { convId, text: text.slice(0, 80), status });

    const userMsg: LocalMessage = { id: `local-${Date.now()}`, role: 'user', content: text.trim() };
    const assistantMsgId = `stream-${Date.now()}`;
    const assistantMsg: LocalMessage = { id: assistantMsgId, role: 'assistant', content: '', streaming: true, toolCalls: [], toolResults: [], canvasOps: [] };

    setMessages((prev) => [...prev, userMsg, assistantMsg]);
    setStatus('generating');
    activeMsgIdRef.current = assistantMsgId;

    // 重置画布操作状态
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
      // 读取 AI 配置以获取 provider
      const aiConfig = await loadAiConfig();
      const activeProvider = resolveGlobalAiProvider(aiConfig);
      if (!activeProvider) {
        setMessages((prev) =>
          prev.map((m) =>
            m.id === assistantMsgId ? { ...m, content: '未配置 AI 提供商，请先在设置中配置', streaming: false } : m,
          ),
        );
        setStatus('idle');
        return;
      }

      const result = await copilotChatStream(
        convId,
        text.trim(),
        activeProvider,
        {
          toolCallingEnabled: aiConfig.agentSettings.toolCallingEnabled,
          userSystemPrompt: aiConfig.agentSettings.systemPrompt ?? undefined,
          temperature: aiConfig.copilotParams.temperature ?? undefined,
          maxTokens: aiConfig.copilotParams.maxTokens ?? undefined,
          topP: aiConfig.copilotParams.topP ?? undefined,
          workspacePath,
        },
        {
          onDelta: (accumulated) => {
            pendingUpdateRef.current = {
              ...pendingUpdateRef.current,
              content: accumulated,
            };
            scheduleFlush();
          },
          onToolCalls: (toolCallInfo) => {
            panelLog('toolCalls 回调', { names: toolCallInfo.names });
            const current = pendingUpdateRef.current;
            pendingUpdateRef.current = {
              ...current,
              content: current?.content ?? '',
              toolCalls: [...(current?.toolCalls ?? []), toolCallInfo],
            };
            scheduleFlush();
          },
          onToolResult: (toolResultInfo) => {
            panelLog('toolResult 回调', { name: toolResultInfo.name, isError: toolResultInfo.isError });
            const current = pendingUpdateRef.current;
            pendingUpdateRef.current = {
              ...current,
              content: current?.content ?? '',
              toolResults: [...(current?.toolResults ?? []), toolResultInfo],
            };
            scheduleFlush();
          },
          onCanvasOp: (op) => {
            panelLog('canvasOp 回调', { type: op.type, ref: op.ref, nodeType: op.nodeType });
          if (op.type === 'create_workflow') {
            if (!boardEnsuredRef.current) {
              panelLog('首次调用 onEnsureBoardOpen');
              onEnsureBoardOpen(op.name);
              boardEnsuredRef.current = true;
            }
          } else if (op.type === 'add_node' && op.nodeId) {
            if (!boardEnsuredRef.current) {
              panelLog('首次调用 onEnsureBoardOpen（add_node）');
              onEnsureBoardOpen();
              boardEnsuredRef.current = true;
            }
            canvasRef.current?.addCanvasOps({
              nodes: [{
                id: op.nodeId,
                type: op.nodeType ?? 'debugConsole',
                label: op.label,
                config: op.config as Record<string, unknown> | undefined,
                connection_id: op.connectionId,
              }],
              edges: [],
            });
          } else if (op.type === 'add_edge' && op.fromRef && op.toRef) {
            canvasRef.current?.addCanvasOps({
              nodes: [],
              edges: [{
                from: op.fromId ?? op.fromRef,
                to: op.toId ?? op.toRef,
                source_port_id: op.sourcePortId,
                target_port_id: op.targetPortId,
              }],
            });
          }

          // 追加到展示数据
            const current = pendingUpdateRef.current;
            pendingUpdateRef.current = {
              ...current,
              content: current?.content ?? '',
              canvasOps: [...(current?.canvasOps ?? []), op],
            };
            scheduleFlush();
          },
        },
        abortController.signal,
      );

      // 流结束，立即 flush 最终状态
      flushPendingUpdate();
      setMessages((prev) =>
        prev.map((m) => (m.id === assistantMsgId ? { ...m, streaming: false } : m)),
      );

      // 从数据库加载最终消息——流式事件可能丢失，
      // 但后端保证在流结束前已持久化 AI 回复
      if (isTauri && convId) {
        try {
          const loaded = await invoke<CopilotMessageResponse[]>('copilot_load_conversation', { id: convId });
          const assistantMsgs = loaded.filter((m) => m.role === 'assistant');
          const lastAssistant = assistantMsgs[assistantMsgs.length - 1];
          if (lastAssistant?.content) {
            setMessages((prev) => {
              const ids = new Set(loaded.map((m) => m.id));
              const merged = prev.map((m) => {
                if (m.id === assistantMsgId) {
                  return { ...m, content: lastAssistant.content, streaming: false };
                }
                return m;
              });
              return merged;
            });
          }
        } catch { /* 数据库加载失败也无所谓，流式内容可能已经到位 */ }
      }

      // 流结束后自动整理画布布局
      if (boardEnsuredRef.current) {
        canvasRef.current?.autoLayout();
      }

      panelLog('流正常结束', {
        textLen: result.text.length,
        aborted: result.aborted,
        finishReason: result.finishReason,
        preview: result.text.slice(0, 200),
      });
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
  }, [activeId, status, isTauri, refreshConversations, canvasRef, onEnsureBoardOpen, flushPendingUpdate, scheduleFlush, workspacePath]);

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
    <section
      ref={panelRef}
      className="copilot-panel"
      style={{ width: panelWidth }}
    >
      <div
        className="copilot-panel__resize-handle"
        onMouseDown={handleResizeStart}
      />
      <div className="copilot-panel__header">
        <button type="button" className="copilot-btn-icon" title="历史会话" onClick={() => setHistoryOpen((prev) => !prev)}>&#9776;</button>
        <button type="button" className="copilot-btn-icon" title="新建对话" onClick={handleNewConversation}>+</button>
        <button type="button" className="copilot-btn-icon" title="收起面板" onClick={() => setCollapsed(true)}>&laquo;</button>
        {historyOpen && (
          <div className="copilot-history-dropdown">
            {conversations.length === 0 ? (
              <div className="copilot-history-empty">暂无历史会话</div>
            ) : (
              conversations.map((conv) => (
                <button
                  key={conv.id}
                  type="button"
                  className={`copilot-history-item${conv.id === activeId ? ' is-active' : ''}`}
                  onClick={() => handleSelectConversation(conv.id)}
                >
                  <span
                    role="button"
                    tabIndex={0}
                    className="copilot-history-item__delete"
                    title="删除会话"
                    onClick={(e) => { e.stopPropagation(); void handleDeleteConversation(conv.id); }}
                    onKeyDown={(e) => { if (e.key === 'Enter') { e.stopPropagation(); void handleDeleteConversation(conv.id); } }}
                  >
                    &times;
                  </span>
                  <span className="copilot-history-item__title">{conv.title}</span>
                  <span className="copilot-history-item__time">
                    {new Date(conv.updatedAt).toLocaleDateString()}
                  </span>
                </button>
              ))
            )}
          </div>
        )}
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
