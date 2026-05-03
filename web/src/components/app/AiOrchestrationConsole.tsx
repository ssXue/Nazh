import { useEffect, useState } from 'react';

import { useDslOrchestrator } from '../../hooks/use-dsl-orchestrator';
import type { DslOrchestrationPhase } from '../../hooks/use-dsl-orchestrator';
import { AiIcon, FileYamlIcon, RunActionIcon } from './AppIcons';

interface AiOrchestrationConsoleProps {
  isTauriRuntime: boolean;
  onStatusMessage: (message: string) => void;
  onGraphReady?: (graphJson: Record<string, unknown>) => void;
}

const PHASE_LABELS: Record<DslOrchestrationPhase, string> = {
  idle: '等待输入',
  generating: 'AI 生成中...',
  streaming: 'AI 流式输出中...',
  compiling: '编译中...',
  ready: '就绪',
  error: '出错',
};

export function AiOrchestrationConsole({
  isTauriRuntime,
  onStatusMessage,
  onGraphReady,
}: AiOrchestrationConsoleProps) {
  const { state, loadAssets, generate, generateStream, compileProposed, reset } =
    useDslOrchestrator();

  const [goalInput, setGoalInput] = useState('');
  const [useStream, setUseStream] = useState(true);

  useEffect(() => {
    void loadAssets();
  }, [loadAssets]);

  // 编译成功时通知外部
  useEffect(() => {
    if (state.phase === 'ready' && state.compiledGraph && onGraphReady) {
      onGraphReady(state.compiledGraph);
    }
  }, [state.phase, state.compiledGraph, onGraphReady]);

  const handleSubmit = async () => {
    if (!goalInput.trim()) return;
    onStatusMessage('AI 正在生成 Workflow DSL...');
    if (useStream) {
      await generateStream(goalInput);
    } else {
      await generate(goalInput);
    }
    onStatusMessage(state.phase === 'ready' ? 'AI 生成完成' : 'AI 生成遇到问题');
  };

  const handleRecompile = async () => {
    await compileProposed();
    onStatusMessage(state.compileError ? '重新编译失败' : '重新编译成功');
  };

  if (!isTauriRuntime) {
    return (
      <div className="ai-orchestration">
        <div className="ai-orchestration__empty">
          <h2>AI 编排控制台</h2>
          <p>AI 编排功能需要 Tauri 桌面运行时和已配置的 AI 提供者。</p>
        </div>
      </div>
    );
  }

  const isLoading = state.phase === 'generating' || state.phase === 'streaming' || state.phase === 'compiling';

  return (
    <div className="ai-orchestration">
      {/* 左栏：目标输入 + 不确定项 */}
      <div className="ai-orchestration__left">
        <div className="ai-orchestration__section">
          <h3>
            <AiIcon width={14} height={14} />
            编排目标
          </h3>
          <textarea
            className="ai-orchestration__goal-input"
            value={goalInput}
            onChange={(e) => setGoalInput(e.target.value)}
            placeholder="用自然语言描述工作流目标，例如：&#10;当温度传感器超过 80°C 时启动冷却系统，同时发送告警通知"
            rows={4}
            disabled={isLoading}
          />
          <div className="ai-orchestration__actions">
            <button
              type="button"
              className="ai-orchestration__btn ai-orchestration__btn--primary"
              disabled={isLoading || !goalInput.trim()}
              onClick={() => void handleSubmit()}
            >
              <AiIcon width={14} height={14} />
              {isLoading ? PHASE_LABELS[state.phase] : '生成工作流'}
            </button>
            <label className="ai-orchestration__toggle">
              <input
                type="checkbox"
                checked={useStream}
                onChange={(e) => setUseStream(e.target.checked)}
              />
              流式输出
            </label>
            {state.phase !== 'idle' && (
              <button
                type="button"
                className="ai-orchestration__btn"
                onClick={reset}
                disabled={isLoading}
              >
                重置
              </button>
            )}
          </div>
        </div>

        {/* 不确定项 */}
        {state.uncertainties.length > 0 && (
          <div className="ai-orchestration__section">
            <h4>不确定项 ({state.uncertainties.length})</h4>
            <ul className="ai-orchestration__uncertainties">
              {state.uncertainties.map((u, idx) => (
                <li key={idx} className="ai-orchestration__uncertainty">
                  <code>{u.fieldPath}</code>
                  <span className="ai-orchestration__guess">推测: {u.guessedValue}</span>
                  <span className="ai-orchestration__reason">{u.reason}</span>
                </li>
              ))}
            </ul>
          </div>
        )}

        {/* 警告 */}
        {state.warnings.length > 0 && (
          <div className="ai-orchestration__section">
            <h4>警告 ({state.warnings.length})</h4>
            <ul className="ai-orchestration__warnings">
              {state.warnings.map((w, idx) => (
                <li key={idx}>{w}</li>
              ))}
            </ul>
          </div>
        )}
      </div>

      {/* 中栏：YAML 编辑 */}
      <div className="ai-orchestration__center">
        <div className="ai-orchestration__section ai-orchestration__section--flex">
          <h3>
            <FileYamlIcon width={14} height={14} />
            Workflow DSL
          </h3>

          {/* 流式输出实时文本 */}
          {state.phase === 'streaming' && state.streamingText && (
            <pre className="ai-orchestration__stream-preview">{state.streamingText}</pre>
          )}

          {/* 生成的 YAML */}
          {state.proposedYaml && state.phase !== 'streaming' && (
            <textarea
              className="ai-orchestration__yaml-editor"
              value={state.proposedYaml}
              onChange={(e) => {
                const next = { ...state, proposedYaml: e.target.value };
                // 直接让用户编辑，不改变 phase
                void compileProposed(e.target.value);
              }}
              spellCheck={false}
            />
          )}

          {!state.proposedYaml && state.phase !== 'streaming' && (
            <div className="ai-orchestration__placeholder">
              AI 生成的 Workflow DSL 将显示在此处
            </div>
          )}

          {state.compileError && (
            <div className="ai-orchestration__error">
              <strong>编译错误</strong>
              <pre>{state.compileError}</pre>
            </div>
          )}

          {state.proposedYaml && (
            <button
              type="button"
              className="ai-orchestration__btn ai-orchestration__btn--secondary"
              disabled={isLoading}
              onClick={() => void handleRecompile()}
            >
              <RunActionIcon width={14} height={14} />
              重新编译
            </button>
          )}
        </div>
      </div>

      {/* 右栏：资产快照 + 编译结果摘要 */}
      <div className="ai-orchestration__right">
        <div className="ai-orchestration__section">
          <h4>资产快照</h4>
          {state.assetSnapshot ? (
            <div className="ai-orchestration__asset-summary">
              <span>{state.assetSnapshot.devices.length} 设备</span>
              <span>{state.assetSnapshot.capabilities.length} 能力</span>
            </div>
          ) : (
            <span className="ai-orchestration__placeholder">未加载</span>
          )}
        </div>

        {state.compiledGraph && (
          <div className="ai-orchestration__section">
            <h4>编译结果</h4>
            <details>
              <summary>WorkflowGraph JSON</summary>
              <pre className="ai-orchestration__graph-json">
                {JSON.stringify(state.compiledGraph, null, 2)}
              </pre>
            </details>
          </div>
        )}

        {/* 状态指示 */}
        <div className="ai-orchestration__status">
          <span className={`ai-orchestration__phase-badge ai-orchestration__phase-badge--${state.phase}`}>
            {PHASE_LABELS[state.phase]}
          </span>
        </div>
      </div>
    </div>
  );
}
