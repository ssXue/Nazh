import { useCallback } from 'react';

interface Props {
  value: string;
  onChange: (value: string) => void;
  onSend: () => void;
  disabled: boolean;
}

export function CopilotChatInput({ value, onChange, onSend, disabled }: Props) {
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
        disabled={disabled}
      />
      <button
        type="button"
        className="copilot-input__send"
        disabled={disabled || !value.trim()}
        onClick={onSend}
        title="发送"
      >
        &uarr;
      </button>
    </div>
  );
}
