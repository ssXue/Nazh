import { startTransition, useMemo, useRef, useState, type RefObject } from 'react';

import type { BoardWorkspaceHandle } from '../components/app/BoardWorkspace';
import {
  buildRuntimeConsoleEntries,
  formatLogTimestamp,
  type RuntimeConsoleEntry,
} from '../components/app/runtime-console';
import { formatWorkflowGraph } from '../lib/flowgram';
import { parseWorkflowGraph } from '../lib/graph';
import type { ProjectRecord } from '../lib/projects';
import {
  createEmptyWorkflowDraft,
  createWorkflowOrchestrationState,
  getWorkflowAiUnavailableReason,
  resolvePreferredWorkflowAiProvider,
  streamWorkflowOrchestration,
  type WorkflowOrchestrationMode,
  type WorkflowOrchestrationSessionState,
} from '../lib/workflow-orchestrator';
import { describeUnknownError } from '../lib/workflow-events';
import { hasTauriRuntime, loadAiAssetContext } from '../lib/tauri';
import type { AiConfigView, AppErrorRecord, RuntimeLogEntry, WorkflowRuntimeState } from '../types';

const MAX_RUNTIME_ERROR_CONTEXT_ENTRIES = 8;

interface UseAiWorkflowComposerStateOptions {
  activeBoardId: string | null;
  activeProject: ProjectRecord | null;
  activeWorkflowId: string | null;
  aiConfig: AiConfigView | null;
  appErrors: AppErrorRecord[];
  appendAppError: (scope: 'workflow' | 'command' | 'frontend' | 'runtime', title: string, detail?: string | null) => void;
  appendRuntimeLog: (source: string, level: 'info' | 'success' | 'warn' | 'error', message: string, detail?: string | null) => void;
  createProject: (name?: string, description?: string) => ProjectRecord;
  eventFeed: RuntimeLogEntry[];
  flowgramCanvasRef: RefObject<BoardWorkspaceHandle | null>;
  openBoard: (boardId: string) => void;
  projectCount: number;
  resetWorkspaceRuntime: (nextMessage: string) => void;
  runtimeState: WorkflowRuntimeState;
  setStatusMessage: (message: string) => void;
  workspacePath: string;
  updateProjectDraft: (
    projectId: string,
    nextDraft: Partial<Pick<ProjectRecord, 'astText' | 'payloadText' | 'name' | 'description'>>,
  ) => void;
}

async function loadWorkflowAssetContext(
  workspacePath: string,
  appendRuntimeLog: UseAiWorkflowComposerStateOptions['appendRuntimeLog'],
) {
  if (!hasTauriRuntime()) {
    return null;
  }

  try {
    const context = await loadAiAssetContext(workspacePath);
    if (context.devices.length > 0 || context.capabilities.length > 0) {
      appendRuntimeLog(
        'ai',
        'info',
        '已加载 AI 设备资产上下文',
        `${context.devices.length} 个设备 · ${context.capabilities.length} 个能力`,
      );
    }
    return context;
  } catch (error) {
    appendRuntimeLog('ai', 'warn', 'AI 设备资产上下文加载失败', String(error));
    return null;
  }
}

function consoleEntryNodeId(entry: RuntimeConsoleEntry): string | null {
  if (entry.nodeId?.trim()) {
    return entry.nodeId.trim();
  }

  if (entry.channel === 'event' && entry.source.trim()) {
    return entry.source.trim();
  }

  return null;
}

function buildRuntimeErrorContextText(options: {
  activeWorkflowId: string | null;
  appErrors: AppErrorRecord[];
  eventFeed: RuntimeLogEntry[];
  runtimeState: WorkflowRuntimeState;
}): string | null {
  const consoleEntries = buildRuntimeConsoleEntries(options.eventFeed, options.appErrors);
  const errorEntries = consoleEntries
    .filter((entry) => entry.level === 'error')
    .slice(-MAX_RUNTIME_ERROR_CONTEXT_ENTRIES);

  if (
    errorEntries.length === 0 &&
    !options.runtimeState.lastError &&
    options.runtimeState.failedNodeIds.length === 0
  ) {
    return null;
  }

  const stateLines = [
    `activeWorkflowId: ${options.activeWorkflowId ?? 'null'}`,
    `traceId: ${options.runtimeState.traceId ?? 'null'}`,
    `lastNodeId: ${options.runtimeState.lastNodeId ?? 'null'}`,
    `lastEventType: ${options.runtimeState.lastEventType ?? 'null'}`,
    `failedNodeIds: ${
      options.runtimeState.failedNodeIds.length > 0
        ? options.runtimeState.failedNodeIds.join(', ')
        : '[]'
    }`,
  ];

  if (options.runtimeState.lastError) {
    stateLines.push(`lastError: ${options.runtimeState.lastError}`);
  }

  const errorLines = errorEntries.map((entry) => {
    const nodeId = consoleEntryNodeId(entry);
    const detail = entry.detail?.trim();
    return [
      `- [${formatLogTimestamp(entry.timestamp)}] source=${entry.source}`,
      nodeId ? `nodeId=${nodeId}` : null,
      entry.scope ? `scope=${entry.scope}` : null,
      `message=${entry.message}`,
      detail ? `detail=${detail}` : null,
    ]
      .filter((segment): segment is string => Boolean(segment))
      .join(' ');
  });

  return [
    '运行状态：',
    ...stateLines,
    '最近错误：',
    ...(errorLines.length > 0 ? errorLines : ['- Runtime Dock 暂无错误行，仅有运行状态失败标记。']),
  ].join('\n');
}

export function useAiWorkflowComposerState({
  activeBoardId,
  activeProject,
  activeWorkflowId,
  aiConfig,
  appErrors,
  appendAppError,
  appendRuntimeLog,
  createProject,
  eventFeed,
  flowgramCanvasRef,
  openBoard,
  projectCount,
  resetWorkspaceRuntime,
  runtimeState,
  setStatusMessage,
  workspacePath,
  updateProjectDraft,
}: UseAiWorkflowComposerStateOptions) {
  const [open, setOpen] = useState(false);
  const [mode, setMode] = useState<WorkflowOrchestrationMode>('create');
  const [requirement, setRequirement] = useState('');
  const [status, setStatus] = useState<'idle' | 'generating' | 'completed' | 'interrupted'>('idle');
  const [error, setError] = useState<string | null>(null);
  const [rawText, setRawText] = useState<string | null>(null);
  const [thinkingText, setThinkingText] = useState<string | null>(null);
  const [sessionState, setSessionState] =
    useState<WorkflowOrchestrationSessionState | null>(null);
  const targetProjectIdRef = useRef<string | null>(null);
  const sessionIdRef = useRef(0);
  const generating = status === 'generating';
  const preferredWorkflowAiProvider = useMemo(
    () => resolvePreferredWorkflowAiProvider(aiConfig),
    [aiConfig],
  );
  const actionTitle = useMemo(() => {
    if (generating) {
      return mode === 'create' ? 'AI 正在流式编排新工作流' : 'AI 正在流式编辑当前工作流';
    }

    return getWorkflowAiUnavailableReason(aiConfig);
  }, [generating, mode, aiConfig]);
  const runtimeErrorContext = useMemo(
    () =>
      buildRuntimeErrorContextText({
        activeWorkflowId,
        appErrors,
        eventFeed,
        runtimeState,
      }),
    [activeWorkflowId, appErrors, eventFeed, runtimeState],
  );

  function buildDraftFromProject(project: ProjectRecord) {
    const parsedGraphState = parseWorkflowGraph(project.astText);
    const currentGraph =
      project.id === activeBoardId
        ? flowgramCanvasRef.current?.getCurrentWorkflowGraph() ?? parsedGraphState.graph
        : parsedGraphState.graph;

    return {
      name: project.name,
      description: project.description,
      payloadText: project.payloadText,
      graph: currentGraph ?? createEmptyWorkflowDraft(project.name).graph,
    };
  }

  function resetState(
    nextMode: WorkflowOrchestrationMode,
    nextRequirement = '',
    nextSessionState: WorkflowOrchestrationSessionState | null = null,
  ) {
    setMode(nextMode);
    setRequirement(nextRequirement);
    setStatus('idle');
    setError(null);
    setRawText(null);
    setThinkingText(null);
    setSessionState(nextSessionState);
  }

  function openEdit() {
    if (!activeProject) {
      return;
    }

    resetState('edit', '', createWorkflowOrchestrationState(buildDraftFromProject(activeProject)));
    setOpen(true);
  }

  function close() {
    if (generating) {
      return;
    }

    setOpen(false);
    setStatus('idle');
    setError(null);
    setRawText(null);
    setThinkingText(null);
    setSessionState(null);
    targetProjectIdRef.current = null;
  }

  function ensureTargetProject(
    nextMode: WorkflowOrchestrationMode,
    nextDraft: WorkflowOrchestrationSessionState['draft'],
  ) {
    if (nextMode === 'edit') {
      return activeProject?.id ?? targetProjectIdRef.current;
    }

    if (targetProjectIdRef.current) {
      return targetProjectIdRef.current;
    }

    const nextProject = createProject(
      nextDraft.name.trim() || `AI 编排草稿 ${projectCount + 1}`,
      nextDraft.description.trim() || 'AI 正在编排工作流。',
    );
    targetProjectIdRef.current = nextProject.id;
    openBoard(nextProject.id);
    resetWorkspaceRuntime(`AI 正在编排工程 ${nextProject.name}。`);
    appendRuntimeLog('ai', 'info', '已创建 AI 编排草稿工程', nextProject.name);
    return nextProject.id;
  }

  function applyDraft(
    nextMode: WorkflowOrchestrationMode,
    nextState: WorkflowOrchestrationSessionState,
  ) {
    const projectId = ensureTargetProject(nextMode, nextState.draft);
    if (!projectId) {
      return;
    }

    startTransition(() => {
      updateProjectDraft(projectId, {
        name: nextState.draft.name,
        description: nextState.draft.description,
        payloadText: nextState.draft.payloadText,
        astText: formatWorkflowGraph(nextState.draft.graph),
      });
    });
  }

  async function submit() {
    if (!preferredWorkflowAiProvider) {
      setError(actionTitle);
      return;
    }

    const trimmedRequirement = requirement.trim();
    if (!trimmedRequirement) {
      setError('请输入编排需求。');
      return;
    }

    const currentMode = mode;
    const baseDraft =
      currentMode === 'edit' && activeProject ? buildDraftFromProject(activeProject) : null;
    const initialSessionState = createWorkflowOrchestrationState(baseDraft);
    const sessionId = sessionIdRef.current + 1;
    sessionIdRef.current = sessionId;
    targetProjectIdRef.current = currentMode === 'edit' ? activeProject?.id ?? null : null;

    setStatus('generating');
    setError(null);
    setRawText(null);
    setThinkingText(null);
    setSessionState(initialSessionState);
    appendRuntimeLog(
      'ai',
      'info',
      currentMode === 'create' ? '开始 AI 流式编排工作流' : '开始 AI 流式编辑工作流',
      trimmedRequirement,
    );
    setStatusMessage(
      currentMode === 'create' ? 'AI 正在流式编排新工作流。' : 'AI 正在流式编辑当前工作流。',
    );

    try {
      const assetContext = await loadWorkflowAssetContext(workspacePath, appendRuntimeLog);
      const finalState = await streamWorkflowOrchestration({
        mode: currentMode,
        requirement: trimmedRequirement,
        providerId: preferredWorkflowAiProvider.id,
        model: preferredWorkflowAiProvider.defaultModel,
        baseDraft,
        assetContext,
        runtimeErrorContext: currentMode === 'edit' ? runtimeErrorContext : null,
        params: aiConfig?.copilotParams,
        timeoutMs: aiConfig?.agentSettings.timeoutMs,
        onRawText: (nextRawText) => {
          if (sessionIdRef.current !== sessionId) {
            return;
          }

          setRawText(nextRawText);
        },
        onThinking: (nextThinkingText) => {
          if (sessionIdRef.current !== sessionId) {
            return;
          }

          setThinkingText(nextThinkingText);
        },
        onOperation: (operation, nextState) => {
          if (sessionIdRef.current !== sessionId) {
            return;
          }

          startTransition(() => {
            setSessionState(nextState);
            applyDraft(currentMode, nextState);
          });

          if (operation.type === 'done') {
            appendRuntimeLog(
              'ai',
              'success',
              currentMode === 'create' ? 'AI 编排完成' : 'AI 编辑完成',
              nextState.summary ?? nextState.draft.name,
            );
            setStatusMessage(
              currentMode === 'create'
                ? `AI 已完成工程 ${nextState.draft.name} 的流式编排。`
                : `AI 已完成工程 ${nextState.draft.name} 的流式编辑。`,
            );
          }
        },
        onRetry: (attempt, retryError, retryState, strategy) => {
          if (sessionIdRef.current !== sessionId) {
            return;
          }

          setError(null);
          startTransition(() => {
            setSessionState(retryState);
            if (strategy === 'restart' && (currentMode === 'edit' || targetProjectIdRef.current)) {
              applyDraft(currentMode, retryState);
            }
          });
          appendRuntimeLog(
            'ai',
            'warn',
            strategy === 'retry'
              ? `AI 输出波动，正在原位重试（第 ${attempt} 次）`
              : strategy === 'resume'
                ? `AI 输出中断，正在断点续传（第 ${attempt} 次）`
                : `AI 输出中断，正在重新开始（第 ${attempt} 次）`,
            retryError.message,
          );
          setStatusMessage(
            strategy === 'retry'
              ? currentMode === 'create'
                ? 'AI 输出波动，正在重试当前编排请求。'
                : 'AI 输出波动，正在重试当前编辑请求。'
              : strategy === 'resume'
                ? currentMode === 'create'
                  ? 'AI 输出中断，正在断点续传编排。'
                  : 'AI 输出中断，正在断点续传编辑。'
                : currentMode === 'create'
                  ? 'AI 输出中断，正在重新开始编排。'
                  : 'AI 输出中断，正在重新开始编辑。',
          );
        },
      });

      if (sessionIdRef.current !== sessionId) {
        return;
      }

      setSessionState(finalState);
      if (currentMode === 'edit' || finalState.operations.length > 0) {
        applyDraft(currentMode, finalState);
      }
      setStatus('completed');
    } catch (submitError) {
      if (sessionIdRef.current !== sessionId) {
        return;
      }

      const { message, detail } = describeUnknownError(submitError);
      const composedDetail = detail ? `${message}\n\n${detail}` : message;
      const targetProjectId = targetProjectIdRef.current;
      setStatus('interrupted');
      setError(message);
      appendAppError('command', 'AI 编排失败', composedDetail);
      appendRuntimeLog(
        'ai',
        'error',
        currentMode === 'create' ? 'AI 编排失败' : 'AI 编辑失败',
        targetProjectId ? `${message}\n已保留当前部分草稿。` : message,
      );
      setStatusMessage(targetProjectId ? 'AI 中断，已保留当前部分编排草稿。' : message);
    }
  }

  return {
    actionDisabled: !preferredWorkflowAiProvider || generating,
    actionTitle,
    composerProps: {
      open,
      mode,
      activeProjectName: activeProject?.name ?? null,
      status,
      generating,
      requirement,
      error,
      rawText,
      thinkingText,
      draft: sessionState?.draft ?? null,
      operations: sessionState?.operations ?? [],
      onRequirementChange: setRequirement,
      onClose: close,
      onSubmit: () => {
        void submit();
      },
    },
    generating,
    mode,
    openEdit,
  };
}
