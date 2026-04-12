import { useState } from 'react';

import type { PayloadPanelProps } from './types';

export function PayloadPanel({ payloadText, deployInfo, onPayloadTextChange }: PayloadPanelProps) {
  const [isValidJson, setIsValidJson] = useState(true);

  function handleChange(value: string) {
    if (value.trim() === '') {
      setIsValidJson(true);
    } else {
      try {
        JSON.parse(value);
        setIsValidJson(true);
      } catch {
        setIsValidJson(false);
      }
    }
    onPayloadTextChange(value);
  }

  return (
    <>
      <div
        className="panel__header panel__header--desktop window-safe-header"
        data-window-drag-region
      >
        <div>
          <h2>测试载荷</h2>
        </div>
        <span className="panel__badge">{deployInfo ? '已可发送' : '等待部署'}</span>
      </div>

      <textarea
        className={`code-editor code-editor--short code-editor--payload ${!isValidJson ? 'is-invalid' : ''}`}
        value={payloadText}
        onChange={(event) => handleChange(event.target.value)}
        spellCheck={false}
        aria-label="Payload JSON"
      />
    </>
  );
}
