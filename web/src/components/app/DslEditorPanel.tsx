import { useEffect } from 'react';

import { useDslEditor } from '../../hooks/use-dsl-editor';
import { FileYamlIcon, RunActionIcon } from './AppIcons';

interface DslEditorPanelProps {
  isTauriRuntime: boolean;
  onStatusMessage: (message: string) => void;
  /** 可选：外部传入的初始 YAML。 */
  initialYaml?: string;
  /** 可选：编译成功后的回调。 */
  onGraphReady?: (graphJson: Record<string, unknown>) => void;
}

export function DslEditorPanel({
  isTauriRuntime,
  onStatusMessage,
  initialYaml,
  onGraphReady,
}: DslEditorPanelProps) {
  const {
    yamlText,
    setYamlText,
    compiling,
    compileResult,
    diagnostics,
    assetSnapshot,
    error,
    compile,
    loadSnapshot,
  } = useDslEditor();

  useEffect(() => {
    if (initialYaml) {
      setYamlText(initialYaml);
    }
  }, [initialYaml, setYamlText]);

  useEffect(() => {
    void loadSnapshot();
  }, [loadSnapshot]);

  const handleCompile = async () => {
    await compile();
    onStatusMessage(compileResult?.error ? '编译失败' : '编译成功');
  };

  // 编译成功时通知外部
  useEffect(() => {
    if (compileResult?.graphJson && !compileResult.error && onGraphReady) {
      onGraphReady(compileResult.graphJson);
    }
  }, [compileResult, onGraphReady]);

  if (!isTauriRuntime) {
    return (
      <div className="dsl-editor">
        <div className="dsl-editor__empty">
          <h2>DSL 编辑器</h2>
          <p>DSL 编辑功能需要 Tauri 桌面运行时。</p>
        </div>
      </div>
    );
  }

  return (
    <div className="dsl-editor">
      <div className="dsl-editor__header">
        <FileYamlIcon width={16} height={16} />
        <span>Workflow DSL</span>
        <button
          type="button"
          className="dsl-editor__compile-btn"
          disabled={compiling || !yamlText.trim()}
          onClick={() => void handleCompile()}
        >
          <RunActionIcon width={14} height={14} />
          {compiling ? '编译中...' : '编译'}
        </button>
      </div>

      <div className="dsl-editor__body">
        <textarea
          className="dsl-editor__textarea"
          value={yamlText}
          onChange={(e) => setYamlText(e.target.value)}
          placeholder={`# Workflow DSL 示例\nid: my_workflow\nversion: "1.0.0"\nstates:\n  idle:\n  running:\ntransitions:\n  - from: idle\n    to: running\n    when: "start == true"`}
          spellCheck={false}
        />

        {/* 编译结果 */}
        {error && (
          <div className="dsl-editor__error">
            <strong>编译错误</strong>
            <pre>{error}</pre>
          </div>
        )}

        {diagnostics.length > 0 && (
          <div className="dsl-editor__diagnostics">
            <h4>诊断 ({diagnostics.length})</h4>
            <ul>
              {diagnostics.map((d, idx) => (
                <li key={idx} className={`dsl-editor__diagnostic dsl-editor__diagnostic--${d.severity}`}>
                  [{d.severity}] {d.message}
                </li>
              ))}
            </ul>
          </div>
        )}

        {compileResult?.graphJson && (
          <details className="dsl-editor__result">
            <summary>WorkflowGraph JSON</summary>
            <pre className="dsl-editor__result-json">
              {JSON.stringify(compileResult.graphJson, null, 2)}
            </pre>
          </details>
        )}
      </div>

      {/* 资产快照 */}
      {assetSnapshot && (assetSnapshot.devices.length > 0 || assetSnapshot.capabilities.length > 0) && (
        <details className="dsl-editor__assets">
          <summary>
            资产快照 ({assetSnapshot.devices.length} 设备 · {assetSnapshot.capabilities.length} 能力)
          </summary>
          <div className="dsl-editor__asset-list">
            {assetSnapshot.devices.map((d, idx) => (
              <div key={idx} className="dsl-editor__asset-item">
                <span className="dsl-editor__asset-tag">设备</span>
                <code>{String(d.id ?? `#${idx}`)}</code>
              </div>
            ))}
            {assetSnapshot.capabilities.map((c, idx) => (
              <div key={idx} className="dsl-editor__asset-item">
                <span className="dsl-editor__asset-tag">能力</span>
                <code>{String(c.id ?? `#${idx}`)}</code>
              </div>
            ))}
          </div>
        </details>
      )}
    </div>
  );
}
