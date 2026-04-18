import { useEffect, useMemo, useRef, useState } from 'react';

import { useProjectLibrary } from './hooks/use-project-library';
import { useSettings } from './hooks/use-settings';
import { useWorkflowEngine } from './hooks/use-workflow-engine';
import { useConnectionLibrary } from './hooks/use-connection-library';

import { AboutPanel } from './components/app/AboutPanel';
import { AiConfigPanel } from './components/app/AiConfigPanel';
import { BoardsPanel, type BoardItem } from './components/app/BoardsPanel';
import { DashboardPanel } from './components/app/DashboardPanel';
import { LogsPanel } from './components/app/LogsPanel';
import { PayloadPanel } from './components/app/PayloadPanel';
import { PluginPanel } from './components/app/PluginPanel';
import { ProjectWorkspaceHeader } from './components/app/ProjectWorkspaceHeader';
import { RuntimeDock } from './components/app/RuntimeDock';
import { RuntimeManagerPanel } from './components/app/RuntimeManagerPanel';
import { SettingsPanel } from './components/app/SettingsPanel';
import { SidebarNav } from './components/app/SidebarNav';
import type { SidebarSection } from './components/app/types';
import { ConnectionStudio } from './components/ConnectionStudio';
import { FlowgramCanvas, type FlowgramCanvasHandle } from './components/FlowgramCanvas';
import {
  clearDeploymentSession,
  loadDeploymentSessionState,
  removeDeploymentSession,
  saveDeploymentSession,
  setDeploymentSessionActiveProject,
  type PersistedDeploymentSession,
} from './lib/deployment-session';
import { parseWorkflowGraph } from './lib/graph';
import { formatWorkflowGraph } from './lib/flowgram';
import {
  arePersistedDeploymentSessionStatesEqual,
  getPreferredRestoreSession,
  mergePersistedDeploymentSessionStates,
  normalizePersistedDeploymentSessionState,
  sortPersistedDeploymentSessions,
} from './lib/persisted-deployment-state';
import {
  applyEnvironmentToConnectionDefinitions,
  CURRENT_USER_NAME,
  formatRelativeTimestamp,
  getActiveEnvironment,
  parseProjectNodeCount,
  type ProjectEnvironmentDiff,
} from './lib/projects';
import { buildSidebarSections } from './lib/sidebar';
import { ACCENT_PRESET_OPTIONS } from './lib/theme';
import {
  applyGlobalAiConfigToWorkflowGraph,
  stripWorkflowNodeLocalAiConfig,
} from './lib/workflow-ai';
import {
  clearDeploymentSessionFile,
  deployWorkflow,
  dispatchPayload,
  hasTauriRuntime,
  loadDeploymentSessionStateFile,
  listRuntimeWorkflows,
  removeDeploymentSessionFile,
  saveDeploymentSessionFile,
  setDeploymentSessionActiveProjectFile,
  undeployWorkflow,
  loadAiConfig,
  saveAiConfig,
  testAiProvider,
} from './lib/tauri';
import type { ConnectionDefinition, WorkflowResult, WorkflowNodeDefinition } from './types';
import type { AiConfigUpdate, AiConfigView, AiProviderDraft, AiTestResult } from './types';
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

function downloadTextFile(fileName: string, text: string) {
  const blob = new Blob([text], { type: 'application/json;charset=utf-8' });
  const url = URL.createObjectURL(blob);
  const anchor = document.createElement('a');
  anchor.href = url;
  anchor.download = fileName;
  anchor.click();
  window.setTimeout(() => URL.revokeObjectURL(url), 1000);
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
  const [activeBoardId, setActiveBoardId] = useState<string | null>(null);
  const [sidebarSection, setSidebarSection] = useState<SidebarSection>(settings.startupPage);
  const [pendingRestoreSessions, setPendingRestoreSessions] =
    useState<PersistedDeploymentSession[]>([]);
  const [pendingRestoreActiveProjectId, setPendingRestoreActiveProjectId] = useState<string | null>(null);
  const [restoreCountdown, setRestoreCountdown] = useState(10);
  const [isRestoreCheckPaused, setIsRestoreCheckPaused] = useState(false);
  const [runtimeWorkflowCount, setRuntimeWorkflowCount] = useState(0);
  const [aiConfig, setAiConfig] = useState<AiConfigView | null>(null);
  const [aiConfigLoading, setAiConfigLoading] = useState(true);
  const [aiConfigError, setAiConfigError] = useState<string | null>(null);
  const [aiTestResult, setAiTestResult] = useState<AiTestResult | null>(null);
  const [aiTesting, setAiTesting] = useState(false);
  const flowgramCanvasRef = useRef<FlowgramCanvasHandle | null>(null);
  const restoreLookupRef = useRef<{
    scope: string | null;
    status: 'idle' | 'loading' | 'prompted' | 'handled' | 'none';
  }>({
    scope: null,
    status: 'idle',
  });
  const migrationDoneRef = useRef(false);

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
  const activeBoard = useMemo(
    () => boardItems.find((board) => board.id === activeBoardId) ?? null,
    [activeBoardId, boardItems],
  );
  const activeProject = useMemo(
    () => projectLibrary.projects.find((project) => project.id === activeBoardId) ?? null,
    [activeBoardId, projectLibrary.projects],
  );
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
    if (!hasTauriRuntime()) {
      setAiConfigLoading(false);
      return;
    }

    let cancelled = false;

    const load = async () => {
      try {
        const config = await loadAiConfig();
        if (!cancelled) {
          setAiConfig(config);
          setAiConfigError(null);
        }
      } catch (error) {
        if (!cancelled) {
          setAiConfigError(describeUnknownError(error).message);
        }
      } finally {
        if (!cancelled) {
          setAiConfigLoading(false);
        }
      }
    };

    void load();
    return () => {
      cancelled = true;
    };
  }, []);

  useEffect(() => {
    if (!activeBoardId) {
      return;
    }

    if (projectLibrary.projects.some((project) => project.id === activeBoardId)) {
      return;
    }

    setActiveBoardId(null);
    setSidebarSection('boards');
  }, [activeBoardId, projectLibrary.projects]);

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
    nextDraft: Partial<Pick<(typeof projectLibrary.projects)[number], 'astText' | 'payloadText'>>,
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

  function handleOpenBoard(board: BoardItem) {
    setActiveBoardId(board.id);
    setSidebarSection('boards');

    if (getDeployProjectId(engine.deployInfo) === board.id) {
      engine.setStatusMessage(`已进入工程 ${board.name}，已保留当前运行态。`);
      return;
    }

    engine.resetWorkspaceRuntime(`已进入工程 ${board.name}。`);
  }

  function handleBackToBoards() {
    setSidebarSection('boards');
    setActiveBoardId(null);
    engine.resetWorkspaceRuntime('已返回所有看板。');
  }

  function handleCreateBoard() {
    const nextProject = projectLibrary.createProject();
    setActiveBoardId(nextProject.id);
    setSidebarSection('boards');
    engine.resetWorkspaceRuntime(`已创建工程 ${nextProject.name}。`);
    engine.appendRuntimeLog('project', 'success', '已创建工程', nextProject.name);
  }

  async function handleImportBoardFile(file: File) {
    try {
      const sourceText = await file.text();
      const result = projectLibrary.importProjects(sourceText);
      const nextProject = result.importedProjects[0] ?? null;

      if (nextProject) {
        setActiveBoardId(nextProject.id);
        setSidebarSection('boards');
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

  function handleExportBoard(projectId: string) {
    const draftSnapshot =
      activeProject?.id === projectId ? buildProjectDraftSnapshot(projectId) : null;
    const exported = projectLibrary.exportProject(
      projectId,
      draftSnapshot?.astText ? { astText: draftSnapshot.astText } : undefined,
    );

    if (!exported) {
      engine.setStatusMessage('导出失败：当前工程不存在。');
      return;
    }

    downloadTextFile(exported.fileName, exported.text);
    engine.setStatusMessage(`已导出工程 ${exported.fileName}。`);
    engine.appendRuntimeLog('project', 'info', '已导出工程包', exported.fileName);
  }

  function handleDeleteBoard(board: BoardItem) {
    const deletedProject = projectLibrary.deleteProject(board.id);
    if (!deletedProject) {
      engine.setStatusMessage('删除失败：当前工程不存在。');
      return;
    }

    if (activeBoardId === board.id) {
      setActiveBoardId(null);
      setSidebarSection('boards');
    }

    engine.setStatusMessage(`已删除工程 ${deletedProject.name}。`);
    engine.appendRuntimeLog('project', 'warn', '已删除工程', deletedProject.name);
  }

  function handleSaveProject() {
    if (!activeProject) {
      return;
    }

    const draftSnapshot = buildProjectDraftSnapshot(activeProject.id);
    if (draftSnapshot.error || !draftSnapshot.astText) {
      engine.appendAppError('command', '保存工程失败', draftSnapshot.error ?? '未知错误');
      engine.setStatusMessage(draftSnapshot.error ?? '保存工程失败。');
      return;
    }

    projectLibrary.saveProject(activeProject.id, {
      astText: draftSnapshot.astText,
      payloadText: activeProject.payloadText,
    });
    engine.setStatusMessage(`已保存工程 ${activeProject.name}。`);
    engine.appendRuntimeLog('project', 'success', '工程已保存', activeProject.name);
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

  async function persistDeploymentSnapshot(snapshot: Pick<
    DeploymentSnapshot,
    'projectId' | 'projectName' | 'environmentId' | 'environmentName' | 'runtimeAstText' | 'runtimeConnections'
  >) {
    const session = {
      version: 1 as const,
      projectId: snapshot.projectId,
      projectName: snapshot.projectName,
      environmentId: snapshot.environmentId,
      environmentName: snapshot.environmentName,
      deployedAt: new Date().toISOString(),
      runtimeAstText: snapshot.runtimeAstText,
      runtimeConnections: snapshot.runtimeConnections,
    };
    const activeProjectId = snapshot.projectId;

    saveDeploymentSession(settings.projectWorkspacePath, session, activeProjectId);

    if (!hasTauriRuntime()) {
      return;
    }

    try {
      await saveDeploymentSessionFile(settings.projectWorkspacePath, session, activeProjectId);
    } catch (error) {
      const { message, detail } = describeUnknownError(error);
      engine.appendAppError('command', '写入部署会话失败，已降级为本地缓存', detail ?? message);
    }
  }

  async function persistActiveDeploymentProject(projectId: string | null) {
    const targetProjectId = projectId?.trim() || null;
    setDeploymentSessionActiveProject(settings.projectWorkspacePath, targetProjectId);

    if (!hasTauriRuntime()) {
      return;
    }

    try {
      await setDeploymentSessionActiveProjectFile(settings.projectWorkspacePath, targetProjectId);
    } catch (error) {
      const { message, detail } = describeUnknownError(error);
      engine.appendAppError('command', '更新主控工作流失败，已降级为本地缓存', detail ?? message);
    }
  }

  async function removePersistedDeploymentSnapshot(projectId: string) {
    const targetProjectId = projectId.trim();
    if (!targetProjectId) {
      return;
    }

    removeDeploymentSession(settings.projectWorkspacePath, targetProjectId);

    if (!hasTauriRuntime()) {
      return;
    }

    try {
      await removeDeploymentSessionFile(settings.projectWorkspacePath, targetProjectId);
    } catch (error) {
      const { message, detail } = describeUnknownError(error);
      engine.appendAppError('command', '清理部署会话失败', detail ?? message);
    }
  }

  async function clearPersistedDeploymentSnapshots() {
    clearDeploymentSession(settings.projectWorkspacePath);

    if (!hasTauriRuntime()) {
      return;
    }

    try {
      await clearDeploymentSessionFile(settings.projectWorkspacePath);
    } catch (error) {
      const { message, detail } = describeUnknownError(error);
      engine.appendAppError('command', '清理部署会话失败', detail ?? message);
    }
  }

  async function loadPersistedDeploymentSnapshots() {
    const localState = normalizePersistedDeploymentSessionState(
      loadDeploymentSessionState(settings.projectWorkspacePath),
    );

    if (!hasTauriRuntime()) {
      return localState;
    }

    let fileState = normalizePersistedDeploymentSessionState({
      sessions: [],
      activeProjectId: null,
    });
    let fileLoaded = false;

    try {
      fileState = normalizePersistedDeploymentSessionState(
        await loadDeploymentSessionStateFile(settings.projectWorkspacePath),
      );
      fileLoaded = true;
    } catch (error) {
      const { message, detail } = describeUnknownError(error);
      engine.appendAppError('command', '读取部署会话失败，尝试使用本地缓存', detail ?? message);
    }

    const mergedState = mergePersistedDeploymentSessionStates(
      fileState,
      localState,
    );

    if (!fileLoaded) {
      return mergedState;
    }

    const localHasFallbackState =
      localState.sessions.length > 0 || localState.activeProjectId !== null;
    const fileNeedsSync = !arePersistedDeploymentSessionStatesEqual(fileState, mergedState);

    if (fileNeedsSync) {
      try {
        if (mergedState.sessions.length === 0) {
          await clearDeploymentSessionFile(settings.projectWorkspacePath);
        } else {
          for (const session of mergedState.sessions) {
            await saveDeploymentSessionFile(settings.projectWorkspacePath, session);
          }
          await setDeploymentSessionActiveProjectFile(
            settings.projectWorkspacePath,
            mergedState.activeProjectId,
          );
        }
        clearDeploymentSession(settings.projectWorkspacePath);
      } catch (error) {
        const { message, detail } = describeUnknownError(error);
        engine.appendAppError('command', '迁移旧部署会话失败', detail ?? message);
        return mergedState;
      }
    } else if (localHasFallbackState) {
      clearDeploymentSession(settings.projectWorkspacePath);
    }

    return mergedState;
  }

  async function runDeploymentSnapshot(
    snapshot: Pick<
      DeploymentSnapshot,
      'projectId' | 'projectName' | 'environmentId' | 'environmentName' | 'runtimeAstText' | 'runtimeConnections'
    >,
    source: 'manual' | 'restore',
  ) {
    if (!hasTauriRuntime()) {
      const statusMessage =
        source === 'restore'
          ? `预览模式下已跳过 ${snapshot.projectName} 的自动恢复部署。`
          : `预览模式下已完成 ${snapshot.environmentName} 的部署校验。`;
      engine.setStatusMessage(statusMessage);
      engine.appendRuntimeLog(
        'project',
        'info',
        source === 'restore' ? '预览模式下跳过自动恢复部署' : '预览模式下跳过实际部署',
        `${snapshot.projectName} · ${snapshot.environmentName}`,
      );
      return true;
    }

    try {
      const response = await deployWorkflow(snapshot.runtimeAstText, snapshot.runtimeConnections, {
        workspacePath: settings.projectWorkspacePath,
        projectId: snapshot.projectId,
        projectName: snapshot.projectName,
        environmentId: snapshot.environmentId,
        environmentName: snapshot.environmentName,
        deploymentSource: source,
      }, {
        workflowId: snapshot.projectId,
      });
      engine.applyDeploymentState(
        response,
        source === 'restore'
          ? `已恢复 ${snapshot.projectName} 的部署，节点数 ${response.nodeCount}，边数 ${response.edgeCount}，环境 ${snapshot.environmentName}。`
          : `部署完成，节点数 ${response.nodeCount}，边数 ${response.edgeCount}，环境 ${snapshot.environmentName}。`,
      );
      await persistDeploymentSnapshot(snapshot);
      if (source === 'restore') {
        engine.appendRuntimeLog(
          'system',
          'success',
          '已恢复上次部署',
          `${snapshot.projectName} · ${snapshot.environmentName}`,
        );
      }
      await engine.refreshConnections();
      return true;
    } catch (error) {
      const { message, detail } = describeUnknownError(error);
      engine.appendAppError(
        'command',
        source === 'restore' ? '自动恢复部署失败' : '部署工作流失败',
        detail ?? message,
      );
      engine.setStatusMessage(
        source === 'restore' ? `自动恢复部署失败: ${message}` : message,
      );
      return false;
    }
  }

  async function handleSkipRestore() {
    if (pendingRestoreSessions.length === 0) {
      return;
    }

    const skippedSessions = pendingRestoreSessions;
    const leadSession = getPreferredRestoreSession(
      skippedSessions,
      pendingRestoreActiveProjectId,
    );
    restoreLookupRef.current = {
      scope: deploymentRestoreScope,
      status: 'handled',
    };
    setPendingRestoreSessions([]);
    setPendingRestoreActiveProjectId(null);
    setRestoreCountdown(10);
    await clearPersistedDeploymentSnapshots();
    engine.setStatusMessage(
      skippedSessions.length > 1
        ? `已取消恢复最近 ${skippedSessions.length} 个工程的上次部署。`
        : `已取消恢复 ${leadSession?.projectName ?? '当前工程'} 的上次部署。`,
    );
    engine.appendRuntimeLog(
      'system',
      'info',
      '已取消自动恢复部署',
      skippedSessions.length > 1
        ? `共 ${skippedSessions.length} 个工程`
        : `${leadSession?.projectName ?? '未知工程'} · ${leadSession?.environmentName ?? '默认环境'}`,
    );
  }

  async function handleConfirmRestore(sessions = pendingRestoreSessions) {
    const restoreSessions = sortPersistedDeploymentSessions(sessions);
    if (restoreSessions.length === 0) {
      return;
    }

    const validSessions: PersistedDeploymentSession[] = [];
    const missingSessions: PersistedDeploymentSession[] = [];

    for (const session of restoreSessions) {
      const targetProject = projectLibrary.projects.find((project) => project.id === session.projectId);
      if (targetProject) {
        validSessions.push(session);
      } else {
        missingSessions.push(session);
      }
    }

    restoreLookupRef.current = {
      scope: deploymentRestoreScope,
      status: 'handled',
    };
    setPendingRestoreSessions([]);
    setPendingRestoreActiveProjectId(null);
    setRestoreCountdown(10);

    for (const missingSession of missingSessions) {
      await removePersistedDeploymentSnapshot(missingSession.projectId);
      engine.appendRuntimeLog(
        'system',
        'warn',
        '恢复目标不存在，已清理部署记录',
        missingSession.projectName,
      );
    }

    if (validSessions.length === 0) {
      engine.setStatusMessage('恢复失败：目标工程不存在，已清理部署记录。');
      return;
    }

    const restoreState = normalizePersistedDeploymentSessionState({
      sessions: validSessions,
      activeProjectId: pendingRestoreActiveProjectId,
    });
    const leadSession = getPreferredRestoreSession(
      restoreState.sessions,
      restoreState.activeProjectId,
    );
    const restoreQueue = [
      ...[...restoreState.sessions.filter((session) => session.projectId !== leadSession?.projectId)].reverse(),
      ...(leadSession ? [leadSession] : []),
    ];
    engine.appendRuntimeLog(
      'system',
      'info',
      validSessions.length > 1 ? '正在批量恢复上次部署' : '正在恢复上次部署',
      validSessions.length > 1
        ? `共 ${validSessions.length} 个工程，主控工程为 ${leadSession?.projectName ?? validSessions[0].projectName}`
        : `${leadSession?.projectName ?? validSessions[0].projectName} · ${leadSession?.environmentName ?? validSessions[0].environmentName}`,
    );

    let restoredCount = 0;
    let lastSuccessfulSession: PersistedDeploymentSession | null = null;
    for (const session of restoreQueue) {
      const restored = await runDeploymentSnapshot(
        {
          projectId: session.projectId,
          projectName: session.projectName,
          environmentId: session.environmentId,
          environmentName: session.environmentName,
          runtimeAstText: session.runtimeAstText,
          runtimeConnections: session.runtimeConnections,
        },
        'restore',
      );
      if (restored) {
        restoredCount += 1;
        lastSuccessfulSession = session;
      }
    }

    if (lastSuccessfulSession) {
      setSidebarSection('boards');
      setActiveBoardId(lastSuccessfulSession.projectId);
    }

    if (validSessions.length > 1) {
      engine.appendRuntimeLog(
        'system',
        restoredCount === validSessions.length ? 'success' : 'warn',
        '批量恢复完成',
        `成功 ${restoredCount}/${validSessions.length} 个工程`,
      );
    }
  }

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
  const deploymentRestoreScope = settings.projectWorkspacePath.trim() || '__default__';
  const pendingRestoreLeadSession = getPreferredRestoreSession(
    pendingRestoreSessions,
    pendingRestoreActiveProjectId,
  );
  const sidebarSections = buildSidebarSections(
    workflowStatusLabel,
    runtimeWorkflowCount,
    engine.connections.length,
    engine.eventFeed.length + engine.appErrors.length,
    boardItems.length,
    currentBoardDeployInfo,
    activeBoard?.name ?? null,
  );

  function beginRestoreCheckPause() {
    restoreLookupRef.current = {
      scope: null,
      status: 'handled',
    };
    setIsRestoreCheckPaused(true);
    setPendingRestoreSessions([]);
    setPendingRestoreActiveProjectId(null);
    setRestoreCountdown(10);
  }

  function endRestoreCheckPause() {
    restoreLookupRef.current = {
      scope: deploymentRestoreScope,
      status: 'idle',
    };
    setIsRestoreCheckPaused(false);
  }

  useEffect(() => {
    restoreLookupRef.current = {
      scope: deploymentRestoreScope,
      status: 'idle',
    };
    setPendingRestoreSessions([]);
    setPendingRestoreActiveProjectId(null);
    setRestoreCountdown(10);
    setIsRestoreCheckPaused(false);
  }, [deploymentRestoreScope]);

  const handleConfirmRestoreRef = useRef(handleConfirmRestore);
  handleConfirmRestoreRef.current = handleConfirmRestore;

  useEffect(() => {
    if (pendingRestoreSessions.length === 0) {
      return;
    }

    if (restoreCountdown <= 0) {
      void handleConfirmRestoreRef.current(pendingRestoreSessions);
      return;
    }

    const timeoutId = window.setTimeout(() => {
      setRestoreCountdown((current) => current - 1);
    }, 1000);

    return () => {
      window.clearTimeout(timeoutId);
    };
  }, [pendingRestoreSessions, restoreCountdown]);

  useEffect(() => {
    if (pendingRestoreSessions.length === 0) {
      return;
    }

    const handleKeyDown = (event: KeyboardEvent) => {
      if (event.key === 'Escape') {
        void handleSkipRestore();
      }
    };

    window.addEventListener('keydown', handleKeyDown);
    return () => window.removeEventListener('keydown', handleKeyDown);
  }, [pendingRestoreSessions]);

  useEffect(() => {
    if (!engine.deployInfo || pendingRestoreSessions.length === 0) {
      return;
    }

    setPendingRestoreSessions([]);
    setPendingRestoreActiveProjectId(null);
    setRestoreCountdown(10);
  }, [engine.deployInfo, pendingRestoreSessions]);

  useEffect(() => {
    if (
      !hasTauriRuntime() ||
      !projectLibrary.storage.isReady ||
      !connectionLibrary.storage.isReady ||
      isRestoreCheckPaused ||
      engine.deployInfo ||
      runtimeWorkflowCount > 0
    ) {
      return;
    }

    if (restoreLookupRef.current.scope !== deploymentRestoreScope) {
      restoreLookupRef.current = {
        scope: deploymentRestoreScope,
        status: 'idle',
      };
    }

    if (restoreLookupRef.current.status !== 'idle') {
      return;
    }

    restoreLookupRef.current = {
      scope: deploymentRestoreScope,
      status: 'loading',
    };

    void loadPersistedDeploymentSnapshots().then((restoredState) => {
      if (restoreLookupRef.current.scope !== deploymentRestoreScope) {
        return;
      }

      if (restoredState.sessions.length === 0) {
        restoreLookupRef.current = {
          scope: deploymentRestoreScope,
          status: 'none',
        };
        return;
      }

      const knownSessions = restoredState.sessions.filter((session) =>
        projectLibrary.projects.some((project) => project.id === session.projectId),
      );
      const unknownSessions = restoredState.sessions.filter(
        (session) => !knownSessions.some((item) => item.projectId === session.projectId),
      );

      if (unknownSessions.length > 0) {
        for (const session of unknownSessions) {
          void removePersistedDeploymentSnapshot(session.projectId);
        }
        engine.appendRuntimeLog(
          'system',
          'warn',
          '已清理失效部署记录',
          unknownSessions.map((session) => session.projectName).join('、'),
        );
      }

      if (knownSessions.length === 0) {
        restoreLookupRef.current = {
          scope: deploymentRestoreScope,
          status: 'handled',
        };
        return;
      }

      const promptState = normalizePersistedDeploymentSessionState({
        sessions: knownSessions,
        activeProjectId: restoredState.activeProjectId,
      });
      const leadSession = getPreferredRestoreSession(
        promptState.sessions,
        promptState.activeProjectId,
      );
      engine.appendRuntimeLog(
        'system',
        'warn',
        '检测到可恢复部署',
        knownSessions.length > 1
          ? `共 ${knownSessions.length} 个工程，主控工程为 ${leadSession?.projectName ?? knownSessions[0].projectName}`
          : `${leadSession?.projectName ?? knownSessions[0].projectName} · ${leadSession?.environmentName ?? knownSessions[0].environmentName}`,
      );
      engine.setStatusMessage(
        knownSessions.length > 1
          ? `检测到 ${knownSessions.length} 个工程的上次部署，10 秒后将自动恢复。`
          : `检测到 ${leadSession?.projectName ?? knownSessions[0].projectName} 的上次部署，10 秒后将自动恢复。`,
      );
      restoreLookupRef.current = {
        scope: deploymentRestoreScope,
        status: 'prompted',
      };
      setPendingRestoreSessions(promptState.sessions);
      setPendingRestoreActiveProjectId(promptState.activeProjectId);
      setRestoreCountdown(10);
    });
  }, [
    connectionLibrary.storage.isReady,
    deploymentRestoreScope,
    engine,
    isRestoreCheckPaused,
    projectLibrary.projects,
    projectLibrary.storage.isReady,
    runtimeWorkflowCount,
    settings.projectWorkspacePath,
  ]);

  const handleAiConfigSave = async (update: AiConfigUpdate) => {
    try {
      const saved = await saveAiConfig(update);
      setAiConfig(saved);
      setAiConfigError(null);
    } catch (error) {
      setAiConfigError(describeUnknownError(error).message);
    }
  };

  const handleAiProviderTest = async (draft: AiProviderDraft) => {
    setAiTesting(true);
    setAiTestResult(null);
    try {
      const result = await testAiProvider(draft);
      setAiTestResult(result);
    } catch (error) {
      setAiTestResult({
        success: false,
        message: describeUnknownError(error).message,
      });
    } finally {
      setAiTesting(false);
    }
  };

  function renderRestoreDialog() {
    if (pendingRestoreSessions.length === 0 || !pendingRestoreLeadSession) {
      return null;
    }

    const progress = `${Math.max(0, Math.min(100, (restoreCountdown / 10) * 100))}%`;

    return (
      <div className="restore-dialog-layer" data-no-window-drag>
        <div
          className="restore-dialog"
          role="alertdialog"
          aria-modal="true"
          aria-labelledby="restore-dialog-title"
          aria-describedby="restore-dialog-description"
          onClick={(event) => event.stopPropagation()}
        >
          <div className="restore-dialog__header">
            <div className="restore-dialog__eyebrow">
              <span className="restore-dialog__eyebrow-dot" />
              <span>启动恢复</span>
            </div>
            <span className="restore-dialog__timer">{restoreCountdown}s</span>
          </div>

          <div className="restore-dialog__body">
            <strong id="restore-dialog-title">
              {pendingRestoreSessions.length > 1
                ? `恢复最近 ${pendingRestoreSessions.length} 个工程的上次部署？`
                : `恢复“${pendingRestoreLeadSession.projectName}”的上次部署？`}
            </strong>
            <p id="restore-dialog-description">
              {pendingRestoreSessions.length > 1
                ? `将批量恢复最近 ${pendingRestoreSessions.length} 个工程的成功部署，并以“${pendingRestoreLeadSession.projectName}”作为当前工程。若不操作，${restoreCountdown} 秒后自动恢复。`
                : `将恢复环境“${pendingRestoreLeadSession.environmentName}”下的最后一次成功部署。若不操作，${restoreCountdown} 秒后自动恢复。`}
            </p>
          </div>

          <div className="restore-dialog__countdown" aria-hidden="true">
            <div className="restore-dialog__countdown-bar" style={{ width: progress }} />
          </div>

          <div className="restore-dialog__actions">
            <button
              type="button"
              className="restore-dialog__action"
              onClick={() => {
                void handleSkipRestore();
              }}
            >
              不恢复
            </button>
            <button
              type="button"
              className="restore-dialog__action restore-dialog__action--primary"
              onClick={() => {
                void handleConfirmRestore();
              }}
            >
              立即恢复
            </button>
          </div>
        </div>
      </div>
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
              onImportBoardFile={handleImportBoardFile}
              onDeleteBoard={handleDeleteBoard}
            />
          </div>
        </section>
      );
    }

    return (
      <section className="studio-content studio-content--board">
        <div
          className={`studio-board-workspace ${engine.isRuntimeDockCollapsed ? 'is-runtime-collapsed' : ''}`}
        >
          <ProjectWorkspaceHeader
            project={activeProject}
            nodeCount={graphNodeCount}
            onBack={handleBackToBoards}
            onSave={handleSaveProject}
            onExport={() => handleExportBoard(activeProject.id)}
            onCreateSnapshot={handleCreateSnapshot}
            onRollbackSnapshot={handleRollbackSnapshot}
            onEnvironmentChange={handleEnvironmentChange}
            onEnvironmentSave={handleEnvironmentSave}
            onDuplicateEnvironment={handleDuplicateEnvironment}
            onDeleteEnvironment={handleDeleteEnvironment}
          />

          <FlowgramCanvas
            ref={flowgramCanvasRef}
            graph={graphState.graph}
            connections={connectionLibrary.connections}
            aiProviders={aiConfig?.providers ?? []}
            activeAiProviderId={aiConfig?.activeProviderId ?? null}
            copilotParams={aiConfig?.copilotParams ?? {}}
            runtimeState={engine.runtimeState}
            workflowStatus={workflowStatus}
            accentHex={settings.accentHex}
            nodeRhaiColor={settings.accentThemeVariables['--node-rhai']}
            onRunRequested={handleDeploy}
            onStopRequested={handleUndeploy}
            onDispatchRequested={handleDispatchPayload}
            canDispatchPayload={canDispatchPayload}
            onGraphChange={handleGraphChange}
            onError={engine.handleFlowgramError}
          />

          <RuntimeDock
            eventFeed={engine.eventFeed}
            appErrors={engine.appErrors}
            results={engine.results}
            connectionPreview={connectionPreview}
            themeMode={settings.themeMode}
            isCollapsed={engine.isRuntimeDockCollapsed}
            onToggleCollapsed={() => engine.setIsRuntimeDockCollapsed((current) => !current)}
          />
        </div>
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
                projectWorkspaceLibraryFilePath={projectLibrary.storage.libraryFilePath}
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
      {renderRestoreDialog()}
    </main>
  );
}

export default App;
