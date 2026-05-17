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
    onSend(inputText.trim());
    setInputText('');
  }, [inputText, status, onSend]);

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
      />
    </div>
  );
}
