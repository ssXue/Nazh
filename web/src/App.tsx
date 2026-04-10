import { useEffect, useMemo, useRef, useState } from 'react';

import { useSettings } from './hooks/use-settings';

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
  enableAdaptiveWindowSizing,
  hasTauriRuntime,
  listConnections,
  onWorkflowDeployed,
  onWorkflowEvent,
  onWorkflowResult,
  onWorkflowUndeployed,
  undeployWorkflow,
} from './lib/tauri';
import type {
  AppErrorRecord,
  ConnectionRecord,
  DeployResponse,
  RuntimeLogEntry,
  WorkflowResult,
  WorkflowRuntimeState,
} from './types';
import { SAMPLE_AST, SAMPLE_PAYLOAD } from './types';
import {
  buildAppErrorRecord,
  buildRuntimeLogEntry,
  describeUnknownError,
  EMPTY_RUNTIME_STATE,
  parseWorkflowEventPayload,
  reduceRuntimeState,
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
  const [statusMessage, setStatusMessage] = useState(
    hasTauriRuntime()
      ? '等待进入工程。'
      : '当前运行在纯 Web 预览模式，调用 Tauri 命令会被跳过。',
  );
  const [deployInfo, setDeployInfo] = useState<DeployResponse | null>(null);
  const [results, setResults] = useState<WorkflowResult[]>([]);
  const [eventFeed, setEventFeed] = useState<RuntimeLogEntry[]>([]);
  const [appErrors, setAppErrors] = useState<AppErrorRecord[]>([]);
  const [connections, setConnections] = useState<ConnectionRecord[]>([]);
  const [runtimeState, setRuntimeState] = useState<WorkflowRuntimeState>(EMPTY_RUNTIME_STATE);
  const [isRuntimeDockCollapsed, setIsRuntimeDockCollapsed] = useState(false);
  const flowgramCanvasRef = useRef<FlowgramCanvasHandle | null>(null);

  const currentBoardId = activeBoard?.id ?? DEFAULT_BOARD_ID;
  const currentProject = projectDrafts[currentBoardId] ?? {
    astText: SAMPLE_AST,
    payloadText: SAMPLE_PAYLOAD,
  };
  const astText = currentProject.astText;
  const payloadText = currentProject.payloadText;
  const graphState = useMemo(() => parseWorkflowGraph(astText), [astText]);

  function appendRuntimeLog(
    source: string,
    level: RuntimeLogEntry['level'],
    message: string,
    detail?: string | null,
  ) {
    const nextEntry = buildRuntimeLogEntry(source, level, message, detail);
    setEventFeed((current) => [...current, nextEntry].slice(-180));
  }

  function appendAppError(
    scope: AppErrorRecord['scope'],
    title: string,
    detail?: string | null,
  ) {
    const nextError = buildAppErrorRecord(scope, title, detail);
    setAppErrors((current) => [...current, nextError].slice(-24));
  }

  function handleFlowgramError(title: string, detail?: string | null) {
    appendAppError('frontend', title, detail);
    appendRuntimeLog('flowgram', 'error', title, detail);
    setStatusMessage(title);
  }

  useEffect(() => {
    setIsRuntimeDockCollapsed(false);
  }, [activeBoard?.id]);

  useEffect(() => {
    if (!hasTauriRuntime() || !activeBoard) {
      return;
    }

    if (sidebarSection !== 'connections' && !deployInfo) {
      return;
    }

    void refreshConnections();
  }, [activeBoard?.id, deployInfo, sidebarSection]);

  useEffect(() => {
    if (!hasTauriRuntime()) {
      return;
    }

    let cleanup = () => {};

    void enableAdaptiveWindowSizing().then((nextCleanup) => {
      cleanup = nextCleanup;
    });

    return () => {
      cleanup();
    };
  }, []);

  useEffect(() => {
    if (typeof window === 'undefined') {
      return;
    }

    const handleWindowError = (event: ErrorEvent) => {
      const message = event.message || '前端运行时异常';
      const detail =
        event.error instanceof Error
          ? event.error.stack ?? event.error.message
          : [event.filename, event.lineno, event.colno].filter(Boolean).join(':') || null;

      appendAppError('frontend', message, detail);
      setStatusMessage(`前端异常已捕获: ${message}`);
      event.preventDefault();
    };

    const handleUnhandledRejection = (event: PromiseRejectionEvent) => {
      const { message, detail } = describeUnknownError(event.reason);
      appendAppError('runtime', `未处理的 Promise 异常: ${message}`, detail);
      setStatusMessage(`未处理的 Promise 异常: ${message}`);
      event.preventDefault();
    };

    window.addEventListener('error', handleWindowError);
    window.addEventListener('unhandledrejection', handleUnhandledRejection);

    return () => {
      window.removeEventListener('error', handleWindowError);
      window.removeEventListener('unhandledrejection', handleUnhandledRejection);
    };
  }, []);

  useEffect(() => {
    if (!hasTauriRuntime()) {
      return;
    }

    let alive = true;
    const cleanups: Array<() => void> = [];

    void onWorkflowEvent((payload) => {
      if (!alive) {
        return;
      }

      const parsedEvent = parseWorkflowEventPayload(payload);
      if (parsedEvent) {
        setRuntimeState((current) => reduceRuntimeState(current, parsedEvent));

        switch (parsedEvent.kind) {
          case 'started':
            appendRuntimeLog(parsedEvent.nodeId, 'info', '节点开始执行');
            break;
          case 'completed':
            appendRuntimeLog(parsedEvent.nodeId, 'success', '节点执行完成');
            break;
          case 'output':
            appendRuntimeLog(parsedEvent.nodeId, 'success', '节点产生输出');
            break;
          case 'failed':
            appendRuntimeLog(parsedEvent.nodeId, 'error', '节点执行失败', parsedEvent.error ?? null);
            appendAppError(
              'workflow',
              `节点 ${parsedEvent.nodeId} 执行失败`,
              parsedEvent.error ?? null,
            );
            break;
        }
      }
    }).then((cleanup) => {
      if (alive) {
        cleanups.push(cleanup);
      }
    });

    void onWorkflowResult((payload) => {
      if (!alive) {
        return;
      }

      setResults((current) => [payload, ...current].slice(0, 8));
    }).then((cleanup) => {
      if (alive) {
        cleanups.push(cleanup);
      }
    });

    void onWorkflowDeployed((payload) => {
      if (!alive) {
        return;
      }

      setDeployInfo(payload);
      setEventFeed([
        buildRuntimeLogEntry(
          'system',
          'success',
          '工作流部署完成',
          payload.rootNodes.length > 0 ? `根节点: ${payload.rootNodes.join(', ')}` : null,
        ),
      ]);
      setResults([]);
      setAppErrors([]);
      setRuntimeState(EMPTY_RUNTIME_STATE);
      setStatusMessage(`已部署 ${payload.nodeCount} 个节点，根节点: ${payload.rootNodes.join(', ')}`);
    }).then((cleanup) => {
      if (alive) {
        cleanups.push(cleanup);
      }
    });

    void onWorkflowUndeployed((payload) => {
      if (!alive) {
        return;
      }

      setDeployInfo(null);
      setResults([]);
      setRuntimeState(EMPTY_RUNTIME_STATE);
      appendRuntimeLog(
        'system',
        payload.hadWorkflow ? 'warn' : 'info',
        payload.hadWorkflow ? '工作流已反部署' : '当前没有已部署工作流',
        payload.hadWorkflow ? `已停止 ${payload.abortedTimerCount} 个触发任务` : null,
      );
      setStatusMessage(
        payload.hadWorkflow
          ? `工作流已反部署，已停止 ${payload.abortedTimerCount} 个触发任务。`
          : '当前没有已部署工作流。',
      );
    }).then((cleanup) => {
      if (alive) {
        cleanups.push(cleanup);
      }
    });

    return () => {
      alive = false;
      for (const cleanup of cleanups) {
        cleanup();
      }
    };
  }, []);

  function resetWorkspaceRuntime(nextMessage: string) {
    setDeployInfo(null);
    setResults([]);
    setEventFeed([]);
    setAppErrors([]);
    setRuntimeState(EMPTY_RUNTIME_STATE);
    setStatusMessage(nextMessage);
  }

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
    setStatusMessage(nextStatusMessage);
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
    resetWorkspaceRuntime(`已进入工程 ${board.name}。`);
  }

  function handleBackToBoards() {
    setSidebarSection('boards');
    setActiveBoard(null);
    resetWorkspaceRuntime('已返回所有看板。');
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
      setStatusMessage('请先从所有看板进入工程。');
      return;
    }

    const deploySnapshot = buildDeploySnapshot();
    if (deploySnapshot.error || !deploySnapshot.astText) {
      appendAppError('command', '部署前 AST 校验失败', deploySnapshot.error ?? '未知错误');
      setStatusMessage(`AST 无法部署: ${deploySnapshot.error ?? '未知错误'}`);
      return;
    }

    if (deploySnapshot.astText !== astText) {
      updateProjectDraft(activeBoard.id, { astText: deploySnapshot.astText });
    }

    if (!hasTauriRuntime()) {
      setStatusMessage('纯 Web 预览模式下不会实际调用后端，已完成 AST 结构校验。');
      appendRuntimeLog('system', 'info', '纯 Web 预览模式下跳过实际部署');
      return;
    }

    try {
      const response = await deployWorkflow(deploySnapshot.astText);
      setStatusMessage(`部署完成，节点数 ${response.nodeCount}，边数 ${response.edgeCount}。`);
      await refreshConnections();
    } catch (error) {
      const { message, detail } = describeUnknownError(error);
      appendAppError('command', '部署工作流失败', detail ?? message);
      setStatusMessage(message);
    }
  }

  async function handleUndeploy() {
    if (!activeBoard) {
      setStatusMessage('请先从所有看板进入工程。');
      return;
    }

    if (!hasTauriRuntime()) {
      resetWorkspaceRuntime('已在预览态清空部署状态。');
      appendRuntimeLog('system', 'info', '预览态已清空部署状态');
      return;
    }

    try {
      const response = await undeployWorkflow();
      if (!response.hadWorkflow) {
        setStatusMessage('当前没有已部署工作流。');
      }
    } catch (error) {
      const { message, detail } = describeUnknownError(error);
      appendAppError('command', '反部署失败', detail ?? message);
      setStatusMessage(message);
    }
  }

  async function handleDispatchPayload() {
    if (!activeBoard) {
      setStatusMessage('请先从所有看板进入工程。');
      return;
    }

    let payload: unknown;

    try {
      payload = JSON.parse(payloadText);
    } catch (error) {
      appendAppError(
        'command',
        '测试载荷 JSON 无法解析',
        error instanceof Error ? error.message : null,
      );
      setStatusMessage(
        error instanceof Error ? `Payload JSON 无法解析: ${error.message}` : 'Payload JSON 无法解析',
      );
      return;
    }

    if (!hasTauriRuntime()) {
      setResults((current) => [
        {
          trace_id: 'web-preview',
          timestamp: new Date().toISOString(),
          payload: payload as WorkflowResult['payload'],
        },
        ...current,
      ].slice(0, 8));
      setStatusMessage('已在纯 Web 预览模式下模拟发送 payload。');
      appendRuntimeLog('system', 'info', '已在预览态模拟发送测试载荷');
      return;
    }

    if (!deployInfo) {
      appendAppError('command', '发送测试载荷失败', '请先部署工作流');
      setStatusMessage('请先部署工作流，再发送测试消息。');
      return;
    }

    try {
      const response = await dispatchPayload(payload);
      appendRuntimeLog('dispatch', 'info', `已提交测试载荷`, `trace_id=${response.traceId}`);
      setStatusMessage(`已提交 payload，trace_id=${response.traceId}`);
    } catch (error) {
      const { message, detail } = describeUnknownError(error);
      appendAppError('command', '发送测试载荷失败', detail ?? message);
      setStatusMessage(message);
    }
  }

  async function refreshConnections() {
    if (!hasTauriRuntime()) {
      return;
    }

    try {
      const nextConnections = await listConnections();
      setConnections(nextConnections);
    } catch (error) {
      const { message, detail } = describeUnknownError(error);
      appendAppError('command', '加载连接列表失败', detail ?? message);
      setStatusMessage(message);
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
    deployInfo,
    runtimeState,
  );
  const workflowStatusLabel = getWorkflowStatusLabel(workflowStatus);
  const workflowStatusPillClass = getWorkflowStatusPillClass(workflowStatus);
  const canDispatchPayload = Boolean(activeBoard) && (!isTauriRuntime || Boolean(deployInfo));
  const connectionPreview = connections.slice(0, 4);
  const sidebarSections = buildSidebarSections(
    workflowStatusLabel,
    graphState.error,
    graphConnectionCount,
    deployInfo,
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
          className={`studio-board-workspace ${isRuntimeDockCollapsed ? 'is-runtime-collapsed' : ''}`}
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
            runtimeState={runtimeState}
            workflowStatus={workflowStatus}
            accentHex={settings.accentHex}
            nodeRhaiColor={settings.accentThemeVariables['--node-rhai']}
            onRunRequested={handleDeploy}
            onStopRequested={handleUndeploy}
            onDispatchRequested={handleDispatchPayload}
            canDispatchPayload={canDispatchPayload}
            onGraphChange={handleGraphChange}
            onError={handleFlowgramError}
          />

          <RuntimeDock
            eventFeed={eventFeed}
            appErrors={appErrors}
            results={results}
            connectionPreview={connectionPreview}
            themeMode={settings.themeMode}
            isCollapsed={isRuntimeDockCollapsed}
            onToggleCollapsed={() => setIsRuntimeDockCollapsed((current) => !current)}
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
                activeNodeCount={runtimeState.activeNodeIds.length}
                completedNodeCount={runtimeState.completedNodeIds.length}
                failedNodeCount={runtimeState.failedNodeIds.length}
                outputNodeCount={runtimeState.outputNodeIds.length}
                eventCount={eventFeed.length}
                resultCount={results.length}
                statusMessage={statusMessage}
                deployInfo={deployInfo}
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
                runtimeConnections={connections}
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
                deployInfo={deployInfo}
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
                statusMessage={statusMessage}
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
