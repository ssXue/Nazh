import { useCallback, useEffect, useRef } from 'react';

import { useSettings } from '../../hooks/use-settings';

import { BorderGlow } from '../animations/BorderGlow';

import { CopilotAttachmentChip, validateAttachment, type CopilotAttachment } from './CopilotAttachment';
import type { CopilotSessionStatus } from './CopilotPanel';

/** 亮色主题用更深的发光颜色，暗色主题保持原值 */
const GLOW_LIGHT = { glowColor: '220 75 55', colors: ['#3f5cb5', '#4a9e78', '#b8883e'] as const };
const GLOW_DARK  = { glowColor: '220 80 70', colors: ['#5b7fd6', '#6bc9a0', '#d4a056'] as const };

interface Props {
  value: string;
  onChange: (value: string) => void;
  onSend: () => void;
  status: CopilotSessionStatus;
  onCancel: () => void;
  attachment: CopilotAttachment | null;
  onAttachmentChange: (attachment: CopilotAttachment | null) => void;
  onAttachmentError: (msg: string) => void;
}

export function CopilotChatInput({ value, onChange, onSend, status, onCancel, attachment, onAttachmentChange, onAttachmentError }: Props) {
  const generating = status === 'generating';
  const { resolvedThemeMode } = useSettings();
  const isDark = resolvedThemeMode === 'dark';
  const glow = isDark ? GLOW_DARK : GLOW_LIGHT;
  const textareaRef = useRef<HTMLTextAreaElement>(null);

  // value 清空（发送后）时立即重置高度
  useEffect(() => {
    if (!value && textareaRef.current) {
      textareaRef.current.style.height = 'auto';
    }
  }, [value]);

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

  const handleFileSelect = useCallback(() => {
    const input = document.createElement('input');
    input.type = 'file';
    input.accept = '.pdf,.xml,.esi';
    input.onchange = () => {
      const file = input.files?.[0];
      if (!file) return;
      const result = validateAttachment(file);
      if (typeof result === 'string') {
        onAttachmentError(result);
      } else {
        onAttachmentChange(result);
      }
    };
    input.click();
  }, [onAttachmentChange, onAttachmentError]);

  return (
    <BorderGlow
      className="copilot-input"
      animated={generating}
      glowColor={glow.glowColor}
      colors={[...glow.colors]}
      borderRadius={12}
      glowRadius={isDark ? 30 : 24}
      glowIntensity={generating ? 2.0 : 1.2}
      backgroundColor="var(--surface)"
    >
      {attachment && (
        <div className="copilot-attachment-bar">
          <CopilotAttachmentChip
            attachment={attachment}
            onRemove={() => onAttachmentChange(null)}
          />
        </div>
      )}
      <textarea
        ref={textareaRef}
        className="copilot-input__textarea"
        data-testid="copilot-input"
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
        <span className="copilot-input__btn-wrap copilot-input__btn-wrap--dual">
          <button
            type="button"
            className="copilot-input__attach"
            data-testid="copilot-attach"
            onClick={handleFileSelect}
            title="附加文件（PDF / ESI）"
          >
            <svg viewBox="0 0 24 24" width="18" height="18" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round"><path d="M21.44 11.05l-9.19 9.19a6 6 0 01-8.49-8.49l9.19-9.19a4 4 0 015.66 5.66l-9.2 9.19a2 2 0 01-2.83-2.83l8.49-8.48"/></svg>
          </button>
          <button
            type="button"
            className="copilot-input__send"
            data-testid="copilot-send"
            disabled={!value.trim() && !attachment}
            onClick={onSend}
            title="发送"
          >
            <svg viewBox="0 0 24 24" width="16" height="16" fill="currentColor"><path d="M3.478 2.404a.75.75 0 00-.926.941l2.432 7.905H13.5a.75.75 0 010 1.5H4.984l-2.432 7.905a.75.75 0 00.926.94 60.519 60.519 0 0018.445-8.986.75.75 0 000-1.218A60.517 60.517 0 003.478 2.404z"/></svg>
          </button>
        </span>
      )}
    </BorderGlow>
  );
}
