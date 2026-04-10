import { useMemo, useRef, useState } from 'react';

import { useSettings } from './hooks/use-settings';
import { useWorkflowEngine } from './hooks/use-workflow-engine';

import { AboutPanel } from './components/app/AboutPanel';
import {
  BackIcon,
} from './components/app/AppIcons';
import { BOARD_LIBRARY, BoardsPanel, type BoardItem } from './components/app/BoardsPanel';
import { DashboardPanel } from './components/app/DashboardPanel';
import { PayloadPanel } from './components/app/PayloadPanel';
import { RuntimeDock } from './components/app/RuntimeDock';
import { SettingsPanel } from './components/app/SettingsPanel';
import { SidebarNav } from './components/app/SidebarNav';
import { SourcePanel } from './components/app/SourcePanel';
import type { SidebarSection } from './components/app/types';
import { ConnectionStudio } from './components/ConnectionStudio';
import { FlowgramCanvas, type FlowgramCanvasHandle } from './components/FlowgramCanvas';
import { buildInitialProjectDrafts, buildProjectAst, CURRENT_USER_NAME, DEFAULT_BOARD_ID, type ProjectDraft } from './lib/demo-data';
import { parseWorkflowGraph } from './lib/graph';
import { formatWorkflowGraph } from './lib/flowgram';
import { buildSidebarSections } from './lib/sidebar';
import { ACCENT_PRESET_OPTIONS } from './lib/theme';
import {
  deployWorkflow,
  dispatchPayload,
  hasTauriRuntime,
  undeployWorkflow,
} from './lib/tauri';
import { SAMPLE_AST, SAMPLE_PAYLOAD } from './types';
import type { WorkflowResult } from './types';
import {
  describeUnknownError,
} from './lib/workflow-events';
import {
  deriveWorkflowStatus,
  getWorkflowStatusLabel,
  getWorkflowStatusPillClass,
} from './lib/workflow-status';

function App() {
  // 偏好设置状态（主题、强调色、密度、动效、启动页）由 useSettings 统一管理。
  const settings = useSettings();

  const [projectDrafts, setProjectDrafts] = useState<Record<string, ProjectDraft>>(
    buildInitialProjectDrafts,
  );
  const [activeBoard, setActiveBoard] = useState<BoardItem | null>(null);
  const [sidebarSection, setSidebarSection] = useState<SidebarSection>(settings.startupPage);
  const flowgramCanvasRef = useRef<FlowgramCanvasHandle | null>(null);

  // 工作流生命周期状态与副作用由 useWorkflowEngine 统一管理。
  const engine = useWorkflowEngine(activeBoard, sidebarSection);

  const currentBoardId = activeBoard?.id ?? DEFAULT_BOARD_ID;
  const currentProject = projectDrafts[currentBoardId] ?? {
    astText: SAMPLE_AST,
    payloadText: SAMPLE_PAYLOAD,
  };
  const astText = currentProject.astText;
  const payloadText = currentProject.payloadText;
  const graphState = useMemo(() => parseWorkflowGraph(astText), [astText]);

  function updateProjectDraft(boardId: string, nextDraft: Partial<ProjectDraft>) {
    setProjectDrafts((current) => ({
      ...current,
      [boardId]: {
        ...(current[boardId] ?? {
          astText: buildProjectAst(activeBoard?.id ?? DEFAULT_BOARD_ID, activeBoard?.name ?? '默认工作流'),
          payloadText: SAMPLE_PAYLOAD,
        }),
        ...nextDraft,
      },
    }));
  }

  function handleAstTextChange(nextText: string) {
    if (!activeBoard) {
      return;
    }

    updateProjectDraft(activeBoard.id, { astText: nextText });

    const nextGraphState = parseWorkflowGraph(nextText);
    if (!nextGraphState.error && nextGraphState.graph) {
      flowgramCanvasRef.current?.loadWorkflowGraph(nextGraphState.graph);
    }
  }

  function applyStructuredGraphChange(nextAstText: string, nextStatusMessage: string) {
    if (!activeBoard || nextAstText === astText) {
      return;
    }

    updateProjectDraft(activeBoard.id, { astText: nextAstText });
    engine.setStatusMessage(nextStatusMessage);
  }

  function handleGraphChange(nextAstText: string) {
    applyStructuredGraphChange(nextAstText, '画布变更已同步回 AST 文本。');
  }

  function handleConnectionGraphChange(nextAstText: string, nextStatusMessage: string) {
    applyStructuredGraphChange(nextAstText, nextStatusMessage);
  }

  function handlePayloadTextChange(nextText: string) {
    if (!activeBoard) {
      return;
    }

    updateProjectDraft(activeBoard.id, { payloadText: nextText });
  }

  function handleOpenBoard(board: BoardItem) {
    setActiveBoard(board);
    setSidebarSection('boards');
    engine.resetWorkspaceRuntime(`已进入工程 ${board.name}。`);
  }

  function handleBackToBoards() {
    setSidebarSection('boards');
    setActiveBoard(null);
    engine.resetWorkspaceRuntime('已返回所有看板。');
  }

  function buildDeploySnapshot() {
    const sourceGraphState = parseWorkflowGraph(astText);
    if (sourceGraphState.error) {
      return {
        graph: null,
        astText: null,
        error: sourceGraphState.error,
      };
    }

    const currentGraph =
      flowgramCanvasRef.current?.getCurrentWorkflowGraph() ?? sourceGraphState.graph;
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

  async function handleDeploy() {
    if (!activeBoard) {
      engine.setStatusMessage('请先从所有看板进入工程。');
      return;
    }

    const deploySnapshot = buildDeploySnapshot();
    if (deploySnapshot.error || !deploySnapshot.astText) {
      engine.appendAppError('command', '部署前 AST 校验失败', deploySnapshot.error ?? '未知错误');
      engine.setStatusMessage(`AST 无法部署: ${deploySnapshot.error ?? '未知错误'}`);
      return;
    }

    if (deploySnapshot.astText !== astText) {
      updateProjectDraft(activeBoard.id, { astText: deploySnapshot.astText });
    }

    if (!hasTauriRuntime()) {
      engine.setStatusMessage('纯 Web 预览模式下不会实际调用后端，已完成 AST 结构校验。');
      engine.appendRuntimeLog('system', 'info', '纯 Web 预览模式下跳过实际部署');
      return;
    }

    try {
      const response = await deployWorkflow(deploySnapshot.astText);
      engine.setStatusMessage(`部署完成，节点数 ${response.nodeCount}，边数 ${response.edgeCount}。`);
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
      engine.appendRuntimeLog('dispatch', 'info', `已提交测试载荷`, `trace_id=${response.traceId}`);
      engine.setStatusMessage(`已提交 payload，trace_id=${response.traceId}`);
    } catch (error) {
      const { message, detail } = describeUnknownError(error);
      engine.appendAppError('command', '发送测试载荷失败', detail ?? message);
      engine.setStatusMessage(message);
    }
  }

  const graphNodeCount = graphState.graph ? Object.keys(graphState.graph.nodes).length : 0;
  const graphEdgeCount = graphState.graph?.edges.length ?? 0;
  const graphConnectionCount = graphState.graph?.connections?.length ?? 0;
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
    graphState.error,
    graphConnectionCount,
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
    if (!activeBoard) {
      return (
        <section className="studio-content studio-content--panel">
          <div className="panel studio-content__panel studio-content__panel--scroll">
            <BoardsPanel onOpenBoard={handleOpenBoard} />
          </div>
        </section>
      );
    }

    return (
      <section className="studio-content studio-content--board">
        <div
          className={`studio-board-workspace ${engine.isRuntimeDockCollapsed ? 'is-runtime-collapsed' : ''}`}
        >
          <div
            className="studio-board-workspace__header window-safe-header"
            data-window-drag-region
          >
            <div className="studio-board-workspace__header-main">
              <div className="studio-board-workspace__header-heading">
                <button
                  type="button"
                  className="studio-board-workspace__back"
                  onClick={handleBackToBoards}
                  aria-label="返回所有看板"
                  title="返回所有看板"
                >
                  <BackIcon />
                </button>
                <h2>{activeBoard.name}</h2>
              </div>
              <span>{`${activeBoard.updatedAt} · ${graphNodeCount} 节点`}</span>
            </div>
          </div>

          <FlowgramCanvas
            ref={flowgramCanvasRef}
            graph={graphState.graph}
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
                boardCount={BOARD_LIBRARY.length}
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
      case 'source':
        if (!activeBoard) {
          return renderProjectGate('流程源配置');
        }

        return (
          <section className="studio-content studio-content--panel">
            <div className="panel studio-content__panel studio-content__panel--editor">
              <SourcePanel
                astText={astText}
                graphError={graphState.error}
                onAstTextChange={handleAstTextChange}
              />
            </div>
          </section>
        );
      case 'connections':
        if (!activeBoard) {
          return renderProjectGate('连接资源');
        }

        return (
          <section className="studio-content studio-content--panel">
            <div className="panel studio-content__panel studio-content__panel--scroll panel--connection-card">
              <ConnectionStudio
                graph={graphState.graph}
                astError={graphState.error}
                runtimeConnections={engine.connections}
                onGraphChange={handleConnectionGraphChange}
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
                densityMode={settings.densityMode}
                onDensityModeChange={settings.setDensityMode}
                motionMode={settings.motionMode}
                onMotionModeChange={settings.setMotionMode}
                startupPage={settings.startupPage}
                onStartupPageChange={settings.setStartupPage}
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
