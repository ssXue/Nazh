import { useCallback } from 'react';

import type { NodeSettingsProps } from '../settings-shared';

/**
 * 子图节点设置面板——编辑标签 + 参数绑定（JSON key-value）。
 */
export function SubgraphNodeSettings({ draft, updateDraft }: NodeSettingsProps) {
  const bindingsJson = JSON.stringify(draft.parameterBindings, null, 2);

  const handleBindingsChange = useCallback(
    (event: React.ChangeEvent<HTMLTextAreaElement>) => {
      try {
        const parsed = JSON.parse(event.target.value);
        if (typeof parsed === 'object' && parsed !== null && !Array.isArray(parsed)) {
          updateDraft({
            parameterBindings: parsed,
          });
        }
      } catch {
        // JSON 解析失败时不更新，保持原值
      }
    },
    [updateDraft],
  );

  return (
    <>
      <label>
        <span>参数绑定</span>
        <textarea
          value={bindingsJson}
          onChange={handleBindingsChange}
          placeholder='{"host": "192.168.1.10"}'
          rows={4}
        />
      </label>
    </>
  );
}
