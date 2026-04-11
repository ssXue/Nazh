import { useEffect, useMemo, useRef, useState } from 'react';

import { useProjectLibrary } from './hooks/use-project-library';
import { useSettings } from './hooks/use-settings';
import { useWorkflowEngine } from './hooks/use-workflow-engine';
import { useConnectionLibrary } from './hooks/use-connection-library';

import { AboutPanel } from './components/app/AboutPanel';
import { BoardsPanel, type BoardItem } from './components/app/BoardsPanel';
import { DashboardPanel } from './components/app/DashboardPanel';
import { LogsPanel } from './components/app/LogsPanel';
import { PayloadPanel } from './components/app/PayloadPanel';
import { ProjectWorkspaceHeader } from './components/app/ProjectWorkspaceHeader';
import { RuntimeDock } from './components/app/RuntimeDock';
import { SettingsPanel } from './components/app/SettingsPanel';
import { SidebarNav } from './components/app/SidebarNav';
import type { SidebarSection } from './components/app/types';
import { ConnectionStudio } from './components/ConnectionStudio';
import { FlowgramCanvas, type FlowgramCanvasHandle } from './components/FlowgramCanvas';
import { parseWorkflowGraph } from './lib/graph';
import { formatWorkflowGraph } from './lib/flowgram';
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
  deployWorkflow,
  dispatchPayload,
  hasTauriRuntime,
  undeployWorkflow,
} from './lib/tauri';
import type { ConnectionDefinition, WorkflowResult, WorkflowNodeDefinition } from './types';
import { describeUnknownError } from './lib/workflow-events';
import {
  deriveWorkflowStatus,
  getWorkflowStatusLabel,
  getWorkflowStatusPillClass,
} from './lib/workflow-status';

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
  const flowgramCanvasRef = useRef<FlowgramCanvasHandle | null>(null);

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

    const nextAstText = formatWorkflowGraph(currentGraph);
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
        astText: null,
        runtimeAstText: null,
        runtimeConnections: null,
        error: '请先从所有看板进入工程。',
      };
    }

    if (!connectionLibrary.storage.isReady) {
      return {
        astText: null,
        runtimeAstText: null,
        runtimeConnections: null,
        error: '连接资源仍在加载，请稍后再试。',
      };
    }

    const draftSnapshot = buildProjectDraftSnapshot(activeProject.id);
    if (draftSnapshot.error || !draftSnapshot.graph || !draftSnapshot.astText) {
      return {
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
    const runtimeAstText = formatWorkflowGraph(runtimeGraph);
    const nextGraphState = parseWorkflowGraph(runtimeAstText);
    if (nextGraphState.error || !nextGraphState.graph) {
      return {
        astText: null,
        runtimeAstText: null,
        runtimeConnections: null,
        error: nextGraphState.error ?? '环境覆盖后的工作流无法序列化。',
      };
    }

    return {
      astText: draftSnapshot.astText,
      runtimeAstText,
      runtimeConnections,
      error: null,
    };
  }

  async function handleDeploy() {
    if (!activeBoard || !activeProject) {
      engine.setStatusMessage('请先从所有看板进入工程。');
      return;
    }

    const deploySnapshot = buildDeploySnapshot();
    if (
      deploySnapshot.error ||
      !deploySnapshot.astText ||
      !deploySnapshot.runtimeAstText ||
      !deploySnapshot.runtimeConnections
    ) {
      engine.appendAppError('command', '部署前 AST 校验失败', deploySnapshot.error ?? '未知错误');
      engine.setStatusMessage(`AST 无法部署: ${deploySnapshot.error ?? '未知错误'}`);
      return;
    }

    if (deploySnapshot.astText !== activeProject.astText) {
      updateProjectDraft(activeProject.id, { astText: deploySnapshot.astText });
    }

    const environmentName = getActiveEnvironment(activeProject)?.name ?? '默认环境';

    if (!hasTauriRuntime()) {
      engine.setStatusMessage(`预览模式下已完成 ${environmentName} 的部署校验。`);
      engine.appendRuntimeLog('project', 'info', '预览模式下跳过实际部署', environmentName);
      return;
    }

    try {
      const response = await deployWorkflow(
        deploySnapshot.runtimeAstText,
        deploySnapshot.runtimeConnections,
      );
      engine.setStatusMessage(
        `部署完成，节点数 ${response.nodeCount}，边数 ${response.edgeCount}，环境 ${environmentName}。`,
      );
      await engine.refreshConnections();
    } catch (error) {
      const { message, detail } = describeUnknownError(error);
      engine.appendAppError('command', '部署工作流失败', detail ?? message);
      engine.setStatusMessage(message);
    }
  }

  async function handleUndeploy() {
    if (!activeBoard) {
      engine.setStatusMessage('请先从所有看板进入工程。');
      return;
    }

    if (!hasTauriRuntime()) {
      engine.resetWorkspaceRuntime('已在预览态清空部署状态。');
      engine.appendRuntimeLog('system', 'info', '预览态已清空部署状态');
      return;
    }

    try {
      const response = await undeployWorkflow();
      if (!response.hadWorkflow) {
        engine.setStatusMessage('当前没有已部署工作流。');
      }
    } catch (error) {
      const { message, detail } = describeUnknownError(error);
      engine.appendAppError('command', '反部署失败', detail ?? message);
      engine.setStatusMessage(message);
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

    if (!engine.deployInfo) {
      engine.appendAppError('command', '发送测试载荷失败', '请先部署工作流');
      engine.setStatusMessage('请先部署工作流，再发送测试消息。');
      return;
    }

    try {
      const response = await dispatchPayload(payload);
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
  const workflowStatus = deriveWorkflowStatus(
    isTauriRuntime,
    Boolean(activeBoard),
    engine.deployInfo,
    engine.runtimeState,
  );
  const workflowStatusLabel = getWorkflowStatusLabel(workflowStatus);
  const workflowStatusPillClass = getWorkflowStatusPillClass(workflowStatus);
  const canDispatchPayload = Boolean(activeBoard) && (!isTauriRuntime || Boolean(engine.deployInfo));
  const connectionPreview = engine.connections.slice(0, 4);
  const sidebarSections = buildSidebarSections(
    workflowStatusLabel,
    engine.connections.length,
    engine.eventFeed.length + engine.appErrors.length,
    boardItems.length,
    engine.deployInfo,
    activeBoard?.name ?? null,
  );

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
                deployInfo={engine.deployInfo}
                onNavigateToBoards={handleBackToBoards}
              />
            </div>
          </section>
        );
      case 'boards':
        return renderBoardWorkspace();
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
      case 'payload':
        if (!activeBoard) {
          return renderProjectGate('测试载荷');
        }

        return (
          <section className="studio-content studio-content--panel">
            <div className="panel studio-content__panel studio-content__panel--editor">
              <PayloadPanel
                payloadText={payloadText}
                deployInfo={engine.deployInfo}
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
    </main>
  );
}

export default App;
