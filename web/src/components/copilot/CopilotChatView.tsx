import { useCallback, useEffect, useRef, useState } from 'react';

import { CopilotChatInput } from './CopilotChatInput';
import { CopilotMessageItem } from './CopilotMessageItem';
import type { LocalMessage, CopilotSessionStatus } from './CopilotPanel';

interface Props {
  messages: LocalMessage[];
  status: CopilotSessionStatus;
  hasConversation: boolean;
  onSend: (text: string) => void;
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

  useEffect(() => {
    if (listRef.current) {
      listRef.current.scrollTop = listRef.current.scrollHeight;
    }
  }, [messages]);

  const handleSend = useCallback(() => {
    if (!inputText.trim() || status !== 'idle') return;
    onSend(inputText);
    setInputText('');
  }, [inputText, status, onSend]);

  if (!hasConversation) {
    return (
      <div className="copilot-chat copilot-chat--empty">
        <div className="copilot-chat__welcome">
          <p>Nazh 副驾驶</p>
          <button type="button" className="copilot-btn-primary" onClick={onNewConversation}>
            开始新对话
          </button>
        </div>
      </div>
    );
  }

  return (
    <div className="copilot-chat">
      <div className="copilot-chat__messages" ref={listRef}>
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
      />
    </div>
  );
}
