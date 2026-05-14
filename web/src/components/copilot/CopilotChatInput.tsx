import { useCallback } from 'react';

import { BorderGlow } from '../animations/BorderGlow';

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

  const handleChange = useCallback((e: React.ChangeEvent<HTMLTextAreaElement>) => {
    const el = e.target;
    el.style.height = 'auto';
    el.style.height = `${el.scrollHeight}px`;
    onChange(el.value);
  }, [onChange]);

  return (
    <BorderGlow
      className="copilot-input"
      animated={generating}
      glowColor="220 80 70"
      colors={['#5b7fd6', '#6bc9a0', '#d4a056']}
      borderRadius={12}
      glowRadius={30}
      glowIntensity={generating ? 2.0 : 1.2}
      backgroundColor="var(--surface)"
    >
      <textarea
        className="copilot-input__textarea"
        placeholder="输入消息… (Enter 发送，Shift+Enter 换行)"
        rows={1}
        value={value}
        onChange={handleChange}
        onKeyDown={handleKeyDown}
        disabled={generating}
      />
      {generating ? (
        <span className="copilot-input__btn-wrap">
          <button
            type="button"
            className="copilot-input__stop"
            onClick={onCancel}
            title="停止生成"
          >
            &#9632;
          </button>
        </span>
      ) : (
        <span className="copilot-input__btn-wrap">
          <button
            type="button"
            className="copilot-input__send"
            disabled={!value.trim()}
            onClick={onSend}
            title="发送"
          >
            &uarr;
          </button>
        </span>
      )}
    </BorderGlow>
  );
}
