import { useEffect, useRef } from 'react';

import type {
  WorkflowOrchestrationDraft,
  WorkflowOrchestrationMode,
  WorkflowOrchestrationOperation,
} from '../../lib/workflow-orchestrator';
import { describeWorkflowOrchestrationOperation } from '../../lib/workflow-orchestrator';

interface AiWorkflowComposerProps {
  open: boolean;
  mode: WorkflowOrchestrationMode;
  activeProjectName?: string | null;
  status: 'idle' | 'generating' | 'completed' | 'interrupted';
  generating: boolean;
  requirement: string;
  error?: string | null;
  rawText?: string | null;
  thinkingText?: string | null;
  draft?: WorkflowOrchestrationDraft | null;
  operations: WorkflowOrchestrationOperation[];
  onRequirementChange: (value: string) => void;
  onClose: () => void;
  onSubmit: () => void;
}

function getDialogTitle(mode: WorkflowOrchestrationMode): string {
  return mode === 'create' ? 'AI 流式编排' : 'AI 编辑工作流';
}

function getDialogHint(mode: WorkflowOrchestrationMode, activeProjectName?: string | null): string {
  if (mode === 'create') {
    return '描述你的业务目标，AI 会以操作流的方式逐步搭建工作流，并在编排过程中持续更新结果。';
  }

  return activeProjectName
    ? `基于当前工程“${activeProjectName}”继续流式修改，只会逐步输出必要的节点和连线变更。`
    : '基于当前工作流继续流式修改。';
}

function getStatusLabel(
  status: 'idle' | 'generating' | 'completed' | 'interrupted',
  mode: WorkflowOrchestrationMode,
): string {
  switch (status) {
    case 'generating':
      return mode === 'create' ? '流式编排中' : '流式编辑中';
    case 'completed':
      return mode === 'create' ? '编排已完成' : '编辑已完成';
    case 'interrupted':
      return mode === 'create' ? '编排已中断' : '编辑已中断';
    case 'idle':
      return mode === 'create' ? '等待开始' : '准备编辑';
  }
}

function getSubmitLabel(
  status: 'idle' | 'generating' | 'completed' | 'interrupted',
  mode: WorkflowOrchestrationMode,
): string {
  if (status === 'generating') {
    return mode === 'create' ? '编排中...' : '编辑中...';
  }

  if (status === 'interrupted') {
    return mode === 'create' ? '重新开始编排' : '重新开始编辑';
  }

  if (status === 'completed') {
    return mode === 'create' ? '再次编排' : '再次编辑';
  }

  return mode === 'create' ? '开始编排' : '开始编辑';
}

function scrollContainerToBottom(element: HTMLElement | null) {
  if (!element) {
    return;
  }

  element.scrollTop = element.scrollHeight;
}

export function AiWorkflowComposer({
  open,
  mode,
  activeProjectName,
  status,
  generating,
  requirement,
  error,
  rawText,
  thinkingText,
  draft,
  operations,
  onRequirementChange,
  onClose,
  onSubmit,
}: AiWorkflowComposerProps) {
  if (!open) {
    return null;
  }

  const streamListRef = useRef<HTMLDivElement | null>(null);
  const thinkingBodyRef = useRef<HTMLPreElement | null>(null);
  const rawBodyRef = useRef<HTMLPreElement | null>(null);
  const graphNodeCount = draft ? Object.keys(draft.graph.nodes).length : 0;
  const graphEdgeCount = draft?.graph.edges.length ?? 0;
  const hasStreamOutput = Boolean(rawText && rawText.trim());

  useEffect(() => {
    if (!generating) {
      return;
    }

    const frameId = window.requestAnimationFrame(() => {
      scrollContainerToBottom(streamListRef.current);
    });

    return () => {
      window.cancelAnimationFrame(frameId);
    };
  }, [generating, operations.length]);

  useEffect(() => {
    if (!generating) {
      return;
    }

    const frameId = window.requestAnimationFrame(() => {
      scrollContainerToBottom(thinkingBodyRef.current);
    });

    return () => {
      window.cancelAnimationFrame(frameId);
    };
  }, [generating, thinkingText]);

  useEffect(() => {
    if (!generating) {
      return;
    }

    const frameId = window.requestAnimationFrame(() => {
      scrollContainerToBottom(rawBodyRef.current);
    });

    return () => {
      window.cancelAnimationFrame(frameId);
    };
  }, [generating, rawText]);

  return (
    <div
      className="workflow-orchestrator-dialog-layer"
      onClick={() => {
        if (!generating) {
          onClose();
        }
      }}
    >
      <div
        className="workflow-orchestrator-dialog"
        role="dialog"
        aria-modal="true"
        aria-labelledby="workflow-orchestrator-title"
        onClick={(event) => event.stopPropagation()}
      >
        <div className="workflow-orchestrator-dialog__header">
          <div className="workflow-orchestrator-dialog__copy">
            <strong id="workflow-orchestrator-title">{getDialogTitle(mode)}</strong>
            <p>{getDialogHint(mode, activeProjectName)}</p>
          </div>
          <div className="workflow-orchestrator-dialog__chips">
            <span className="workflow-orchestrator-dialog__chip">
              {getStatusLabel(status, mode)}
            </span>
            {draft ? (
              <span className="workflow-orchestrator-dialog__chip workflow-orchestrator-dialog__chip--accent">
                {draft.name}
              </span>
            ) : null}
          </div>
        </div>

        <label className="workflow-orchestrator-dialog__field">
          <span>需求描述</span>
          <textarea
            value={requirement}
            disabled={generating}
            placeholder={
              mode === 'create'
                ? '例如：每 5 秒采集一次温度，超过阈值后写入 SQLite 并输出到调试台'
                : '例如：保留现有采集链路，再增加 HTTP 上报和告警分支'
            }
            onChange={(event) => onRequirementChange(event.target.value)}
          />
        </label>

        {draft ? (
          <section className="workflow-orchestrator-dialog__summary">
            <div className="workflow-orchestrator-dialog__summary-head">
              <strong>实时草稿</strong>
              <span>{draft.description || 'AI 正在组织工作流说明。'}</span>
            </div>
            <div className="workflow-orchestrator-dialog__summary-metrics">
              <span>{graphNodeCount} 节点</span>
              <span>{graphEdgeCount} 连线</span>
              <span>{operations.length} 操作</span>
            </div>
          </section>
        ) : null}

        {error ? (
          <article className="workflow-orchestrator-dialog__notice workflow-orchestrator-dialog__notice--danger">
            {error}
          </article>
        ) : null}

        <section className="workflow-orchestrator-dialog__stream">
          <div className="workflow-orchestrator-dialog__stream-head">
            <strong>流式操作</strong>
            <span>{operations.length === 0 ? '等待 AI 输出第一条操作…' : '按收到顺序展示'}</span>
          </div>

          {operations.length === 0 ? (
            <div className="workflow-orchestrator-dialog__empty">
              <strong>还没有收到结构化操作</strong>
              <span>一旦 AI 开始输出节点、连线或工程元信息，这里会实时滚动更新。</span>
            </div>
          ) : (
            <div ref={streamListRef} className="workflow-orchestrator-dialog__stream-list">
              {operations.map((operation, index) => (
                <article
                  key={`${operation.type}-${index}`}
                  className="workflow-orchestrator-dialog__stream-item"
                >
                  <span className="workflow-orchestrator-dialog__stream-index">
                    {String(index + 1).padStart(2, '0')}
                  </span>
                  <span className="workflow-orchestrator-dialog__stream-copy">
                    {describeWorkflowOrchestrationOperation(operation)}
                  </span>
                </article>
              ))}
            </div>
          )}
        </section>

        {thinkingText && thinkingText.trim() ? (
          <details className="workflow-orchestrator-dialog__thinking" open={!hasStreamOutput}>
            <summary>思考过程</summary>
            <pre ref={thinkingBodyRef}><code>{thinkingText}</code></pre>
          </details>
        ) : null}

        {rawText && rawText.trim() ? (
          <details className="workflow-orchestrator-dialog__raw">
            <summary>原始流输出</summary>
            <pre ref={rawBodyRef}><code>{rawText}</code></pre>
          </details>
        ) : null}

        <div className="workflow-orchestrator-dialog__actions">
          <button
            type="button"
            className="workflow-orchestrator-dialog__action"
            disabled={generating}
            onClick={onClose}
          >
            关闭
          </button>
          <button
            type="button"
            className="workflow-orchestrator-dialog__action workflow-orchestrator-dialog__action--primary"
            disabled={generating || !requirement.trim()}
            onClick={onSubmit}
          >
            {getSubmitLabel(status, mode)}
          </button>
        </div>
      </div>
    </div>
  );
}
