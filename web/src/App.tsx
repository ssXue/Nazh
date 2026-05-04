import { StrictMode, useCallback, useEffect, useMemo, useRef, useState } from 'react';

import { useAiConfigState } from './hooks/use-ai-config-state';
import { useAiWorkflowComposerState } from './hooks/use-ai-workflow-composer-state';
import { useAppNavigation } from './hooks/use-app-navigation';
import { useDeploymentRestore } from './hooks/use-deployment-restore';
import { useProjectLibrary } from './hooks/use-project-library';
import { useProjectWorkspaceActions } from './hooks/use-project-workspace-actions';
import { useRuntimeWorkflowCount } from './hooks/use-runtime-workflow-count';
import { useSettings } from './hooks/use-settings';
import { useTestRun } from './hooks/use-test-run';
import { useWorkflowEngine } from './hooks/use-workflow-engine';
import { useConnectionLibrary } from './hooks/use-connection-library';

import { AiWorkflowComposer } from './components/app/AiWorkflowComposer';
import { SidebarToggleIcon } from './components/app/AppIcons';
import type { BoardWorkspaceHandle } from './components/app/BoardWorkspace';
import type { BoardItem } from './components/app/BoardsPanel';
import { RestoreDeploymentDialog } from './components/app/RestoreDeploymentDialog';
import { SidebarNav } from './components/app/SidebarNav';
import { StudioContentRouter } from './components/app/StudioContentRouter';
import { parseWorkflowGraph } from './lib/graph';
import { formatWorkflowGraph } from './lib/flowgram';
import {
  applyEnvironmentToConnectionDefinitions,
  CURRENT_USER_NAME,
  formatRelativeTimestamp,
  getActiveEnvironment,
  parseProjectNodeCount,
} from './lib/projects';
import { buildSidebarSections } from './lib/sidebar';
import { applyGlobalAiConfigToWorkflowGraph } from './lib/workflow-ai';
import {
  hasTauriRuntime,
  listRuntimeWorkflows,
  undeployWorkflow,
} from './lib/tauri';
import type { ConnectionDefinition, DeployResponse, WorkflowResult, WorkflowNodeDefinition } from './types';
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
  const { runtimeWorkflowCount, setRuntimeWorkflowCount } = useRuntimeWorkflowCount(
    settings.projectWorkspacePath,
  );
  const flowgramCanvasRef = useRef<BoardWorkspaceHandle>(null);

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
  const [sidebarCollapsed, setSidebarCollapsed] = useState(false);
  const toggleSidebarCollapsed = useCallback(() => setSidebarCollapsed((prev) => !prev), []);
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

  const projectActions = useProjectWorkspaceActions({
    activeBoardId,
    activeProject,
    clearActiveBoard,
    engine,
    flowgramCanvasRef,
    openBoard,
    projectLibrary,
    setSidebarCollapsed,
  });

  const aiWorkflowComposer = useAiWorkflowComposerState({
    activeBoardId,
    activeProject,
    aiConfig,
    appendAppError: engine.appendAppError,
    appendRuntimeLog: engine.appendRuntimeLog,
    createProject: projectLibrary.createProject,
    flowgramCanvasRef,
    openBoard,
    projectCount: projectLibrary.projects.length,
    resetWorkspaceRuntime: engine.resetWorkspaceRuntime,
    setStatusMessage: engine.setStatusMessage,
    updateProjectDraft: projectActions.updateProjectDraft,
  });

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

    const draftSnapshot = projectActions.buildProjectDraftSnapshot(activeProject.id);
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
    testRun.reset();

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
      projectActions.updateProjectDraft(activeProject.id, { astText: nextDeploySnapshot.astText });
    }

    await runDeploymentSnapshot(nextDeploySnapshot.snapshot, 'manual');
  }

  async function handleUndeploy() {
    testRun.reset();

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

  const currentBoardDeployInfoRef = useRef<DeployResponse | null>(null);

  const testRun = useTestRun({
    getPayloadText: () => payloadText,
    buildAndDeploy: async () => {
      const snap = buildDeploySnapshot();
      if (snap.error || !snap.snapshot) {
        engine.appendAppError('command', '测试运行部署失败', snap.error ?? '未知错误');
        engine.setStatusMessage(`AST 无法部署: ${snap.error ?? '未知错误'}`);
        return false;
      }
      await runDeploymentSnapshot(snap.snapshot, 'manual');
      return true;
    },
    undeploy: handleUndeploy,
    beginRestoreCheckPause,
    endRestoreCheckPause,
    appendRuntimeLog: engine.appendRuntimeLog,
    appendAppError: engine.appendAppError,
    setStatusMessage: engine.setStatusMessage,
    addPreviewResult: (payload) => {
      engine.addResult({
        trace_id: 'web-preview',
        timestamp: new Date().toISOString(),
        payload: payload as WorkflowResult['payload'],
      });
    },
    getActiveBoardId: () => activeBoard?.id,
    isCurrentlyDeployed: () => Boolean(currentBoardDeployInfoRef.current),
    hasActiveBoard: () => Boolean(activeBoard),
    hasActiveProject: () => Boolean(activeProject),
  });

  const graphNodeCount = graphState.graph ? Object.keys(graphState.graph.nodes).length : 0;
  const graphEdgeCount = graphState.graph?.edges.length ?? 0;
  const graphConnectionCount = getReferencedConnectionCount(graphState.graph?.nodes ?? null);
  const isTauriRuntime = hasTauriRuntime();
  const currentUserRole = isTauriRuntime ? '桌面操作员' : '预览访客';
  const runtimeModeLabel = isTauriRuntime ? '桌面会话' : '浏览器预览';
  const currentBoardDeployInfo =
    activeBoard && getDeployProjectId(engine.deployInfo) === activeBoard.id ? engine.deployInfo : null;
  currentBoardDeployInfoRef.current = currentBoardDeployInfo;
  const workflowStatus = deriveWorkflowStatus(
    isTauriRuntime,
    Boolean(activeBoard),
    currentBoardDeployInfo,
    engine.runtimeState,
  );
  const workflowStatusLabel = getWorkflowStatusLabel(workflowStatus);
  const workflowStatusPillClass = getWorkflowStatusPillClass(workflowStatus);
  const canTestRun = testRun.canTestRun;
  const isTestRunning = testRun.isTestRunning;
  const connectionPreview = engine.connections.slice(0, 4);
  const sidebarSections = buildSidebarSections(
    workflowStatusLabel,
    runtimeWorkflowCount,
    engine.connections.length,
    engine.eventFeed.length + engine.appErrors.length,
    boardItems.length,
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

  return (
    <main className="app-shell app-shell--studio">
      <section className="studio-frame">
        <button
          type="button"
          className="studio-nav-toggle"
          aria-label={sidebarCollapsed ? '打开导航' : '收起导航'}
          title={sidebarCollapsed ? '打开导航栏' : '收起导航栏'}
          onClick={toggleSidebarCollapsed}
        >
          <SidebarToggleIcon />
        </button>
        <aside className={`studio-nav-sidebar${sidebarCollapsed ? ' is-collapsed' : ''}`}>
          <StrictMode>
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
              isCollapsed={sidebarCollapsed}
              onToggleCollapsed={toggleSidebarCollapsed}
            />
          </StrictMode>
        </aside>

        <StudioContentRouter
          activeBoard={activeBoard}
          activeProject={activeProject}
          aiActionDisabled={aiWorkflowComposer.actionDisabled}
          aiActionLoadingCreate={aiWorkflowComposer.generating && aiWorkflowComposer.mode === 'create'}
          aiActionLoadingEdit={aiWorkflowComposer.generating && aiWorkflowComposer.mode === 'edit'}
          aiActionTitle={aiWorkflowComposer.actionTitle}
          aiConfig={aiConfig}
          aiConfigError={aiConfigError}
          aiConfigLoading={aiConfigLoading}
          aiTestResult={aiTestResult}
          aiTesting={aiTesting}
          boardItems={boardItems}
          canTestRun={canTestRun}
          connectionLibrary={connectionLibrary}
          connectionPreview={connectionPreview}
          connectionUsageById={connectionUsageById}
          currentBoardDeployInfo={currentBoardDeployInfo}
          engine={engine}
          flowgramCanvasRef={flowgramCanvasRef}
          graph={graphState.graph}
          graphConnectionCount={graphConnectionCount}
          graphEdgeCount={graphEdgeCount}
          graphNodeCount={graphNodeCount}
          isTauriRuntime={isTauriRuntime}
          payloadText={payloadText}
          projectLibrary={projectLibrary}
          runtimeModeLabel={runtimeModeLabel}
          section={sidebarSection}
          settings={settings}
          workflowStatus={workflowStatus}
          workflowStatusLabel={workflowStatusLabel}
          onAfterWorkflowStop={endRestoreCheckPause}
          onBackToBoards={projectActions.handleBackToBoards}
          onBeforeWorkflowStop={beginRestoreCheckPause}
          onCreateBoard={projectActions.handleCreateBoard}
          onCreateSnapshot={projectActions.handleCreateSnapshot}
          onDeleteBoard={projectActions.handleDeleteBoard}
          onDeleteEnvironment={projectActions.handleDeleteEnvironment}
          onDeleteSnapshot={projectActions.handleDeleteSnapshot}
          onTestRun={testRun.handleTestRun}
          onDuplicateEnvironment={projectActions.handleDuplicateEnvironment}
          onEnvironmentChange={projectActions.handleEnvironmentChange}
          onEnvironmentSave={projectActions.handleEnvironmentSave}
          onGraphChange={projectActions.handleGraphChange}
          onImportBoardFile={projectActions.handleImportBoardFile}
          onOpenAiCreate={aiWorkflowComposer.openCreate}
          onOpenAiEdit={aiWorkflowComposer.openEdit}
          onOpenBoard={projectActions.handleOpenBoard}
          onPayloadTextChange={projectActions.handlePayloadTextChange}
          onPersistActiveProject={persistActiveDeploymentProject}
          onRemovePersistedDeployment={removePersistedDeploymentSnapshot}
          onRollbackSnapshot={projectActions.handleRollbackSnapshot}
          onRuntimeCountChange={setRuntimeWorkflowCount}
          onSectionChange={setSidebarSection}
          onStartDeploy={handleDeploy}
          onStopDeploy={handleUndeploy}
          onAiConfigSave={handleAiConfigSave}
          onAiProviderTest={handleAiProviderTest}
        />
      </section>
      <AiWorkflowComposer {...aiWorkflowComposer.composerProps} />
      {renderRestoreDialog()}
    </main>
  );
}

export default App;
