import { useCallback, useEffect, useState } from 'react';

import { invoke } from '@tauri-apps/api/core';

import type { CopilotConversationResponse } from '../../generated/CopilotConversationResponse';
import type { CopilotMessageResponse } from '../../generated/CopilotMessageResponse';
import { copilotChatStream } from '../../lib/copilot-stream';
import { hasTauriRuntime } from '../../lib/tauri';
import { CopilotChatView } from './CopilotChatView';
import { CopilotConversationList } from './CopilotConversationList';

interface LocalMessage {
  id: string;
  role: 'user' | 'assistant';
  content: string;
  streaming?: boolean;
}

function localConversationId(): string {
  return `local-${Date.now()}-${Math.random().toString(16).slice(2)}`;
}

export function CopilotPanel() {
  const [collapsed, setCollapsed] = useState(false);
  const [conversations, setConversations] = useState<CopilotConversationResponse[]>([]);
  const [activeId, setActiveId] = useState<string | null>(null);
  const [messages, setMessages] = useState<LocalMessage[]>([]);
  const [sending, setSending] = useState(false);
  const isTauri = hasTauriRuntime();

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

  const handleSend = useCallback(async (text: string) => {
    if (!activeId || !text.trim() || sending) return;

    const userMsg: LocalMessage = { id: `local-${Date.now()}`, role: 'user', content: text.trim() };
    const assistantMsg: LocalMessage = { id: `stream-${Date.now()}`, role: 'assistant', content: '', streaming: true };

    setMessages((prev) => [...prev, userMsg, assistantMsg]);
    setSending(true);

    if (!isTauri) {
      setMessages((prev) =>
        prev.map((m) =>
          m.id === assistantMsg.id ? { ...m, content: '预览模式：AI 不可用，请在 Tauri 桌面端使用', streaming: false } : m,
        ),
      );
      setSending(false);
      return;
    }

    try {
      await copilotChatStream(
        activeId,
        text.trim(),
        (accumulated) => {
          setMessages((prev) =>
            prev.map((m) => (m.id === assistantMsg.id ? { ...m, content: accumulated } : m)),
          );
        },
      );
    } catch (error) {
      const errorMessage = error instanceof Error ? error.message : String(error);
      setMessages((prev) =>
        prev.map((m) =>
          m.id === assistantMsg.id ? { ...m, content: `错误: ${errorMessage}`, streaming: false } : m,
        ),
      );
    } finally {
      setMessages((prev) =>
        prev.map((m) => (m.id === assistantMsg.id ? { ...m, streaming: false } : m)),
      );
      setSending(false);
      void refreshConversations();
    }
  }, [activeId, sending, isTauri, refreshConversations]);

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
      <div className="copilot-panel__sidebar">
        <CopilotConversationList
          conversations={conversations}
          activeId={activeId}
          onSelect={handleSelectConversation}
          onNew={handleNewConversation}
          onDelete={handleDeleteConversation}
          onCollapse={() => setCollapsed(true)}
        />
      </div>
      <div className="copilot-panel__main">
        <CopilotChatView
          messages={messages}
          sending={sending}
          hasConversation={Boolean(activeId)}
          onSend={handleSend}
          onNewConversation={handleNewConversation}
        />
      </div>
    </section>
  );
}
