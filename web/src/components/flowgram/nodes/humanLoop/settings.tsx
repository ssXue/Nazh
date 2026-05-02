import { useCallback } from 'react';

import type { NodeSettingsProps } from '../settings-shared';

interface SelectOption {
  value: string;
  label: string;
}

interface FieldRow {
  type: 'boolean' | 'number' | 'string' | 'select';
  name: string;
  label: string;
  required: boolean;
  options?: SelectOption[];
}

function parseFields(json: string): FieldRow[] {
  if (!json.trim()) return [];
  try {
    const arr = JSON.parse(json);
    if (!Array.isArray(arr)) return [];
    return arr.filter(
      (f: unknown): f is FieldRow =>
        typeof f === 'object' && f !== null &&
        typeof (f as Record<string, unknown>).name === 'string',
    ).map((f) => ({
      ...f,
      options: Array.isArray(f.options) ? f.options : [],
    }));
  } catch {
    return [];
  }
}

function serializeFields(fields: FieldRow[]): string {
  return fields.length ? JSON.stringify(fields, null, 2) : '';
}

export function HumanLoopNodeSettings({ draft, updateDraft }: NodeSettingsProps) {
  const fields = parseFields(draft.hitlFormSchemaJson);

  const updateFields = useCallback(
    (next: FieldRow[]) => {
      updateDraft({ hitlFormSchemaJson: serializeFields(next) });
    },
    [updateDraft],
  );

  const updateField = useCallback(
    (index: number, patch: Partial<FieldRow>) => {
      updateFields(fields.map((f, i) => (i === index ? { ...f, ...patch } : f)));
    },
    [fields, updateFields],
  );

  const addField = useCallback(() => {
    updateFields([...fields, { type: 'string', name: '', label: '', required: false }]);
  }, [fields, updateFields]);

  const removeField = useCallback(
    (index: number) => {
      updateFields(fields.filter((_, i) => i !== index));
    },
    [fields, updateFields],
  );

  const updateOption = useCallback(
    (fieldIndex: number, optIndex: number, patch: Partial<SelectOption>) => {
      const field = fields[fieldIndex];
      if (!field) return;
      const opts = [...(field.options ?? [])];
      opts[optIndex] = { ...opts[optIndex], ...patch };
      updateField(fieldIndex, { options: opts });
    },
    [fields, updateField],
  );

  const addOption = useCallback(
    (fieldIndex: number) => {
      const field = fields[fieldIndex];
      if (!field) return;
      updateField(fieldIndex, { options: [...(field.options ?? []), { value: '', label: '' }] });
    },
    [fields, updateField],
  );

  const removeOption = useCallback(
    (fieldIndex: number, optIndex: number) => {
      const field = fields[fieldIndex];
      if (!field) return;
      updateField(fieldIndex, { options: (field.options ?? []).filter((_, i) => i !== optIndex) });
    },
    [fields, updateField],
  );

  return (
    <>
      <label>
        <span>审批标题</span>
        <input
          value={draft.hitlTitle}
          onChange={(e) => updateDraft({ hitlTitle: e.target.value })}
          placeholder="例如：液压操作确认"
        />
      </label>
      <label>
        <span>审批说明</span>
        <textarea
          value={draft.hitlDescription}
          onChange={(e) => updateDraft({ hitlDescription: e.target.value })}
          placeholder="向审批人说明需要确认的内容"
          rows={2}
        />
      </label>
      <label>
        <span>审批超时(秒)</span>
        <input
          value={draft.hitlApprovalTimeoutSec}
          onChange={(e) => updateDraft({ hitlApprovalTimeoutSec: e.target.value })}
          placeholder="留空 = 无限等待"
          min="1"
        />
      </label>
      <label>
        <span>超时默认动作</span>
        <select
          value={draft.hitlDefaultAction}
          onChange={(e) => updateDraft({ hitlDefaultAction: e.target.value })}
        >
          <option value="autoReject">自动拒绝</option>
          <option value="autoApprove">自动通过</option>
        </select>
      </label>

      <div className="hitl-field-list">
        <span>表单字段</span>
        {fields.map((field, i) => (
          <div key={i}>
            <div className="hitl-field-row">
              <select
                value={field.type}
                onChange={(e) =>
                  updateField(i, { type: e.target.value as FieldRow['type'] })
                }
              >
                <option value="string">文本</option>
                <option value="number">数值</option>
                <option value="boolean">布尔</option>
                <option value="select">选择</option>
              </select>
              <input
                value={field.name}
                onChange={(e) => updateField(i, { name: e.target.value })}
                placeholder="字段名"
              />
              <input
                value={field.label}
                onChange={(e) => updateField(i, { label: e.target.value })}
                placeholder="显示标签"
              />
              <button type="button" className="hitl-field-row__remove" onClick={() => removeField(i)} title="删除">✕</button>
            </div>
            {field.type === 'select' && (
              <div className="hitl-field-options">
                {(field.options ?? []).map((opt, oi) => (
                  <div key={oi} className="hitl-field-option">
                    <input
                      value={opt.value}
                      onChange={(e) => updateOption(i, oi, { value: e.target.value })}
                      placeholder="值"
                    />
                    <input
                      value={opt.label}
                      onChange={(e) => updateOption(i, oi, { label: e.target.value })}
                      placeholder="标签"
                    />
                    <button type="button" className="hitl-field-row__remove" onClick={() => removeOption(i, oi)} title="删除">✕</button>
                  </div>
                ))}
                <button type="button" className="hitl-field-list__add" onClick={() => addOption(i)}>+ 选项</button>
              </div>
            )}
          </div>
        ))}
        <button type="button" className="hitl-field-list__add" onClick={addField}>+ 添加字段</button>
      </div>
    </>
  );
}
