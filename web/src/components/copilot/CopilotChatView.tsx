import { useCallback, useEffect, useRef, useState } from 'react';

import { type CopilotAttachment } from './CopilotAttachment';
import { CopilotChatInput } from './CopilotChatInput';
import { CopilotMessageItem } from './CopilotMessageItem';
import type { LocalMessage, CopilotSessionStatus } from './CopilotPanel';

interface Props {
  messages: LocalMessage[];
  status: CopilotSessionStatus;
  hasConversation: boolean;
  onSend: (text: string, attachment?: CopilotAttachment | null) => void;
  onNewConversation: () => void;
  onCancel: () => void;
}

export function CopilotChatView({
  messages,
  status,
  hasConversation,
  onSend,
  onNewConversation,
  onCancel,
}: Props) {
  const listRef = useRef<HTMLDivElement>(null);
  const [inputText, setInputText] = useState('');
  const [attachment, setAttachment] = useState<CopilotAttachment | null>(null);

  /// 发送后附件自动清除（由外部 handleSend 触发）。
  const clearAttachment = useCallback(() => setAttachment(null), []);

  const showToast = useCallback((msg: string) => {
    // 简易 toast——追加一条系统消息
    console.warn('[attachment]', msg);
  }, []);

  useEffect(() => {
    if (listRef.current) {
      listRef.current.scrollTop = listRef.current.scrollHeight;
    }
  }, [messages]);

  const handleSend = useCallback(() => {
    if ((!inputText.trim() && !attachment) || status !== 'idle') return;
    onSend(inputText.trim(), attachment);
    setInputText('');
    clearAttachment();
  }, [inputText, attachment, status, onSend, clearAttachment]);

  return (
    <div className="copilot-chat">
      <div className="copilot-chat__messages" data-testid="copilot-messages" ref={listRef}>
        {!hasConversation && (
          <div className="copilot-chat__welcome">
            <p>Nazh 副驾驶</p>
          </div>
        )}
        {messages.map((msg) => (
          <CopilotMessageItem
            key={msg.id}
            role={msg.role}
            content={msg.content}
            streaming={msg.streaming}
            toolCalls={msg.toolCalls}
            toolResults={msg.toolResults}
            canvasOps={msg.canvasOps}
          />
        ))}
      </div>
      <CopilotChatInput
        value={inputText}
        onChange={setInputText}
        onSend={handleSend}
        status={status}
        onCancel={onCancel}
        attachment={attachment}
        onAttachmentChange={setAttachment}
        onAttachmentError={showToast}
      />
    </div>
  );
}
