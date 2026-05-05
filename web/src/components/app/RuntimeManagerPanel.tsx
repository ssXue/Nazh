import { useEffect, useMemo, useState } from 'react';
import { JsonView, collapseAllNested, darkStyles, defaultStyles } from 'react-json-view-lite';
import 'react-json-view-lite/dist/index.css';

import {
  CopyIcon,
  HistoryIcon,
  InspectIcon,
  RunActionIcon,
  StopActionIcon,
} from './AppIcons';
import type { DeadLetterRecord, RuntimeWorkflowSummary } from '../../types';
import {
  hasTauriRuntime,
  listDeadLetters,
  listRuntimeWorkflows,
  setActiveRuntimeWorkflow,
  undeployWorkflow,
} from '../../lib/tauri';
import { describeUnknownError } from '../../lib/workflow-events';
import { formatLogTimestamp } from './runtime-console';
import type { RuntimeManagerPanelProps } from './types';

type DeadLetterScope = 'selected' | 'all';

function formatRelativeStamp(value: string): string {
  const timestamp = Date.parse(value);
  if (Number.isNaN(timestamp)) {
    return value;
  }

  const date = new Date(timestamp);
  const now = Date.now();
  const deltaMs = now - timestamp;
  const deltaMinutes = Math.round(deltaMs / 60_000);

  if (deltaMinutes < 1) {
    return '刚刚';
  }

  if (deltaMinutes < 60) {
    return `${deltaMinutes} 分钟前`;
  }

  const deltaHours = Math.round(deltaMinutes / 60);
  if (deltaHours < 24) {
    return `${deltaHours} 小时前`;
  }

  return new Intl.DateTimeFormat('zh-CN', {
    month: '2-digit',
    day: '2-digit',
    hour: '2-digit',
    minute: '2-digit',
  }).format(date);
}

function formatDuration(ms: number): string {
  if (ms >= 1000) {
    return `${(ms / 1000).toFixed(ms >= 10_000 ? 0 : 1)} s`;
  }
  return `${ms} ms`;
}

function formatPayloadPreview(payload: DeadLetterRecord['payload']): string {
  try {
    const text = JSON.stringify(payload);
    if (text.length <= 96) {
      return text;
    }
    return `${text.slice(0, 93)}...`;
  } catch {
    return '[payload]';
  }
}

function normalizeJsonTree(payload: DeadLetterRecord['payload']): Record<string, unknown> | unknown[] {
  if (Array.isArray(payload)) {
    return payload;
  }

  if (payload && typeof payload === 'object') {
    return payload as Record<string, unknown>;
  }

  return { value: payload };
}

async function copyText(value: string): Promise<boolean> {
  try {
    if (navigator.clipboard?.writeText) {
      await navigator.clipboard.writeText(value);
      return true;
    }

    const textarea = document.createElement('textarea');
    textarea.value = value;
    textarea.setAttribute('readonly', 'true');
    textarea.style.position = 'absolute';
    textarea.style.left = '-9999px';
    document.body.appendChild(textarea);
    textarea.select();
    document.execCommand('copy');
    document.body.removeChild(textarea);
    return true;
  } catch {
    return false;
  }
}

export function RuntimeManagerPanel({
  workspacePath,
  themeMode,
  activeBoardId,
  onOpenBoard,
  onPersistActiveProject,
  onBeforeWorkflowStop,
  onAfterWorkflowStop,
  onRemovePersistedDeployment,
  onStatusMessage,
  onRuntimeCountChange,
  initialWorkflows = [],
  initialDeadLetters = [],
}: RuntimeManagerPanelProps) {
  const [workflows, setWorkflows] = useState<RuntimeWorkflowSummary[]>(initialWorkflows);
  const [deadLetters, setDeadLetters] = useState<DeadLetterRecord[]>(initialDeadLetters);
  const [selectedWorkflowId, setSelectedWorkflowId] = useState<string | null>(
    initialWorkflows.find((workflow) => workflow.active)?.workflowId ?? initialWorkflows[0]?.workflowId ?? null,
  );
  const [selectedDeadLetterId, setSelectedDeadLetterId] = useState<string | null>(
    initialDeadLetters[0]?.id ?? null,
  );
  const [deadLetterScope, setDeadLetterScope] = useState<DeadLetterScope>('selected');
  const [isLoading, setIsLoading] = useState(false);
  const [isDeadLettersLoading, setIsDeadLettersLoading] = useState(false);
  const [runtimeError, setRuntimeError] = useState<string | null>(null);
  const [deadLetterError, setDeadLetterError] = useState<string | null>(null);
  const [pendingAction, setPendingAction] = useState<string | null>(null);
  const [hasCopiedPayload, setHasCopiedPayload] = useState(false);

  useEffect(() => {
    onRuntimeCountChange?.(workflows.length);
  }, [onRuntimeCountChange, workflows.length]);

  useEffect(() => {
    if (!hasTauriRuntime()) {
      return;
    }

    let cancelled = false;

    const loadWorkflows = async () => {
      setIsLoading(true);
      try {
        const nextWorkflows = await listRuntimeWorkflows();
        if (cancelled) {
          return;
        }
        setWorkflows(nextWorkflows);
        setRuntimeError(null);
        setSelectedWorkflowId((current) => {
          if (current && nextWorkflows.some((workflow) => workflow.workflowId === current)) {
            return current;
          }
          return (
            nextWorkflows.find((workflow) => workflow.active)?.workflowId ??
            nextWorkflows[0]?.workflowId ??
            null
          );
        });
      } catch (error) {
        if (cancelled) {
          return;
        }
        const { message } = describeUnknownError(error);
        setRuntimeError(message);
      } finally {
        if (!cancelled) {
          setIsLoading(false);
        }
      }
    };

    void loadWorkflows();
    const timer = window.setInterval(() => {
      void loadWorkflows();
    }, 2500);

    return () => {
      cancelled = true;
      window.clearInterval(timer);
    };
  }, []);

  const selectedWorkflow = useMemo(
    () => workflows.find((workflow) => workflow.workflowId === selectedWorkflowId) ?? null,
    [selectedWorkflowId, workflows],
  );

  useEffect(() => {
    if (!hasTauriRuntime()) {
      return;
    }

    let cancelled = false;

    const loadDeadLetterFeed = async () => {
      setIsDeadLettersLoading(true);
      try {
        const nextDeadLetters = await listDeadLetters(
          workspacePath,
          deadLetterScope === 'selected' ? selectedWorkflow?.workflowId ?? null : null,
          160,
        );
        if (cancelled) {
          return;
        }
        setDeadLetters(nextDeadLetters);
        setDeadLetterError(null);
        setSelectedDeadLetterId((current) =>
          current && nextDeadLetters.some((entry) => entry.id === current)
            ? current
            : nextDeadLetters[0]?.id ?? null,
        );
      } catch (error) {
        if (cancelled) {
          return;
        }
        const { message } = describeUnknownError(error);
        setDeadLetterError(message);
      } finally {
        if (!cancelled) {
          setIsDeadLettersLoading(false);
        }
      }
    };

    void loadDeadLetterFeed();
    const timer = window.setInterval(() => {
      void loadDeadLetterFeed();
    }, 2500);

    return () => {
      cancelled = true;
      window.clearInterval(timer);
    };
  }, [deadLetterScope, selectedWorkflow?.workflowId, workspacePath]);

  const selectedDeadLetter = useMemo(
    () => deadLetters.find((entry) => entry.id === selectedDeadLetterId) ?? null,
    [deadLetters, selectedDeadLetterId],
  );

  const totalRetryCount = workflows.reduce(
    (sum, workflow) => sum + workflow.manualLane.retried + workflow.triggerLane.retried,
    0,
  );
  const totalDeadLetterCount = workflows.reduce(
    (sum, workflow) => sum + workflow.manualLane.deadLettered + workflow.triggerLane.deadLettered,
    0,
  );
  const totalQueuedCount = workflows.reduce(
    (sum, workflow) => sum + workflow.manualLane.depth + workflow.triggerLane.depth,
    0,
  );

  async function handleRefresh() {
    try {
      const [nextWorkflows, nextDeadLetters] = await Promise.all([
        listRuntimeWorkflows(),
        listDeadLetters(
          workspacePath,
          deadLetterScope === 'selected' ? selectedWorkflow?.workflowId ?? null : null,
          160,
        ),
      ]);
      setWorkflows(nextWorkflows);
      setDeadLetters(nextDeadLetters);
      onRuntimeCountChange?.(nextWorkflows.length);
      onStatusMessage('运行时管理已刷新。');
    } catch (error) {
      const { message } = describeUnknownError(error);
      onStatusMessage(`刷新运行时管理失败: ${message}`);
    }
  }

  async function handleActivateWorkflow(workflowId: string) {
    setPendingAction(`activate:${workflowId}`);
    try {
      const summary = await setActiveRuntimeWorkflow(workflowId);
      await onPersistActiveProject?.(summary.projectId?.trim() || summary.workflowId.trim() || null);
      setWorkflows((current) =>
        current.map((workflow) => ({
          ...workflow,
          active: workflow.workflowId === summary.workflowId,
        })),
      );
      setSelectedWorkflowId(summary.workflowId);
      onStatusMessage(`已切换当前工作流至 ${summary.projectName ?? summary.workflowId}。`);
    } catch (error) {
      const { message } = describeUnknownError(error);
      onStatusMessage(`切换当前工作流失败: ${message}`);
    } finally {
      setPendingAction(null);
    }
  }

  async function handleStopWorkflow(workflow: RuntimeWorkflowSummary) {
    setPendingAction(`stop:${workflow.workflowId}`);
    onBeforeWorkflowStop?.();
    try {
      const response = await undeployWorkflow(workflow.workflowId);
      const targetProjectId = workflow.projectId?.trim() || workflow.workflowId.trim();
      if (targetProjectId && onRemovePersistedDeployment) {
        await onRemovePersistedDeployment(targetProjectId);
      }

      const nextWorkflows = await listRuntimeWorkflows();
      const nextActiveWorkflow = nextWorkflows.find((item) => item.active) ?? null;
      await onPersistActiveProject?.(
        nextActiveWorkflow?.projectId?.trim() || nextActiveWorkflow?.workflowId.trim() || null,
      );
      setWorkflows(nextWorkflows);
      setSelectedWorkflowId((current) => {
        if (current && nextWorkflows.some((workflow) => workflow.workflowId === current)) {
          return current;
        }
        return nextWorkflows.find((workflow) => workflow.active)?.workflowId ?? nextWorkflows[0]?.workflowId ?? null;
      });
      onRuntimeCountChange?.(nextWorkflows.length);
      onStatusMessage(
        response.hadWorkflow
          ? `已停止 ${workflow.projectName ?? workflow.workflowId}，共中止 ${response.abortedTimerCount} 个触发任务。`
          : `未找到运行中的 ${workflow.projectName ?? workflow.workflowId}。`,
      );
    } catch (error) {
      const { message } = describeUnknownError(error);
      onStatusMessage(`停止工作流失败: ${message}`);
    } finally {
      onAfterWorkflowStop?.();
      setPendingAction(null);
    }
  }

  async function handleOpenWorkflowProject(workflow: RuntimeWorkflowSummary) {
    if (!workflow.projectId) {
      return;
    }

    setPendingAction(`activate:${workflow.workflowId}`);
    try {
      const summary = await setActiveRuntimeWorkflow(workflow.workflowId);
      await onPersistActiveProject?.(summary.projectId?.trim() || summary.workflowId.trim() || null);
      setWorkflows((current) =>
        current.map((item) => ({
          ...item,
          active: item.workflowId === workflow.workflowId,
        })),
      );
      setSelectedWorkflowId(workflow.workflowId);
    } catch (error) {
      const { message } = describeUnknownError(error);
      onStatusMessage(`切换当前工作流失败: ${message}`);
      setPendingAction(null);
      return;
    } finally {
      setPendingAction(null);
    }

    onOpenBoard(workflow.projectId);
  }

  async function handleCopyDeadLetterPayload() {
    if (!selectedDeadLetter) {
      return;
    }

    const copied = await copyText(JSON.stringify(selectedDeadLetter.payload, null, 2));
    if (!copied) {
      return;
    }

    setHasCopiedPayload(true);
    window.setTimeout(() => setHasCopiedPayload(false), 1200);
  }

  const jsonTheme = themeMode === 'dark' ? darkStyles : defaultStyles;

  return (
    <div className="runtime-manager">
      <div
        className="panel__header panel__header--desktop window-safe-header"
        data-window-drag-region
      >
        <div className="panel__header__heading">
          <h2>运行时管理</h2>
        </div>

        <div className="panel__header-metrics">
          <span className="panel__header-metric">
            <strong>{workflows.length}</strong> 在线
          </span>
          <span className="panel__header-metric">
            <strong>{totalQueuedCount}</strong> 积压
          </span>
          <span className="panel__header-metric">
            <strong>{totalRetryCount}</strong> 重试
          </span>
          <span className="panel__header-metric">
            <strong>{totalDeadLetterCount}</strong> 死信
          </span>
        </div>

        <div className="panel__header-actions" data-no-window-drag>
          <button
            type="button"
            className="panel__icon-button"
            onClick={() => void handleRefresh()}
            disabled={isLoading || isDeadLettersLoading}
            aria-label="刷新运行时管理"
            title="刷新"
          >
            <HistoryIcon />
            <span>刷新</span>
          </button>
        </div>
      </div>

      <div className="runtime-manager__workspace">
        <aside className="runtime-manager__lane runtime-manager__lane--list" aria-label="运行中的工作流">
          <div className="runtime-manager__section-head">
            <strong>在线编队</strong>
            <span>{isLoading ? '同步中' : runtimeError ? '读取异常' : `${workflows.length} 个`}</span>
          </div>

          <div className="runtime-manager__workflow-list">
            {workflows.length === 0 ? (
              <div className="runtime-manager__empty" data-testid="runtime-empty-state">
                <strong>当前没有在线工作流</strong>
                <span>部署工程后，运行实例会出现在这里。</span>
              </div>
            ) : (
              workflows.map((workflow) => {
                const isSelected = workflow.workflowId === selectedWorkflowId;
                const isOwnedByBoard = activeBoardId === workflow.projectId;
                const queuePressure = workflow.manualLane.depth + workflow.triggerLane.depth;

                return (
                  <button
                    key={workflow.workflowId}
                    type="button"
                    className={
                      isSelected
                        ? 'runtime-manager__workflow-row is-selected'
                        : 'runtime-manager__workflow-row'
                    }
                    data-testid="runtime-workflow-item"
                    onClick={() => setSelectedWorkflowId(workflow.workflowId)}
                  >
                    <div className="runtime-manager__workflow-row-top">
                      <strong>{workflow.projectName ?? workflow.workflowId}</strong>
                      <span
                        className={
                          workflow.active
                            ? 'runtime-manager__workflow-badge is-active'
                            : 'runtime-manager__workflow-badge'
                        }
                      >
                        {workflow.active ? '当前' : '在线'}
                      </span>
                    </div>

                    <div className="runtime-manager__workflow-row-meta">
                      <span>{workflow.environmentName ?? '默认环境'}</span>
                      <span>{formatRelativeStamp(workflow.deployedAt)}</span>
                      <span>{workflow.nodeCount} 节点</span>
                      {queuePressure > 0 ? <span>{queuePressure} 积压</span> : null}
                      {isOwnedByBoard ? <span>当前工程</span> : null}
                    </div>
                  </button>
                );
              })
            )}
          </div>
        </aside>

        <div className="runtime-manager__right">
          <section className="runtime-manager__lane runtime-manager__lane--detail" aria-label="工作流详情">
            <div className="runtime-manager__section-head">
              <strong>生命周期</strong>
              <span>
                {selectedWorkflow ? selectedWorkflow.workflowId.slice(0, 8) : '等待选择'}
              </span>
              {selectedWorkflow && (
                <div className="runtime-manager__section-actions" data-no-window-drag>
                  {!selectedWorkflow.active ? (
                    <button
                      type="button"
                      className="runtime-manager__action-button"
                      onClick={() => void handleActivateWorkflow(selectedWorkflow.workflowId)}
                      disabled={pendingAction === `activate:${selectedWorkflow.workflowId}`}
                    >
                      <RunActionIcon />
                      <span>设为当前</span>
                    </button>
                  ) : null}

                  {selectedWorkflow.projectId ? (
                    <button
                      type="button"
                      className="runtime-manager__action-button"
                      onClick={() => void handleOpenWorkflowProject(selectedWorkflow)}
                    >
                      <InspectIcon />
                      <span>进入工程</span>
                    </button>
                  ) : null}

                  <button
                    type="button"
                    className="runtime-manager__action-button runtime-manager__action-button--danger"
                    onClick={() => void handleStopWorkflow(selectedWorkflow)}
                    disabled={pendingAction === `stop:${selectedWorkflow.workflowId}`}
                  >
                    <StopActionIcon />
                    <span>停止</span>
                  </button>
                </div>
              )}
            </div>

            {selectedWorkflow ? (
              <div className="runtime-manager__detail">
                <div className="runtime-manager__queue-grid">
                  <article className="runtime-manager__queue-block">
                    <div className="runtime-manager__queue-head">
                      <strong>手动通道</strong>
                      <span>{selectedWorkflow.policy.manualBackpressureStrategy}</span>
                    </div>
                    <div className="runtime-manager__queue-stats">
                      <span>深度 {selectedWorkflow.manualLane.depth}</span>
                      <span>接收 {selectedWorkflow.manualLane.accepted}</span>
                      <span>重试 {selectedWorkflow.manualLane.retried}</span>
                      <span>死信 {selectedWorkflow.manualLane.deadLettered}</span>
                    </div>
                    <div className="runtime-manager__queue-rail">
                      <span
                        className="runtime-manager__queue-fill"
                        style={{
                          width: `${Math.min(
                            100,
                            (selectedWorkflow.manualLane.depth /
                              Math.max(1, selectedWorkflow.policy.manualQueueCapacity)) *
                              100,
                          )}%`,
                        }}
                      />
                    </div>
                  </article>

                  <article className="runtime-manager__queue-block">
                    <div className="runtime-manager__queue-head">
                      <strong>触发通道</strong>
                      <span>{selectedWorkflow.policy.triggerBackpressureStrategy}</span>
                    </div>
                    <div className="runtime-manager__queue-stats">
                      <span>深度 {selectedWorkflow.triggerLane.depth}</span>
                      <span>接收 {selectedWorkflow.triggerLane.accepted}</span>
                      <span>重试 {selectedWorkflow.triggerLane.retried}</span>
                      <span>死信 {selectedWorkflow.triggerLane.deadLettered}</span>
                    </div>
                    <div className="runtime-manager__queue-rail">
                      <span
                        className="runtime-manager__queue-fill is-trigger"
                        style={{
                          width: `${Math.min(
                            100,
                            (selectedWorkflow.triggerLane.depth /
                              Math.max(1, selectedWorkflow.policy.triggerQueueCapacity)) *
                              100,
                          )}%`,
                        }}
                      />
                    </div>
                  </article>
                </div>

                <div className="runtime-manager__detail-grid">
                  <section className="runtime-manager__detail-block">
                    <div className="runtime-manager__detail-block-title">调度策略</div>
                    <div className="runtime-manager__kv-list">
                      <div className="runtime-manager__kv-row">
                        <span>手动容量</span>
                        <strong>{selectedWorkflow.policy.manualQueueCapacity}</strong>
                      </div>
                      <div className="runtime-manager__kv-row">
                        <span>触发容量</span>
                        <strong>{selectedWorkflow.policy.triggerQueueCapacity}</strong>
                      </div>
                      <div className="runtime-manager__kv-row">
                        <span>最大重试</span>
                        <strong>{selectedWorkflow.policy.maxRetryAttempts}</strong>
                      </div>
                      <div className="runtime-manager__kv-row">
                        <span>初始退避</span>
                        <strong>{formatDuration(selectedWorkflow.policy.initialRetryBackoffMs)}</strong>
                      </div>
                      <div className="runtime-manager__kv-row">
                        <span>最大退避</span>
                        <strong>{formatDuration(selectedWorkflow.policy.maxRetryBackoffMs)}</strong>
                      </div>
                    </div>
                  </section>

                  <section className="runtime-manager__detail-block">
                    <div className="runtime-manager__detail-block-title">根节点</div>
                    <div className="runtime-manager__chip-row">
                      {selectedWorkflow.rootNodes.map((nodeId) => (
                        <span key={nodeId} className="runtime-manager__chip">
                          {nodeId}
                        </span>
                      ))}
                    </div>
                  </section>
                </div>
              </div>
            ) : (
              <div className="runtime-manager__empty runtime-manager__empty--detail">
                <strong>选择左侧工作流</strong>
                <span>查看队列状态与调度策略。</span>
              </div>
            )}
          </section>

          <aside className="runtime-manager__lane runtime-manager__lane--dead" aria-label="死信留存">
            <div className="runtime-manager__section-head">
              <strong>Dead Letter</strong>
              <div className="runtime-manager__section-actions" data-no-window-drag>
                <button
                  type="button"
                  className={
                    deadLetterScope === 'selected'
                      ? 'runtime-manager__scope-toggle is-active'
                      : 'runtime-manager__scope-toggle'
                  }
                  onClick={() => setDeadLetterScope('selected')}
                >
                  当前
                </button>
                <button
                  type="button"
                  className={
                    deadLetterScope === 'all'
                      ? 'runtime-manager__scope-toggle is-active'
                      : 'runtime-manager__scope-toggle'
                  }
                  onClick={() => setDeadLetterScope('all')}
                >
                  全部
                </button>
              </div>
            </div>

            <div className="runtime-manager__dead-layout">
              <div className="runtime-manager__dead-list">
                {deadLetters.length === 0 ? (
                  <div className="runtime-manager__empty runtime-manager__empty--dead">
                    <strong>{isDeadLettersLoading ? '读取中' : '暂无死信'}</strong>
                    <span>{deadLetterError ? deadLetterError : '重试耗尽的消息会出现在这里。'}</span>
                  </div>
                ) : (
                  deadLetters.map((entry) => (
                    <button
                      key={entry.id}
                      type="button"
                      className={
                        entry.id === selectedDeadLetterId
                          ? 'runtime-manager__dead-row is-selected'
                          : 'runtime-manager__dead-row'
                      }
                      data-testid="runtime-dead-letter-item"
                      onClick={() => setSelectedDeadLetterId(entry.id)}
                    >
                      <div className="runtime-manager__dead-row-top">
                        <strong>{entry.projectName ?? entry.workflowId}</strong>
                        <span>{formatLogTimestamp(Date.parse(entry.timestamp) || Date.now())}</span>
                      </div>
                      <div className="runtime-manager__dead-row-reason">{entry.reason}</div>
                    </button>
                  ))
                )}
              </div>

              <div className="runtime-manager__dead-detail">
                {selectedDeadLetter ? (
                  <>
                    <div className="runtime-manager__dead-detail-head">
                      <div>
                        <strong>{selectedDeadLetter.projectName ?? selectedDeadLetter.workflowId}</strong>
                        <span>{selectedDeadLetter.reason}</span>
                      </div>

                      <button
                        type="button"
                        className={
                          hasCopiedPayload
                            ? 'runtime-manager__icon-button is-active'
                            : 'runtime-manager__icon-button'
                        }
                        onClick={() => void handleCopyDeadLetterPayload()}
                        aria-label="复制死信载荷"
                        title="复制 JSON"
                      >
                        <CopyIcon />
                      </button>
                    </div>

                    <div className="runtime-manager__kv-list">
                      <div className="runtime-manager__kv-row">
                        <span>Trace</span>
                        <strong>{selectedDeadLetter.traceId}</strong>
                      </div>
                      <div className="runtime-manager__kv-row">
                        <span>目标节点</span>
                        <strong>{selectedDeadLetter.targetNodeId ?? 'workflow-root'}</strong>
                      </div>
                      <div className="runtime-manager__kv-row">
                        <span>重试</span>
                        <strong>{selectedDeadLetter.attempts} 次</strong>
                      </div>
                    </div>

                    <div className="runtime-manager__json-shell">
                      <JsonView
                        data={normalizeJsonTree(selectedDeadLetter.payload)}
                        shouldExpandNode={collapseAllNested}
                        style={jsonTheme}
                      />
                    </div>
                  </>
                ) : (
                  <div className="runtime-manager__empty runtime-manager__empty--dead-detail">
                    <strong>选择一条死信记录</strong>
                    <span>查看失败原因和完整 payload。</span>
                  </div>
                )}
              </div>
            </div>
          </aside>
        </div>
      </div>
    </div>
  );
}
