import { startTransition, useEffect, useMemo, useRef, useState } from 'react';

import { useAiConfigState } from './hooks/use-ai-config-state';
import { useAppNavigation } from './hooks/use-app-navigation';
import { useDeploymentRestore } from './hooks/use-deployment-restore';
import { useProjectLibrary } from './hooks/use-project-library';
import { useSettings } from './hooks/use-settings';
import { useWorkflowEngine } from './hooks/use-workflow-engine';
import { useConnectionLibrary } from './hooks/use-connection-library';

import { AboutPanel } from './components/app/AboutPanel';
import { AiConfigPanel } from './components/app/AiConfigPanel';
import { AiWorkflowComposer } from './components/app/AiWorkflowComposer';
import { BoardWorkspace, type BoardWorkspaceHandle } from './components/app/BoardWorkspace';
import { BoardsPanel, type BoardItem } from './components/app/BoardsPanel';
import { DashboardPanel } from './components/app/DashboardPanel';
import { LogsPanel } from './components/app/LogsPanel';
import { PayloadPanel } from './components/app/PayloadPanel';
import { PluginPanel } from './components/app/PluginPanel';
import { RestoreDeploymentDialog } from './components/app/RestoreDeploymentDialog';
import { RuntimeManagerPanel } from './components/app/RuntimeManagerPanel';
import { SettingsPanel } from './components/app/SettingsPanel';
import { SidebarNav } from './components/app/SidebarNav';
import { ConnectionStudio } from './components/ConnectionStudio';
import { parseWorkflowGraph } from './lib/graph';
import { formatWorkflowGraph } from './lib/flowgram';
import {
  applyEnvironmentToConnectionDefinitions,
  CURRENT_USER_NAME,
  formatRelativeTimestamp,
  getActiveEnvironment,
  parseProjectNodeCount,
  type ProjectRecord,
  type ProjectEnvironmentDiff,
} from './lib/projects';
import { buildSidebarSections } from './lib/sidebar';
import { ACCENT_PRESET_OPTIONS } from './lib/theme';
import {
  applyGlobalAiConfigToWorkflowGraph,
  stripWorkflowNodeLocalAiConfig,
} from './lib/workflow-ai';
import {
  createEmptyWorkflowDraft,
  createWorkflowOrchestrationState,
  getWorkflowAiUnavailableReason,
  resolvePreferredWorkflowAiProvider,
  streamWorkflowOrchestration,
  type WorkflowOrchestrationMode,
  type WorkflowOrchestrationSessionState,
} from './lib/workflow-orchestrator';
import {
  dispatchPayload,
  hasTauriRuntime,
  listRuntimeWorkflows,
  undeployWorkflow,
} from './lib/tauri';
import type { ConnectionDefinition, WorkflowResult, WorkflowNodeDefinition } from './types';
import { describeUnknownError } from './lib/workflow-events';
import {
  deriveWorkflowStatus,
  getWorkflowStatusLabel,
  getWorkflowStatusPillClass,
} from './lib/workflow-status';

interface DeploymentSnapshot {
  projectId: string;
  projectName: string;
  environmentId: string;
  environmentName: string;
  astText: string;
  runtimeAstText: string;
  runtimeConnections: ConnectionDefinition[];
}

interface ConnectionUsageSummary {
  nodeIds: string[];
  projectNames: string[];
}

function getDeployProjectId(
  deployInfo: { projectId?: string | null; workflowId?: string | null } | null,
) {
  if (!deployInfo) {
    return null;
  }

  return deployInfo.projectId?.trim() || deployInfo.workflowId?.trim() || null;
}

function buildConnectionUsageMap(
  projects: Array<{ name: string; astText: string }>,
): Map<string, ConnectionUsageSummary> {
  const usage = new Map<string, { nodeIds: Set<string>; projectNames: Set<string> }>();

  for (const project of projects) {
    const parsed = parseWorkflowGraph(project.astText);
    if (!parsed.graph) {
      continue;
    }

    for (const [nodeId, node] of Object.entries(parsed.graph.nodes)) {
      const connectionId = node.connection_id?.trim();
      if (!connectionId) {
        continue;
      }

      const current = usage.get(connectionId) ?? {
        nodeIds: new Set<string>(),
        projectNames: new Set<string>(),
      };
      current.nodeIds.add(nodeId);
      current.projectNames.add(project.name);
      usage.set(connectionId, current);
    }
  }

  return new Map(
    [...usage.entries()].map(([connectionId, summary]) => [
      connectionId,
      {
        nodeIds: [...summary.nodeIds],
        projectNames: [...summary.projectNames],
      },
    ]),
  );
}

function getReferencedConnectionCount(nodes: Record<string, WorkflowNodeDefinition> | null): number {
  if (!nodes) {
    return 0;
  }

  return new Set(
    Object.values(nodes)
      .map((node) => node.connection_id?.trim())
      .filter((connectionId): connectionId is string => Boolean(connectionId)),
  ).size;
}

function collectLegacyProjectConnections(
  projects: Array<{ astText: string }>,
  currentConnections: ConnectionDefinition[],
): ConnectionDefinition[] {
  const knownIds = new Set(currentConnections.map((connection) => connection.id));
  const migratedConnections: ConnectionDefinition[] = [];

  for (const project of projects) {
    const parsed = parseWorkflowGraph(project.astText);
    const legacyConnections = parsed.graph?.connections ?? [];
    for (const connection of legacyConnections) {
      if (knownIds.has(connection.id)) {
        continue;
      }

      knownIds.add(connection.id);
      migratedConnections.push(connection);
    }
  }

  return migratedConnections;
}

function collectLegacyProjectAstUpdates(
  projects: Array<{ id: string; astText: string }>,
): Array<{ projectId: string; astText: string }> {
  const updates: Array<{ projectId: string; astText: string }> = [];

  for (const project of projects) {
    const parsed = parseWorkflowGraph(project.astText);
    if (!parsed.graph || (parsed.graph.connections?.length ?? 0) === 0) {
      continue;
    }

    updates.push({
      projectId: project.id,
      astText: formatWorkflowGraph({
        ...parsed.graph,
        connections: [],
      }),
    });
  }

  return updates;
}

function App() {
  const settings = useSettings();
  const projectLibrary = useProjectLibrary(settings.projectWorkspacePath);
  const connectionLibrary = useConnectionLibrary(settings.projectWorkspacePath);
  const {
    aiConfig,
    aiConfigError,
    aiConfigLoading,
    aiTesting,
    aiTestResult,
    handleAiConfigSave,
    handleAiProviderTest,
  } = useAiConfigState();
  const [runtimeWorkflowCount, setRuntimeWorkflowCount] = useState(0);
  const [aiComposerOpen, setAiComposerOpen] = useState(false);
  const [aiComposerMode, setAiComposerMode] = useState<WorkflowOrchestrationMode>('create');
  const [aiComposerRequirement, setAiComposerRequirement] = useState('');
  const [aiComposerStatus, setAiComposerStatus] = useState<
    'idle' | 'generating' | 'completed' | 'interrupted'
  >('idle');
  const [aiComposerError, setAiComposerError] = useState<string | null>(null);
  const [aiComposerRawText, setAiComposerRawText] = useState<string | null>(null);
  const [aiComposerThinkingText, setAiComposerThinkingText] = useState<string | null>(null);
  const [aiComposerState, setAiComposerState] =
    useState<WorkflowOrchestrationSessionState | null>(null);
  const flowgramCanvasRef = useRef<BoardWorkspaceHandle | null>(null);
  const aiComposerTargetProjectIdRef = useRef<string | null>(null);
  const aiComposerSessionIdRef = useRef(0);
  const aiComposerGenerating = aiComposerStatus === 'generating';
  const migrationDoneRef = useRef(false);
  const preferredWorkflowAiProvider = useMemo(
    () => resolvePreferredWorkflowAiProvider(aiConfig),
    [aiConfig],
  );
  const aiWorkflowActionTitle = useMemo(() => {
    if (aiComposerGenerating) {
      return aiComposerMode === 'create' ? 'AI 正在流式编排新工作流' : 'AI 正在流式编辑当前工作流';
    }

    return getWorkflowAiUnavailableReason(aiConfig);
  }, [aiComposerGenerating, aiComposerMode, aiConfig]);

  const boardItems = useMemo<BoardItem[]>(
    () =>
      projectLibrary.projects.map((project) => ({
        id: project.id,
        name: project.name,
        description: project.description,
        nodeCount: parseProjectNodeCount(project.astText),
        updatedAt: formatRelativeTimestamp(project.updatedAt),
        snapshotCount: project.snapshots.length,
        environmentCount: project.environments.length,
        environmentName: getActiveEnvironment(project)?.name ?? '未选择环境',
        migrationNote: project.migrationNotes[0] ?? null,
      })),
    [projectLibrary.projects],
  );
  const {
    activeBoard,
    activeBoardId,
    activeProject,
    clearActiveBoard,
    openBoard,
    setSidebarSection,
    sidebarSection,
  } = useAppNavigation({
    boards: boardItems,
    projects: projectLibrary.projects,
    startupPage: settings.startupPage,
  });
  const currentProject = activeProject ?? projectLibrary.projects[0] ?? null;
  const connectionUsageById = useMemo(
    () => buildConnectionUsageMap(projectLibrary.projects),
    [projectLibrary.projects],
  );
  const astText = currentProject?.astText ?? '';
  const payloadText = currentProject?.payloadText ?? '{}';
  const graphState = useMemo(
    () =>
      astText
        ? parseWorkflowGraph(astText)
        : {
            graph: null,
            error: '当前没有可用的工作流工程。',
          },
    [astText],
  );

  const engine = useWorkflowEngine(activeBoard, sidebarSection);

  useEffect(() => {
    if (!hasTauriRuntime()) {
      setRuntimeWorkflowCount(0);
      return;
    }

    let cancelled = false;

    const loadRuntimeWorkflowCount = async () => {
      try {
        const workflows = await listRuntimeWorkflows();
        if (!cancelled) {
          setRuntimeWorkflowCount(workflows.length);
        }
      } catch {
        if (!cancelled) {
          setRuntimeWorkflowCount(0);
        }
      }
    };

    void loadRuntimeWorkflowCount();
    const timer = window.setInterval(() => {
      void loadRuntimeWorkflowCount();
    }, 2500);

    return () => {
      cancelled = true;
      window.clearInterval(timer);
    };
  }, [settings.projectWorkspacePath]);

  useEffect(() => {
    if (!connectionLibrary.storage.isReady || !projectLibrary.storage.isReady) {
      return;
    }

    if (migrationDoneRef.current) return;
    migrationDoneRef.current = true;

    const migratedConnections = collectLegacyProjectConnections(
      projectLibrary.projects,
      connectionLibrary.connections,
    );
    const legacyAstUpdates = collectLegacyProjectAstUpdates(projectLibrary.projects);

    if (migratedConnections.length === 0 && legacyAstUpdates.length === 0) {
      return;
    }

    if (migratedConnections.length > 0) {
      connectionLibrary.setConnections((current) => [...current, ...migratedConnections]);
    }

    legacyAstUpdates.forEach((update) => {
      projectLibrary.updateProjectDraft(update.projectId, { astText: update.astText });
    });
  }, [
    connectionLibrary.connections,
    connectionLibrary.setConnections,
    connectionLibrary.storage.isReady,
    projectLibrary.projects,
    projectLibrary.storage.isReady,
    projectLibrary.updateProjectDraft,
  ]);

  useEffect(() => {
    if (
      !hasTauriRuntime() ||
      sidebarSection !== 'connections' ||
      !connectionLibrary.storage.isReady ||
      connectionLibrary.storage.isSyncing
    ) {
      return;
    }

    void engine.refreshConnections();
  }, [
    connectionLibrary.connections,
    connectionLibrary.storage.isReady,
    connectionLibrary.storage.isSyncing,
    sidebarSection,
  ]);

  function updateProjectDraft(
    projectId: string,
    nextDraft: Partial<
      Pick<(typeof projectLibrary.projects)[number], 'astText' | 'payloadText' | 'name' | 'description'>
    >,
  ) {
    projectLibrary.updateProjectDraft(projectId, nextDraft);
  }

  function buildProjectDraftSnapshot(projectId: string) {
    const project = projectLibrary.projects.find((item) => item.id === projectId);
    if (!project) {
      return {
        graph: null,
        astText: null,
        error: '当前工程不存在。',
      };
    }

    const sourceGraphState = parseWorkflowGraph(project.astText);
    if (sourceGraphState.error && !flowgramCanvasRef.current?.getCurrentWorkflowGraph()) {
      return {
        graph: null,
        astText: null,
        error: sourceGraphState.error,
      };
    }

    const currentGraph =
      project.id === activeBoardId
        ? flowgramCanvasRef.current?.getCurrentWorkflowGraph() ?? sourceGraphState.graph
        : sourceGraphState.graph;
    if (!currentGraph) {
      return {
        graph: null,
        astText: null,
        error: '当前没有可执行的工作流。',
      };
    }

    const nextAstText = formatWorkflowGraph(stripWorkflowNodeLocalAiConfig(currentGraph));
    const nextGraphState = parseWorkflowGraph(nextAstText);
    if (nextGraphState.error || !nextGraphState.graph) {
      return {
        graph: null,
        astText: null,
        error: nextGraphState.error ?? '当前工作流快照无法序列化。',
      };
    }

    return {
      graph: nextGraphState.graph,
      astText: nextAstText,
      error: null,
    };
  }

  function applyStructuredGraphChange(nextAstText: string, nextStatusMessage: string) {
    if (!activeProject || nextAstText === activeProject.astText) {
      return;
    }

    updateProjectDraft(activeProject.id, { astText: nextAstText });
    engine.setStatusMessage(nextStatusMessage);
  }

  function handleGraphChange(nextAstText: string) {
    applyStructuredGraphChange(nextAstText, '画布变更已同步回项目草稿。');
  }

  function handlePayloadTextChange(nextText: string) {
    if (!activeProject) {
      return;
    }

    updateProjectDraft(activeProject.id, { payloadText: nextText });
  }

  function buildAiWorkflowDraftFromProject(project: ProjectRecord) {
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

  function resetAiComposerState(
    mode: WorkflowOrchestrationMode,
    nextRequirement = '',
    nextSessionState: WorkflowOrchestrationSessionState | null = null,
  ) {
    setAiComposerMode(mode);
    setAiComposerRequirement(nextRequirement);
    setAiComposerStatus('idle');
    setAiComposerError(null);
    setAiComposerRawText(null);
    setAiComposerThinkingText(null);
    setAiComposerState(nextSessionState);
  }

  function handleOpenAiCreate() {
    resetAiComposerState('create');
    setAiComposerOpen(true);
  }

  function handleOpenAiEdit() {
    if (!activeProject) {
      return;
    }

    resetAiComposerState(
      'edit',
      '',
      createWorkflowOrchestrationState(buildAiWorkflowDraftFromProject(activeProject)),
    );
    setAiComposerOpen(true);
  }

  function handleCloseAiComposer() {
    if (aiComposerGenerating) {
      return;
    }

    setAiComposerOpen(false);
    setAiComposerStatus('idle');
    setAiComposerError(null);
    setAiComposerRawText(null);
    setAiComposerThinkingText(null);
    setAiComposerState(null);
    aiComposerTargetProjectIdRef.current = null;
  }

  function ensureAiComposerTargetProject(
    mode: WorkflowOrchestrationMode,
    nextDraft: WorkflowOrchestrationSessionState['draft'],
  ) {
    if (mode === 'edit') {
      return activeProject?.id ?? aiComposerTargetProjectIdRef.current;
    }

    if (aiComposerTargetProjectIdRef.current) {
      return aiComposerTargetProjectIdRef.current;
    }

    const nextProject = projectLibrary.createProject(
      nextDraft.name.trim() || `AI 编排草稿 ${projectLibrary.projects.length + 1}`,
      nextDraft.description.trim() || 'AI 正在编排工作流。',
    );
    aiComposerTargetProjectIdRef.current = nextProject.id;
    openBoard(nextProject.id);
    engine.resetWorkspaceRuntime(`AI 正在编排工程 ${nextProject.name}。`);
    engine.appendRuntimeLog('ai', 'info', '已创建 AI 编排草稿工程', nextProject.name);
    return nextProject.id;
  }

  function applyAiComposerDraft(
    mode: WorkflowOrchestrationMode,
    nextState: WorkflowOrchestrationSessionState,
  ) {
    const projectId = ensureAiComposerTargetProject(mode, nextState.draft);
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

  async function handleSubmitAiComposer() {
    if (!preferredWorkflowAiProvider) {
      setAiComposerError(aiWorkflowActionTitle);
      return;
    }

    const requirement = aiComposerRequirement.trim();
    if (!requirement) {
      setAiComposerError('请输入编排需求。');
      return;
    }

    const mode = aiComposerMode;
    const baseDraft =
      mode === 'edit' && activeProject ? buildAiWorkflowDraftFromProject(activeProject) : null;
    const initialSessionState = createWorkflowOrchestrationState(baseDraft);
    const sessionId = aiComposerSessionIdRef.current + 1;
    aiComposerSessionIdRef.current = sessionId;
    aiComposerTargetProjectIdRef.current = mode === 'edit' ? activeProject?.id ?? null : null;

    setAiComposerStatus('generating');
    setAiComposerError(null);
    setAiComposerRawText(null);
    setAiComposerThinkingText(null);
    setAiComposerState(initialSessionState);
    engine.appendRuntimeLog(
      'ai',
      'info',
      mode === 'create' ? '开始 AI 流式编排工作流' : '开始 AI 流式编辑工作流',
      requirement,
    );
    engine.setStatusMessage(
      mode === 'create' ? 'AI 正在流式编排新工作流。' : 'AI 正在流式编辑当前工作流。',
    );

    try {
      const finalState = await streamWorkflowOrchestration({
        mode,
        requirement,
        providerId: preferredWorkflowAiProvider.id,
        model: preferredWorkflowAiProvider.defaultModel,
        baseDraft,
        params: aiConfig?.copilotParams,
        timeoutMs: aiConfig?.agentSettings.timeoutMs,
        onRawText: (rawText) => {
          if (aiComposerSessionIdRef.current !== sessionId) {
            return;
          }

          setAiComposerRawText(rawText);
        },
        onThinking: (thinkingText) => {
          if (aiComposerSessionIdRef.current !== sessionId) {
            return;
          }

          setAiComposerThinkingText(thinkingText);
        },
        onOperation: (operation, nextState) => {
          if (aiComposerSessionIdRef.current !== sessionId) {
            return;
          }

          startTransition(() => {
            setAiComposerState(nextState);
            applyAiComposerDraft(mode, nextState);
          });

          if (operation.type === 'done') {
            engine.appendRuntimeLog(
              'ai',
              'success',
              mode === 'create' ? 'AI 编排完成' : 'AI 编辑完成',
              nextState.summary ?? nextState.draft.name,
            );
            engine.setStatusMessage(
              mode === 'create'
                ? `AI 已完成工程 ${nextState.draft.name} 的流式编排。`
                : `AI 已完成工程 ${nextState.draft.name} 的流式编辑。`,
            );
          }
        },
        onRetry: (attempt, error, retryState) => {
          if (aiComposerSessionIdRef.current !== sessionId) {
            return;
          }

          setAiComposerError(null);
          startTransition(() => {
            setAiComposerState(retryState);
            if (mode === 'edit' || aiComposerTargetProjectIdRef.current) {
              applyAiComposerDraft(mode, retryState);
            }
          });
          engine.appendRuntimeLog(
            'ai',
            'warn',
            `AI 输出中断，正在自动重试（第 ${attempt} 次）`,
            error.message,
          );
          engine.setStatusMessage(
            mode === 'create'
              ? 'AI 输出中断，正在自动重试编排。'
              : 'AI 输出中断，正在自动重试编辑。',
          );
        },
      });

      if (aiComposerSessionIdRef.current !== sessionId) {
        return;
      }

      setAiComposerState(finalState);
      if (mode === 'edit' || finalState.operations.length > 0) {
        applyAiComposerDraft(mode, finalState);
      }
      setAiComposerStatus('completed');
    } catch (error) {
      if (aiComposerSessionIdRef.current !== sessionId) {
        return;
      }

      const { message, detail } = describeUnknownError(error);
      const composedDetail = detail ? `${message}\n\n${detail}` : message;
      const targetProjectId = aiComposerTargetProjectIdRef.current;
      setAiComposerStatus('interrupted');
      setAiComposerError(message);
      engine.appendAppError('command', 'AI 编排失败', composedDetail);
      engine.appendRuntimeLog(
        'ai',
        'error',
        mode === 'create' ? 'AI 编排失败' : 'AI 编辑失败',
        targetProjectId ? `${message}\n已保留当前部分草稿。` : message,
      );
      engine.setStatusMessage(
        targetProjectId ? 'AI 中断，已保留当前部分编排草稿。' : message,
      );
    }
  }

  function handleOpenBoard(board: BoardItem) {
    openBoard(board.id);

    if (getDeployProjectId(engine.deployInfo) === board.id) {
      engine.setStatusMessage(`已进入工程 ${board.name}，已保留当前运行态。`);
      return;
    }

    engine.resetWorkspaceRuntime(`已进入工程 ${board.name}。`);
  }

  function handleBackToBoards() {
    clearActiveBoard();
    engine.resetWorkspaceRuntime('已返回所有看板。');
  }

  function handleCreateBoard() {
    const nextProject = projectLibrary.createProject();
    openBoard(nextProject.id);
    engine.resetWorkspaceRuntime(`已创建工程 ${nextProject.name}。`);
    engine.appendRuntimeLog('project', 'success', '已创建工程', nextProject.name);
  }

  async function handleImportBoardFile(file: File) {
    try {
      const sourceText = await file.text();
      const result = projectLibrary.importProjects(sourceText);
      const nextProject = result.importedProjects[0] ?? null;

      if (nextProject) {
        openBoard(nextProject.id);
      }

      const detail = result.migrationNotes.length > 0 ? result.migrationNotes.join('\n') : null;
      engine.resetWorkspaceRuntime(
        nextProject
          ? `已导入工程 ${nextProject.name}。`
          : `已导入 ${result.importedProjects.length} 个工程。`,
      );
      engine.appendRuntimeLog('project', 'success', '工程导入完成', detail);
    } catch (error) {
      const { message, detail } = describeUnknownError(error);
      engine.appendAppError('command', '导入工程失败', detail ?? message);
      engine.setStatusMessage(message);
    }
  }

  function handleDeleteBoard(board: BoardItem) {
    const deletedProject = projectLibrary.deleteProject(board.id);
    if (!deletedProject) {
      engine.setStatusMessage('删除失败：当前工程不存在。');
      return;
    }

    if (activeBoardId === board.id) {
      clearActiveBoard();
    }

    engine.setStatusMessage(`已删除工程 ${deletedProject.name}。`);
    engine.appendRuntimeLog('project', 'warn', '已删除工程', deletedProject.name);
  }

  function handleCreateSnapshot() {
    if (!activeProject) {
      return;
    }

    const draftSnapshot = buildProjectDraftSnapshot(activeProject.id);
    if (draftSnapshot.error || !draftSnapshot.astText) {
      engine.appendAppError('command', '创建快照失败', draftSnapshot.error ?? '未知错误');
      engine.setStatusMessage(draftSnapshot.error ?? '创建快照失败。');
      return;
    }

    projectLibrary.saveProject(activeProject.id, {
      astText: draftSnapshot.astText,
      payloadText: activeProject.payloadText,
    });
    const nextProject = projectLibrary.createSnapshot(activeProject.id);
    engine.setStatusMessage(`已为 ${activeProject.name} 创建版本快照。`);
    engine.appendRuntimeLog(
      'project',
      'info',
      '已创建版本快照',
      nextProject ? `${nextProject.snapshots.length} 个版本` : activeProject.name,
    );
  }

  function handleRollbackSnapshot(snapshotId: string) {
    if (!activeProject) {
      return;
    }

    const nextProject = projectLibrary.rollbackProject(activeProject.id, snapshotId);
    engine.setStatusMessage(`已回滚工程 ${activeProject.name}。`);
    engine.appendRuntimeLog(
      'project',
      'warn',
      '已回滚工程版本',
      nextProject?.snapshots[0]?.label ?? activeProject.name,
    );
  }

  function handleDeleteSnapshot(snapshotId: string) {
    if (!activeProject) {
      return;
    }

    const nextProject = projectLibrary.deleteSnapshot(activeProject.id, snapshotId);
    engine.setStatusMessage(`已删除 ${activeProject.name} 的版本快照。`);
    engine.appendRuntimeLog(
      'project',
      'info',
      '已删除版本快照',
      nextProject ? `剩余 ${nextProject.snapshots.length} 个版本` : activeProject.name,
    );
  }

  function handleEnvironmentChange(environmentId: string) {
    if (!activeProject) {
      return;
    }

    const nextEnvironment = activeProject.environments.find(
      (environment) => environment.id === environmentId,
    );
    projectLibrary.setActiveEnvironment(activeProject.id, environmentId);
    engine.setStatusMessage(`已切换到环境 ${nextEnvironment?.name ?? '未命名环境'}。`);
    engine.appendRuntimeLog(
      'project',
      'info',
      '已切换运行环境',
      nextEnvironment?.name ?? environmentId,
    );
  }

  function handleEnvironmentSave(
    environmentId: string,
    patch: { name: string; description: string; diff: ProjectEnvironmentDiff },
  ) {
    if (!activeProject) {
      return;
    }

    projectLibrary.updateEnvironment(activeProject.id, environmentId, {
      name: patch.name,
      description: patch.description,
      diff: patch.diff,
    });
    engine.setStatusMessage(`已更新环境配置 ${patch.name}。`);
    engine.appendRuntimeLog('project', 'success', '环境差异配置已更新', patch.name);
  }

  function handleDuplicateEnvironment(environmentId: string) {
    if (!activeProject) {
      return;
    }

    const nextEnvironment = projectLibrary.duplicateEnvironment(activeProject.id, environmentId);
    if (!nextEnvironment) {
      return;
    }

    engine.setStatusMessage(`已派生环境 ${nextEnvironment.name}。`);
    engine.appendRuntimeLog('project', 'info', '已派生环境', nextEnvironment.name);
  }

  function handleDeleteEnvironment(environmentId: string) {
    if (!activeProject) {
      return;
    }

    projectLibrary.deleteEnvironment(activeProject.id, environmentId);
    engine.setStatusMessage('已删除环境配置。');
    engine.appendRuntimeLog('project', 'warn', '已删除环境配置');
  }

  function buildDeploySnapshot() {
    if (!activeProject) {
      return {
        snapshot: null,
        astText: null,
        runtimeAstText: null,
        runtimeConnections: null,
        error: '请先从所有看板进入工程。',
      };
    }

    if (!connectionLibrary.storage.isReady) {
      return {
        snapshot: null,
        astText: null,
        runtimeAstText: null,
        runtimeConnections: null,
        error: '连接资源仍在加载，请稍后再试。',
      };
    }

    const draftSnapshot = buildProjectDraftSnapshot(activeProject.id);
    if (draftSnapshot.error || !draftSnapshot.graph || !draftSnapshot.astText) {
      return {
        snapshot: null,
        astText: null,
        runtimeAstText: null,
        runtimeConnections: null,
        error: draftSnapshot.error ?? '当前工作流快照无效。',
      };
    }

    const runtimeGraph = projectLibrary.getProjectGraphForRuntime(
      activeProject.id,
      draftSnapshot.graph,
    );
    if (!runtimeGraph) {
      return {
        snapshot: null,
        astText: null,
        runtimeAstText: null,
        runtimeConnections: null,
        error: '当前环境差异配置无法应用到工作流。',
      };
    }

    const runtimeConnections = applyEnvironmentToConnectionDefinitions(
      connectionLibrary.connections,
      getActiveEnvironment(activeProject),
    );
    const runtimeAstText = formatWorkflowGraph(
      applyGlobalAiConfigToWorkflowGraph(runtimeGraph, aiConfig),
    );
    const nextGraphState = parseWorkflowGraph(runtimeAstText);
    if (nextGraphState.error || !nextGraphState.graph) {
      return {
        snapshot: null,
        astText: null,
        runtimeAstText: null,
        runtimeConnections: null,
        error: nextGraphState.error ?? '环境覆盖后的工作流无法序列化。',
      };
    }

    const activeEnvironment = getActiveEnvironment(activeProject);
    const snapshot: DeploymentSnapshot = {
      projectId: activeProject.id,
      projectName: activeProject.name,
      environmentId: activeEnvironment?.id ?? activeProject.activeEnvironmentId,
      environmentName: activeEnvironment?.name ?? '默认环境',
      astText: draftSnapshot.astText,
      runtimeAstText,
      runtimeConnections,
    };

    return {
      snapshot,
      astText: draftSnapshot.astText,
      runtimeAstText,
      runtimeConnections,
      error: null,
    };
  }

  const {
    beginRestoreCheckPause,
    endRestoreCheckPause,
    handleConfirmRestore,
    handleSkipRestore,
    pendingRestoreLeadSession,
    pendingRestoreSessions,
    persistActiveDeploymentProject,
    removePersistedDeploymentSnapshot,
    restoreCountdown,
    runDeploymentSnapshot,
  } = useDeploymentRestore({
    workspacePath: settings.projectWorkspacePath,
    projects: projectLibrary.projects,
    projectStorageReady: projectLibrary.storage.isReady,
    connectionStorageReady: connectionLibrary.storage.isReady,
    deployInfo: engine.deployInfo,
    runtimeWorkflowCount,
    appendAppError: engine.appendAppError,
    appendRuntimeLog: engine.appendRuntimeLog,
    applyDeploymentState: engine.applyDeploymentState,
    refreshConnections: engine.refreshConnections,
    setStatusMessage: engine.setStatusMessage,
    onRestoreProject: openBoard,
  });

  async function handleDeploy() {
    if (!activeBoard || !activeProject) {
      engine.setStatusMessage('请先从所有看板进入工程。');
      return;
    }

    const nextDeploySnapshot = buildDeploySnapshot();
    if (
      nextDeploySnapshot.error ||
      !nextDeploySnapshot.snapshot ||
      !nextDeploySnapshot.astText ||
      !nextDeploySnapshot.runtimeAstText ||
      !nextDeploySnapshot.runtimeConnections
    ) {
      engine.appendAppError(
        'command',
        '部署前 AST 校验失败',
        nextDeploySnapshot.error ?? '未知错误',
      );
      engine.setStatusMessage(`AST 无法部署: ${nextDeploySnapshot.error ?? '未知错误'}`);
      return;
    }

    if (nextDeploySnapshot.astText !== activeProject.astText) {
      updateProjectDraft(activeProject.id, { astText: nextDeploySnapshot.astText });
    }

    await runDeploymentSnapshot(nextDeploySnapshot.snapshot, 'manual');
  }

  async function handleUndeploy() {
    if (!activeBoard) {
      engine.setStatusMessage('请先从所有看板进入工程。');
      return;
    }

    if (!hasTauriRuntime()) {
      beginRestoreCheckPause();
      engine.resetWorkspaceRuntime('已在预览态清空部署状态。');
      engine.appendRuntimeLog('system', 'info', '预览态已清空部署状态');
      await removePersistedDeploymentSnapshot(activeBoard.id);
      setRuntimeWorkflowCount(0);
      endRestoreCheckPause();
      return;
    }

    beginRestoreCheckPause();

    try {
      const response = await undeployWorkflow(activeBoard.id);
      await removePersistedDeploymentSnapshot(activeBoard.id);
      const nextWorkflows = await listRuntimeWorkflows();
      const nextActiveWorkflow = nextWorkflows.find((workflow) => workflow.active) ?? null;
      await persistActiveDeploymentProject(
        nextActiveWorkflow?.projectId?.trim() || nextActiveWorkflow?.workflowId.trim() || null,
      );
      setRuntimeWorkflowCount(nextWorkflows.length);
      if (!response.hadWorkflow) {
        engine.setStatusMessage('当前没有已部署工作流。');
      }
    } catch (error) {
      const { message, detail } = describeUnknownError(error);
      engine.appendAppError('command', '反部署失败', detail ?? message);
      engine.setStatusMessage(message);
    } finally {
      endRestoreCheckPause();
    }
  }

  async function handleDispatchPayload() {
    if (!activeBoard) {
      engine.setStatusMessage('请先从所有看板进入工程。');
      return;
    }

    let payload: unknown;

    try {
      payload = JSON.parse(payloadText);
    } catch (error) {
      engine.appendAppError(
        'command',
        '测试载荷 JSON 无法解析',
        error instanceof Error ? error.message : null,
      );
      engine.setStatusMessage(
        error instanceof Error ? `Payload JSON 无法解析: ${error.message}` : 'Payload JSON 无法解析',
      );
      return;
    }

    if (!hasTauriRuntime()) {
      engine.addResult({
        trace_id: 'web-preview',
        timestamp: new Date().toISOString(),
        payload: payload as WorkflowResult['payload'],
      });
      engine.setStatusMessage('已在纯 Web 预览模式下模拟发送 payload。');
      engine.appendRuntimeLog('system', 'info', '已在预览态模拟发送测试载荷');
      return;
    }

    if (!currentBoardDeployInfo) {
      engine.appendAppError('command', '发送测试载荷失败', '请先部署工作流');
      engine.setStatusMessage('请先部署工作流，再发送测试消息。');
      return;
    }

    try {
      const response = await dispatchPayload(payload, activeBoard.id);
      engine.appendRuntimeLog('dispatch', 'info', '已提交测试载荷', `trace_id=${response.traceId}`);
      engine.setStatusMessage(`已提交 payload，trace_id=${response.traceId}`);
    } catch (error) {
      const { message, detail } = describeUnknownError(error);
      engine.appendAppError('command', '发送测试载荷失败', detail ?? message);
      engine.setStatusMessage(message);
    }
  }

  const graphNodeCount = graphState.graph ? Object.keys(graphState.graph.nodes).length : 0;
  const graphEdgeCount = graphState.graph?.edges.length ?? 0;
  const graphConnectionCount = getReferencedConnectionCount(graphState.graph?.nodes ?? null);
  const isTauriRuntime = hasTauriRuntime();
  const currentUserRole = isTauriRuntime ? '桌面操作员' : '预览访客';
  const runtimeModeLabel = isTauriRuntime ? '桌面会话' : '浏览器预览';
  const currentBoardDeployInfo =
    activeBoard && getDeployProjectId(engine.deployInfo) === activeBoard.id ? engine.deployInfo : null;
  const workflowStatus = deriveWorkflowStatus(
    isTauriRuntime,
    Boolean(activeBoard),
    currentBoardDeployInfo,
    engine.runtimeState,
  );
  const workflowStatusLabel = getWorkflowStatusLabel(workflowStatus);
  const workflowStatusPillClass = getWorkflowStatusPillClass(workflowStatus);
  const canDispatchPayload =
    Boolean(activeBoard) && (!isTauriRuntime || Boolean(currentBoardDeployInfo));
  const connectionPreview = engine.connections.slice(0, 4);
  const sidebarSections = buildSidebarSections(
    workflowStatusLabel,
    runtimeWorkflowCount,
    engine.connections.length,
    engine.eventFeed.length + engine.appErrors.length,
    boardItems.length,
    currentBoardDeployInfo,
    activeBoard?.name ?? null,
  );

  function renderRestoreDialog() {
    if (pendingRestoreSessions.length === 0 || !pendingRestoreLeadSession) {
      return null;
    }

    return (
      <RestoreDeploymentDialog
        sessions={pendingRestoreSessions}
        leadSession={pendingRestoreLeadSession}
        countdown={restoreCountdown}
        onSkip={() => {
          void handleSkipRestore();
        }}
        onConfirm={() => {
          void handleConfirmRestore();
        }}
      />
    );
  }

  function renderProjectGate(title: string) {
    return (
      <section className="studio-content studio-content--panel">
        <div className="panel studio-content__panel studio-content__panel--scroll studio-gate">
          <div className="studio-gate__copy">
            <h2>{title}</h2>
            <p>先从所有看板进入工程。</p>
          </div>
          <button type="button" onClick={() => setSidebarSection('boards')}>
            前往所有看板
          </button>
        </div>
      </section>
    );
  }

  function renderBoardWorkspace() {
    if (!activeBoard || !activeProject) {
      return (
        <section className="studio-content studio-content--panel">
          <div className="panel studio-content__panel studio-content__panel--scroll">
            <BoardsPanel
              boards={boardItems}
              onOpenBoard={handleOpenBoard}
              onCreateBoard={handleCreateBoard}
              onStartAiCreate={handleOpenAiCreate}
              onImportBoardFile={handleImportBoardFile}
              onDeleteBoard={handleDeleteBoard}
              aiActionTitle={aiWorkflowActionTitle}
              aiActionDisabled={!preferredWorkflowAiProvider || aiComposerGenerating}
              aiActionLoading={aiComposerGenerating && aiComposerMode === 'create'}
            />
          </div>
        </section>
      );
    }

    return (
      <section className="studio-content studio-content--board">
        <BoardWorkspace
          ref={flowgramCanvasRef}
          project={activeProject}
          graph={graphState.graph}
          nodeCount={graphNodeCount}
          connectionPreview={connectionPreview}
          themeMode={settings.themeMode}
          isRuntimeDockCollapsed={engine.isRuntimeDockCollapsed}
          flowgramResources={{
            connections: connectionLibrary.connections,
            aiProviders: aiConfig?.providers ?? [],
            activeAiProviderId: aiConfig?.activeProviderId ?? null,
            copilotParams: aiConfig?.copilotParams ?? {},
          }}
          flowgramRuntime={{
            runtimeState: engine.runtimeState,
            workflowStatus,
            canDispatchPayload,
          }}
          flowgramAppearance={{
            accentHex: settings.accentHex,
            nodeCodeColor: settings.accentThemeVariables['--node-code'],
          }}
          flowgramExportTarget={{
            workspacePath: settings.projectWorkspacePath,
            workflowName: activeProject.name,
          }}
          flowgramActions={{
            onRunRequested: handleDeploy,
            onStopRequested: handleUndeploy,
            onDispatchRequested: handleDispatchPayload,
            onGraphChange: handleGraphChange,
            onError: engine.handleFlowgramError,
            onStatusMessage: engine.setStatusMessage,
          }}
          runtimeDock={{
            eventFeed: engine.eventFeed,
            appErrors: engine.appErrors,
            results: engine.results,
          }}
          onToggleRuntimeDockCollapsed={() => engine.setIsRuntimeDockCollapsed((current) => !current)}
          onBack={handleBackToBoards}
          onCreateSnapshot={handleCreateSnapshot}
          onDeleteSnapshot={handleDeleteSnapshot}
          onRollbackSnapshot={handleRollbackSnapshot}
          onEnvironmentChange={handleEnvironmentChange}
          onEnvironmentSave={handleEnvironmentSave}
          onDuplicateEnvironment={handleDuplicateEnvironment}
          onDeleteEnvironment={handleDeleteEnvironment}
          onOpenAiComposer={handleOpenAiEdit}
          aiActionTitle={aiWorkflowActionTitle}
          aiActionDisabled={!preferredWorkflowAiProvider || aiComposerGenerating}
          aiActionLoading={aiComposerGenerating && aiComposerMode === 'edit'}
        />
      </section>
    );
  }

  function renderStudioContent() {
    switch (sidebarSection) {
      case 'dashboard':
        return (
          <section className="studio-content studio-content--panel">
            <div className="panel studio-content__panel studio-content__panel--scroll">
              <DashboardPanel
                userId={CURRENT_USER_NAME}
                activeBoardName={activeBoard?.name ?? null}
                boardCount={boardItems.length}
                graphNodeCount={graphNodeCount}
                graphEdgeCount={graphEdgeCount}
                graphConnectionCount={graphConnectionCount}
                activeNodeCount={engine.runtimeState.activeNodeIds.length}
                completedNodeCount={engine.runtimeState.completedNodeIds.length}
                failedNodeCount={engine.runtimeState.failedNodeIds.length}
                outputNodeCount={engine.runtimeState.outputNodeIds.length}
                eventCount={engine.eventFeed.length}
                resultCount={engine.results.length}
                statusMessage={engine.statusMessage}
                deployInfo={currentBoardDeployInfo}
                onNavigateToBoards={handleBackToBoards}
              />
            </div>
          </section>
        );
      case 'boards':
        return renderBoardWorkspace();
      case 'runtime':
        return (
          <section className="studio-content studio-content--panel">
            <div className="panel studio-content__panel studio-content__panel--scroll">
              <RuntimeManagerPanel
                workspacePath={settings.projectWorkspacePath}
                themeMode={settings.themeMode}
                activeBoardId={activeBoard?.id ?? null}
                onOpenBoard={(boardId) => {
                  const targetBoard =
                    boardItems.find((board) => board.id === boardId) ?? {
                      id: boardId,
                      name: boardId,
                      description: '',
                      nodeCount: 0,
                      updatedAt: '',
                      snapshotCount: 0,
                      environmentCount: 0,
                      environmentName: '未选择环境',
                      migrationNote: null,
                    };
                  handleOpenBoard(targetBoard);
                }}
                onPersistActiveProject={persistActiveDeploymentProject}
                onBeforeWorkflowStop={beginRestoreCheckPause}
                onAfterWorkflowStop={endRestoreCheckPause}
                onRemovePersistedDeployment={removePersistedDeploymentSnapshot}
                onStatusMessage={engine.setStatusMessage}
                onRuntimeCountChange={setRuntimeWorkflowCount}
              />
            </div>
          </section>
        );
      case 'connections':
        return (
          <section className="studio-content studio-content--panel">
            <div className="panel studio-content__panel studio-content__panel--scroll panel--connection-card">
              <ConnectionStudio
                connections={connectionLibrary.connections}
                setConnections={connectionLibrary.setConnections}
                usageByConnection={connectionUsageById}
                runtimeConnections={engine.connections}
                isLoading={!connectionLibrary.storage.isReady}
                storageError={connectionLibrary.storage.error}
                onStatusMessage={(msg) => engine.setStatusMessage(msg)}
              />
            </div>
          </section>
        );
      case 'plugins':
        return (
          <section className="studio-content studio-content--panel">
            <div className="panel studio-content__panel studio-content__panel--scroll">
              <PluginPanel isTauriRuntime={isTauriRuntime} />
            </div>
          </section>
        );
      case 'payload':
        if (!activeBoard) {
          return renderProjectGate('测试载荷');
        }

        return (
          <section className="studio-content studio-content--panel">
            <div className="panel studio-content__panel studio-content__panel--editor">
              <PayloadPanel
                payloadText={payloadText}
                deployInfo={currentBoardDeployInfo}
                onPayloadTextChange={handlePayloadTextChange}
              />
            </div>
          </section>
        );
      case 'logs':
        return (
          <section className="studio-content studio-content--panel">
            <div className="panel studio-content__panel studio-content__panel--scroll">
              <LogsPanel
                eventFeed={engine.eventFeed}
                appErrors={engine.appErrors}
                resultCount={engine.results.length}
                themeMode={settings.themeMode}
                activeBoardName={activeBoard?.name ?? null}
                workflowStatusLabel={workflowStatusLabel}
                workspacePath={settings.projectWorkspacePath}
                activeTraceId={engine.runtimeState.traceId}
              />
            </div>
          </section>
        );
      case 'settings':
        return (
          <section className="studio-content studio-content--panel">
            <div className="panel studio-content__panel studio-content__panel--scroll">
              <SettingsPanel
                isTauriRuntime={isTauriRuntime}
                runtimeModeLabel={runtimeModeLabel}
                workflowStatusLabel={workflowStatusLabel}
                statusMessage={engine.statusMessage}
                themeMode={settings.themeMode}
                onThemeModeChange={settings.setThemeMode}
                accentPreset={settings.accentPreset}
                accentOptions={ACCENT_PRESET_OPTIONS}
                customAccentHex={settings.customAccentHex}
                onAccentPresetChange={settings.setAccentPreset}
                onCustomAccentChange={settings.setCustomAccentHex}
                motionMode={settings.motionMode}
                onMotionModeChange={settings.setMotionMode}
                startupPage={settings.startupPage}
                onStartupPageChange={settings.setStartupPage}
                projectWorkspacePath={settings.projectWorkspacePath}
                projectWorkspaceResolvedPath={projectLibrary.storage.resolvedWorkspacePath}
                projectWorkspaceBoardsDirectoryPath={projectLibrary.storage.boardsDirectoryPath}
                projectWorkspaceUsingDefault={projectLibrary.storage.usingDefaultLocation}
                projectWorkspaceIsSyncing={projectLibrary.storage.isSyncing}
                projectWorkspaceError={projectLibrary.storage.error}
                onProjectWorkspacePathChange={settings.setProjectWorkspacePath}
              />
            </div>
          </section>
        );
      case 'ai':
        return (
          <section className="studio-content studio-content--panel">
            <div className="panel studio-content__panel studio-content__panel--scroll">
              <AiConfigPanel
                isTauriRuntime={isTauriRuntime}
                aiConfig={aiConfig}
                aiConfigLoading={aiConfigLoading}
                aiConfigError={aiConfigError}
                onAiConfigSave={handleAiConfigSave}
                onAiProviderTest={handleAiProviderTest}
                aiTestResult={aiTestResult}
                aiTesting={aiTesting}
              />
            </div>
          </section>
        );
      case 'about':
        return (
          <section className="studio-content studio-content--panel">
            <div className="panel studio-content__panel studio-content__panel--scroll">
              <AboutPanel />
            </div>
          </section>
        );
    }
  }

  return (
    <main className="app-shell app-shell--studio">
      <section className="studio-frame">
        <aside className="studio-nav-sidebar">
          <SidebarNav
            activeSection={sidebarSection}
            sections={sidebarSections}
            onSectionChange={setSidebarSection}
            userName={CURRENT_USER_NAME}
            userRole={currentUserRole}
            onUserSwitch={() => setSidebarSection('settings')}
            workflowStatusLabel={workflowStatusLabel}
            workflowStatusPillClass={workflowStatusPillClass}
            themeMode={settings.themeMode}
            onToggleTheme={settings.toggleTheme}
          />
        </aside>

        {renderStudioContent()}
      </section>
      <AiWorkflowComposer
        open={aiComposerOpen}
        mode={aiComposerMode}
        activeProjectName={activeProject?.name ?? null}
        status={aiComposerStatus}
        generating={aiComposerGenerating}
        requirement={aiComposerRequirement}
        error={aiComposerError}
        rawText={aiComposerRawText}
        thinkingText={aiComposerThinkingText}
        draft={aiComposerState?.draft ?? null}
        operations={aiComposerState?.operations ?? []}
        onRequirementChange={setAiComposerRequirement}
        onClose={handleCloseAiComposer}
        onSubmit={() => {
          void handleSubmitAiComposer();
        }}
      />
      {renderRestoreDialog()}
    </main>
  );
}

export default App;
