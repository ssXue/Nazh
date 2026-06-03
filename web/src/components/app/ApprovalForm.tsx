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
    <div className="approval-form">
      {formSchema.map((field) => (
        <div key={field.name} className="approval-form__row">
          <label className="approval-form__label">{field.label}</label>
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
              className="approval-form__input"
            />
          ) : field.type === 'select' ? (
            <select
              value={String(formData[field.name] ?? '')}
              onChange={(e) => updateField(field.name, e.target.value)}
              disabled={disabled}
              className="approval-form__input"
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
              className="approval-form__input"
            />
          )}
          {field.unit && <span className="approval-form__unit">{field.unit}</span>}
        </div>
      ))}
      <div className="approval-form__row" style={{ marginTop: 4 }}>
        <label className="approval-form__label">备注</label>
        <input
          type="text"
          value={comment}
          onChange={(e) => setComment(e.target.value)}
          placeholder="审批意见（可选）"
          disabled={disabled}
          className="approval-form__input"
        />
      </div>
      <div className="approval-form__actions">
        <button
          onClick={() => onReject(comment)}
          disabled={disabled}
          className="approval-form__btn--reject"
        >
          拒绝
        </button>
        <button
          onClick={() => onSubmit(formData, comment)}
          disabled={disabled}
          className="approval-form__btn--approve"
        >
          通过
        </button>
      </div>
    </div>
  );
}
