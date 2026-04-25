import { startTransition, useMemo, useRef, useState, type RefObject } from 'react';

import type { BoardWorkspaceHandle } from '../components/app/BoardWorkspace';
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
import type { AiConfigView } from '../types';

interface UseAiWorkflowComposerStateOptions {
  activeBoardId: string | null;
  activeProject: ProjectRecord | null;
  aiConfig: AiConfigView | null;
  appendAppError: (scope: 'workflow' | 'command' | 'frontend' | 'runtime', title: string, detail?: string | null) => void;
  appendRuntimeLog: (source: string, level: 'info' | 'success' | 'warn' | 'error', message: string, detail?: string | null) => void;
  createProject: (name?: string, description?: string) => ProjectRecord;
  flowgramCanvasRef: RefObject<BoardWorkspaceHandle>;
  openBoard: (boardId: string) => void;
  projectCount: number;
  resetWorkspaceRuntime: (nextMessage: string) => void;
  setStatusMessage: (message: string) => void;
  updateProjectDraft: (
    projectId: string,
    nextDraft: Partial<Pick<ProjectRecord, 'astText' | 'payloadText' | 'name' | 'description'>>,
  ) => void;
}

export function useAiWorkflowComposerState({
  activeBoardId,
  activeProject,
  aiConfig,
  appendAppError,
  appendRuntimeLog,
  createProject,
  flowgramCanvasRef,
  openBoard,
  projectCount,
  resetWorkspaceRuntime,
  setStatusMessage,
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

  function openCreate() {
    resetState('create');
    setOpen(true);
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
      const finalState = await streamWorkflowOrchestration({
        mode: currentMode,
        requirement: trimmedRequirement,
        providerId: preferredWorkflowAiProvider.id,
        model: preferredWorkflowAiProvider.defaultModel,
        baseDraft,
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
    openCreate,
    openEdit,
  };
}
