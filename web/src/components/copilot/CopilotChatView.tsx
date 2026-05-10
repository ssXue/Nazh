import { useCallback, useEffect, useRef, useState } from 'react';

import { CopilotChatInput } from './CopilotChatInput';
import { CopilotMessageItem } from './CopilotMessageItem';

interface LocalMessage {
  id: string;
  role: 'user' | 'assistant';
  content: string;
  streaming?: boolean;
}

interface Props {
  messages: LocalMessage[];
  sending: boolean;
  hasConversation: boolean;
  onSend: (text: string) => void;
  onNewConversation: () => void;
}

export function CopilotChatView({
  messages,
  sending,
  hasConversation,
  onSend,
  onNewConversation,
}: Props) {
  const listRef = useRef<HTMLDivElement>(null);
  const [inputText, setInputText] = useState('');

  useEffect(() => {
    if (listRef.current) {
      listRef.current.scrollTop = listRef.current.scrollHeight;
    }
  }, [messages]);

  const handleSend = useCallback(() => {
    if (!inputText.trim() || sending) return;
    onSend(inputText);
    setInputText('');
  }, [inputText, sending, onSend]);

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
          <CopilotMessageItem key={msg.id} role={msg.role} content={msg.content} streaming={msg.streaming} />
        ))}
      </div>
      <CopilotChatInput
        value={inputText}
        onChange={setInputText}
        onSend={handleSend}
        disabled={sending}
      />
    </div>
  );
}
