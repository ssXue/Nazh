import type { SourcePanelProps } from './types';

export function SourcePanel({ astText, graphError, onAstTextChange }: SourcePanelProps) {
  return (
    <>
      <div
        className="panel__header panel__header--desktop window-safe-header"
        data-window-drag-region
      >
        <div>
          <h2>流程源配置</h2>
        </div>
        <span className="panel__badge">单一事实源</span>
      </div>

      <textarea
        className="code-editor code-editor--workspace"
        value={astText}
        onChange={(event) => onAstTextChange(event.target.value)}
        spellCheck={false}
      />

      {graphError ? <p className="panel__error">{graphError}</p> : null}
    </>
  );
}
