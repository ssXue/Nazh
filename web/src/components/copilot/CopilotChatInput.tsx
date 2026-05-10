import { useCallback } from 'react';

import type { CopilotSessionStatus } from './CopilotPanel';

interface Props {
  value: string;
  onChange: (value: string) => void;
  onSend: () => void;
  status: CopilotSessionStatus;
  onCancel: () => void;
}

export function CopilotChatInput({ value, onChange, onSend, status, onCancel }: Props) {
  const generating = status === 'generating';

  const handleKeyDown = useCallback(
    (e: React.KeyboardEvent<HTMLTextAreaElement>) => {
      if (e.key === 'Enter' && !e.shiftKey) {
        e.preventDefault();
        onSend();
      }
    },
    [onSend],
  );

  return (
    <div className="copilot-input">
      <textarea
        className="copilot-input__textarea"
        placeholder="输入消息… (Enter 发送，Shift+Enter 换行)"
        rows={1}
        value={value}
        onChange={(e) => onChange(e.target.value)}
        onKeyDown={handleKeyDown}
        disabled={generating}
      />
      {generating ? (
        <button
          type="button"
          className="copilot-input__stop"
          onClick={onCancel}
          title="停止生成"
        >
          &#9632;
        </button>
      ) : (
        <button
          type="button"
          className="copilot-input__send"
          disabled={!value.trim()}
          onClick={onSend}
          title="发送"
        >
          &uarr;
        </button>
      )}
    </div>
  );
}
