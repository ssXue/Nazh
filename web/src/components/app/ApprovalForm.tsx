import { useState } from 'react';

interface FormField {
  type: string;
  name: string;
  label: string;
  required?: boolean;
  default?: unknown;
  min?: number;
  max?: number;
  unit?: string;
  multiline?: boolean;
  maxLength?: number;
  options?: Array<{ value: string; label: string }>;
}

interface ApprovalFormProps {
  formSchema: FormField[];
  onSubmit: (formData: Record<string, unknown>, comment: string) => void;
  onReject: (comment: string) => void;
  disabled?: boolean;
}

export function ApprovalForm({ formSchema, onSubmit, onReject, disabled }: ApprovalFormProps) {
  const initialFormData: Record<string, unknown> = {};
  for (const field of formSchema) {
    initialFormData[field.name] = field.default ?? null;
  }
  const [formData, setFormData] = useState(initialFormData);
  const [comment, setComment] = useState('');

  const updateField = (name: string, value: unknown) => {
    setFormData((prev) => ({ ...prev, [name]: value }));
  };

  return (
    <div style={{ display: 'flex', flexDirection: 'column', gap: 8 }}>
      {formSchema.map((field) => (
        <div key={field.name} style={{ display: 'flex', alignItems: 'center', gap: 8 }}>
          <label style={{ minWidth: 80, fontSize: 12, color: '#aaa' }}>{field.label}</label>
          {field.type === 'boolean' ? (
            <input
              type="checkbox"
              checked={formData[field.name] === true}
              onChange={(e) => updateField(field.name, e.target.checked)}
              disabled={disabled}
            />
          ) : field.type === 'number' ? (
            <input
              type="number"
              value={String(formData[field.name] ?? '')}
              min={field.min}
              max={field.max}
              onChange={(e) => updateField(field.name, e.target.value === '' ? null : Number(e.target.value))}
              disabled={disabled}
              style={{ flex: 1, background: '#1a1a1a', border: '1px solid #333', borderRadius: 'var(--radius-xs)', padding: '2px 6px', color: '#eee', fontSize: 12 }}
            />
          ) : field.type === 'select' ? (
            <select
              value={String(formData[field.name] ?? '')}
              onChange={(e) => updateField(field.name, e.target.value)}
              disabled={disabled}
              style={{ flex: 1, background: '#1a1a1a', border: '1px solid #333', borderRadius: 'var(--radius-xs)', padding: '2px 6px', color: '#eee', fontSize: 12 }}
            >
              <option value="">--</option>
              {field.options?.map((opt) => (
                <option key={opt.value} value={opt.value}>{opt.label}</option>
              ))}
            </select>
          ) : (
            <input
              type="text"
              value={String(formData[field.name] ?? '')}
              onChange={(e) => updateField(field.name, e.target.value)}
              disabled={disabled}
              style={{ flex: 1, background: '#1a1a1a', border: '1px solid #333', borderRadius: 'var(--radius-xs)', padding: '2px 6px', color: '#eee', fontSize: 12 }}
            />
          )}
          {field.unit && <span style={{ fontSize: 11, color: '#888' }}>{field.unit}</span>}
        </div>
      ))}
      <div style={{ display: 'flex', alignItems: 'center', gap: 8, marginTop: 4 }}>
        <label style={{ minWidth: 80, fontSize: 12, color: '#aaa' }}>备注</label>
        <input
          type="text"
          value={comment}
          onChange={(e) => setComment(e.target.value)}
          placeholder="审批意见（可选）"
          disabled={disabled}
          style={{ flex: 1, background: '#1a1a1a', border: '1px solid #333', borderRadius: 'var(--radius-xs)', padding: '2px 6px', color: '#eee', fontSize: 12 }}
        />
      </div>
      <div style={{ display: 'flex', gap: 8, marginTop: 8, justifyContent: 'flex-end' }}>
        <button
          onClick={() => onReject(comment)}
          disabled={disabled}
          style={{ padding: '4px 12px', background: '#5c2020', border: '1px solid #833', borderRadius: 'var(--radius-xs)', color: '#f88', cursor: disabled ? 'not-allowed' : 'pointer', fontSize: 12 }}
        >
          拒绝
        </button>
        <button
          onClick={() => onSubmit(formData, comment)}
          disabled={disabled}
          style={{ padding: '4px 12px', background: '#1a5c1a', border: '1px solid #383', borderRadius: 'var(--radius-xs)', color: '#8f8', cursor: disabled ? 'not-allowed' : 'pointer', fontSize: 12 }}
        >
          通过
        </button>
      </div>
    </div>
  );
}
