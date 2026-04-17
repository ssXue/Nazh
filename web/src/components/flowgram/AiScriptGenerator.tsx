import { useCallback, useState } from 'react';

export interface AiScriptGeneratorProps {
  open: boolean;
  loading: boolean;
  error: string | null;
  onGenerate: (requirement: string) => void;
  onClose: () => void;
}

export function AiScriptGenerator({
  open,
  loading,
  error,
  onGenerate,
  onClose,
}: AiScriptGeneratorProps) {
  const [requirement, setRequirement] = useState('');

  const handleGenerate = useCallback(() => {
    const trimmed = requirement.trim();
    if (!trimmed) {
      return;
    }
    onGenerate(trimmed);
  }, [requirement, onGenerate]);

  const handleCancel = useCallback(() => {
    if (loading) {
      return;
    }
    setRequirement('');
    onClose();
  }, [loading, onClose]);

  if (!open) {
    return null;
  }

  return (
    <div className="flowgram-overlay" onClick={handleCancel}>
      <section className="flowgram-modal" onClick={(e) => e.stopPropagation()}>
        <div className="flowgram-modal__header">
          <h4>AI 生成脚本</h4>
        </div>
        <div className="flowgram-form">
          <label>
            <span>需求描述</span>
            <textarea
              value={requirement}
              onChange={(e) => setRequirement(e.target.value)}
              placeholder="描述你希望脚本实现的功能..."
              disabled={loading}
              rows={5}
              autoFocus
            />
          </label>
        </div>
        {error ? (
          <div className="flowgram-notes">
            <article className="flowgram-note flowgram-note--danger">{error}</article>
          </div>
        ) : null}
        <div className="flowgram-modal__actions">
          <button type="button" className="ghost" onClick={handleCancel} disabled={loading}>
            取消
          </button>
          <button
            type="button"
            onClick={handleGenerate}
            disabled={loading || !requirement.trim()}
          >
            {loading ? '生成中...' : '生成'}
          </button>
        </div>
      </section>
    </div>
  );
}
