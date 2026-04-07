import { useDeferredValue, useEffect, useMemo, useState } from 'react';

import { AboutPanel } from './components/app/AboutPanel';
import { BOARD_LIBRARY, BoardsPanel, type BoardItem } from './components/app/BoardsPanel';
import { DashboardPanel } from './components/app/DashboardPanel';
import { PayloadPanel } from './components/app/PayloadPanel';
import { RuntimeDock } from './components/app/RuntimeDock';
import { SettingsPanel } from './components/app/SettingsPanel';
import { SidebarNav } from './components/app/SidebarNav';
import { SourcePanel } from './components/app/SourcePanel';
import { StudioControlBar } from './components/app/StudioControlBar';
import type {
  MotionMode,
  SidebarSection,
  SidebarSectionConfig,
  StartupPage,
  ThemeMode,
  UiDensity,
} from './components/app/types';
import { ConnectionStudio } from './components/ConnectionStudio';
import { FlowgramCanvas } from './components/FlowgramCanvas';
import { parseWorkflowGraph } from './lib/graph';
import {
  ACCENT_PRESET_OPTIONS,
  buildAccentThemeVariables,
  getAccentHex,
  normalizeCustomAccentHex,
  type AccentPreset,
} from './lib/theme';
import {
  deployWorkflow,
  dispatchPayload,
  enableAdaptiveWindowSizing,
  hasTauriRuntime,
  listConnections,
  onWorkflowDeployed,
  onWorkflowEvent,
  onWorkflowResult,
} from './lib/tauri';
import type {
  ConnectionRecord,
  DeployResponse,
  JsonValue,
  WorkflowEvent,
  WorkflowRuntimeState,
  WorkflowResult,
  WorkflowWindowStatus,
} from './types';
import { SAMPLE_AST, SAMPLE_PAYLOAD } from './types';

interface ParsedWorkflowEvent {
  kind: 'started' | 'completed' | 'failed' | 'output';
  nodeId: string;
  traceId: string;
  error?: string;
}

interface ProjectDraft {
  astText: string;
  payloadText: string;
}

const EMPTY_RUNTIME_STATE: WorkflowRuntimeState = {
  traceId: null,
  lastEventType: null,
  lastNodeId: null,
  lastError: null,
  lastUpdatedAt: null,
  activeNodeIds: [],
  completedNodeIds: [],
  failedNodeIds: [],
  outputNodeIds: [],
};

const THEME_STORAGE_KEY = 'nazh.theme';
const ACCENT_PRESET_STORAGE_KEY = 'nazh.accent-preset';
const CUSTOM_ACCENT_STORAGE_KEY = 'nazh.custom-accent';
const UI_DENSITY_STORAGE_KEY = 'nazh.ui-density';
const MOTION_MODE_STORAGE_KEY = 'nazh.motion-mode';
const STARTUP_PAGE_STORAGE_KEY = 'nazh.startup-page';
const CURRENT_USER_NAME = 'ssxue';
const DEFAULT_BOARD_ID = BOARD_LIBRARY[0]?.id ?? 'default';

function getInitialThemeMode(): ThemeMode {
  if (typeof window === 'undefined') {
    return 'light';
  }

  try {
    const storedTheme = window.localStorage.getItem(THEME_STORAGE_KEY);
    if (storedTheme === 'light' || storedTheme === 'dark') {
      return storedTheme;
    }
  } catch {
    // Ignore storage access failures and fall back to system preference.
  }

  return window.matchMedia('(prefers-color-scheme: dark)').matches ? 'dark' : 'light';
}

function getInitialAccentPreset(): AccentPreset {
  if (typeof window === 'undefined') {
    return ACCENT_PRESET_OPTIONS[0].key;
  }

  try {
    const storedPreset = window.localStorage.getItem(ACCENT_PRESET_STORAGE_KEY);
    if (
      storedPreset === 'custom' ||
      ACCENT_PRESET_OPTIONS.some((option) => option.key === storedPreset)
    ) {
      return storedPreset as AccentPreset;
    }
  } catch {
    // Ignore storage access failures and fall back to defaults.
  }

  return ACCENT_PRESET_OPTIONS[0].key;
}

function getInitialCustomAccentHex(): string {
  if (typeof window === 'undefined') {
    return normalizeCustomAccentHex(ACCENT_PRESET_OPTIONS[0].hex);
  }

  try {
    const storedHex = window.localStorage.getItem(CUSTOM_ACCENT_STORAGE_KEY);
    if (storedHex) {
      return normalizeCustomAccentHex(storedHex);
    }
  } catch {
    // Ignore storage access failures and fall back to defaults.
  }

  return normalizeCustomAccentHex(ACCENT_PRESET_OPTIONS[0].hex);
}

function getInitialUiDensity(): UiDensity {
  if (typeof window === 'undefined') {
    return 'comfortable';
  }

  try {
    const storedDensity = window.localStorage.getItem(UI_DENSITY_STORAGE_KEY);
    if (storedDensity === 'comfortable' || storedDensity === 'compact') {
      return storedDensity;
    }
  } catch {
    // Ignore storage access failures and fall back to defaults.
  }

  return 'comfortable';
}

function getInitialMotionMode(): MotionMode {
  if (typeof window === 'undefined') {
    return 'full';
  }

  try {
    const storedMotionMode = window.localStorage.getItem(MOTION_MODE_STORAGE_KEY);
    if (storedMotionMode === 'full' || storedMotionMode === 'reduced') {
      return storedMotionMode;
    }
  } catch {
    // Ignore storage access failures and fall back to system preference.
  }

  return window.matchMedia('(prefers-reduced-motion: reduce)').matches ? 'reduced' : 'full';
}

function getInitialStartupPage(): StartupPage {
  if (typeof window === 'undefined') {
    return 'dashboard';
  }

  try {
    const storedPage = window.localStorage.getItem(STARTUP_PAGE_STORAGE_KEY);
    if (storedPage === 'dashboard' || storedPage === 'boards') {
      return storedPage;
    }
  } catch {
    // Ignore storage access failures and fall back to defaults.
  }

  return 'dashboard';
}

function buildProjectAst(boardName: string): string {
  const base = JSON.parse(SAMPLE_AST) as {
    name?: string;
    nodes?: Record<string, { config?: Record<string, JsonValue> }>;
  };

  base.name = boardName;

  if (base.nodes?.ingress?.config) {
    base.nodes.ingress.config.message = `${boardName} 已接收边缘输入`;
  }

  return JSON.stringify(base, null, 2);
}

function buildInitialProjectDrafts(): Record<string, ProjectDraft> {
  return BOARD_LIBRARY.reduce<Record<string, ProjectDraft>>((drafts, board) => {
    drafts[board.id] = {
      astText: buildProjectAst(board.name),
      payloadText: SAMPLE_PAYLOAD,
    };
    return drafts;
  }, {});
}

function buildSidebarSections(
  workflowStatusLabel: string,
  graphError: string | null,
  graphConnectionCount: number,
  deployInfo: DeployResponse | null,
  activeBoardName: string | null,
): SidebarSectionConfig[] {
  return [
    {
      key: 'dashboard',
      group: 'top',
      label: 'Dashboard',
      badge: workflowStatusLabel,
    },
    {
      key: 'boards',
      group: 'top',
      label: '所有看板',
      badge: activeBoardName ?? `${BOARD_LIBRARY.length} 个工程`,
    },
    {
      key: 'source',
      group: 'main',
      label: '流程源配置',
      badge: activeBoardName ? (graphError ? '有错误' : '当前工程') : '未进入工程',
    },
    {
      key: 'connections',
      group: 'main',
      label: '连接资源',
      badge: activeBoardName ? `${graphConnectionCount} 个` : '未进入工程',
    },
    {
      key: 'payload',
      group: 'main',
      label: '测试载荷',
      badge: activeBoardName ? (deployInfo ? '可发送' : '待部署') : '未进入工程',
    },
    {
      key: 'settings',
      group: 'settings',
      label: '设置',
      badge: hasTauriRuntime() ? '桌面态' : '预览态',
    },
    {
      key: 'about',
      group: 'settings',
      label: '帮助关于',
      badge: 'Nazh',
    },
  ];
}

function pushUnique(items: string[], item: string): string[] {
  return items.includes(item) ? items : [...items, item];
}

function removeItem(items: string[], item: string): string[] {
  return items.filter((current) => current !== item);
}

function parseWorkflowEventPayload(payload: unknown): ParsedWorkflowEvent | null {
  if (!payload || typeof payload !== 'object') {
    return null;
  }

  const event = payload as WorkflowEvent;

  if (event.NodeStarted) {
    return {
      kind: 'started',
      nodeId: event.NodeStarted.node_id,
      traceId: event.NodeStarted.trace_id,
    };
  }

  if (event.NodeCompleted) {
    return {
      kind: 'completed',
      nodeId: event.NodeCompleted.node_id,
      traceId: event.NodeCompleted.trace_id,
    };
  }

  if (event.NodeFailed) {
    return {
      kind: 'failed',
      nodeId: event.NodeFailed.node_id,
      traceId: event.NodeFailed.trace_id,
      error: event.NodeFailed.error,
    };
  }

  if (event.WorkflowOutput) {
    return {
      kind: 'output',
      nodeId: event.WorkflowOutput.node_id,
      traceId: event.WorkflowOutput.trace_id,
    };
  }

  return null;
}

function reduceRuntimeState(
  current: WorkflowRuntimeState,
  event: ParsedWorkflowEvent,
): WorkflowRuntimeState {
  const baseState =
    current.traceId === event.traceId
      ? current
      : {
          ...EMPTY_RUNTIME_STATE,
          traceId: event.traceId,
        };

  const nextState: WorkflowRuntimeState = {
    ...baseState,
    traceId: event.traceId,
    lastEventType: event.kind,
    lastNodeId: event.nodeId,
    lastError: event.kind === 'failed' ? event.error ?? null : null,
    lastUpdatedAt: Date.now(),
  };

  switch (event.kind) {
    case 'started':
      nextState.activeNodeIds = pushUnique(baseState.activeNodeIds, event.nodeId);
      nextState.completedNodeIds = removeItem(baseState.completedNodeIds, event.nodeId);
      nextState.failedNodeIds = removeItem(baseState.failedNodeIds, event.nodeId);
      nextState.outputNodeIds = removeItem(baseState.outputNodeIds, event.nodeId);
      return nextState;
    case 'completed':
      nextState.activeNodeIds = removeItem(baseState.activeNodeIds, event.nodeId);
      nextState.completedNodeIds = pushUnique(baseState.completedNodeIds, event.nodeId);
      nextState.failedNodeIds = removeItem(baseState.failedNodeIds, event.nodeId);
      nextState.outputNodeIds = baseState.outputNodeIds;
      return nextState;
    case 'failed':
      nextState.activeNodeIds = removeItem(baseState.activeNodeIds, event.nodeId);
      nextState.completedNodeIds = removeItem(baseState.completedNodeIds, event.nodeId);
      nextState.failedNodeIds = pushUnique(baseState.failedNodeIds, event.nodeId);
      nextState.outputNodeIds = removeItem(baseState.outputNodeIds, event.nodeId);
      return nextState;
    case 'output':
      nextState.activeNodeIds = removeItem(baseState.activeNodeIds, event.nodeId);
      nextState.completedNodeIds = pushUnique(baseState.completedNodeIds, event.nodeId);
      nextState.failedNodeIds = removeItem(baseState.failedNodeIds, event.nodeId);
      nextState.outputNodeIds = pushUnique(baseState.outputNodeIds, event.nodeId);
      return nextState;
  }
}

function deriveWorkflowStatus(
  tauriRuntime: boolean,
  hasActiveBoard: boolean,
  deployInfo: DeployResponse | null,
  runtimeState: WorkflowRuntimeState,
): WorkflowWindowStatus {
  if (!tauriRuntime) {
    return 'preview';
  }

  if (!hasActiveBoard || !deployInfo) {
    return 'idle';
  }

  if (runtimeState.lastEventType === 'failed' || runtimeState.failedNodeIds.length > 0) {
    return 'failed';
  }

  if (runtimeState.lastEventType === 'started' || runtimeState.activeNodeIds.length > 0) {
    return 'running';
  }

  if (
    runtimeState.traceId &&
    (runtimeState.lastEventType === 'output' ||
      runtimeState.outputNodeIds.length > 0 ||
      (runtimeState.lastEventType === 'completed' &&
        runtimeState.completedNodeIds.length > 0 &&
        runtimeState.activeNodeIds.length === 0))
  ) {
    return 'completed';
  }

  return 'deployed';
}

function getWorkflowStatusLabel(status: WorkflowWindowStatus): string {
  switch (status) {
    case 'preview':
      return '浏览器预览';
    case 'idle':
      return '未部署';
    case 'deployed':
      return '已部署待运行';
    case 'running':
      return '运行中';
    case 'completed':
      return '执行完成';
    case 'failed':
      return '执行失败';
  }
}

function getWorkflowStatusPillClass(status: WorkflowWindowStatus): string {
  switch (status) {
    case 'running':
      return 'runtime-pill--running';
    case 'failed':
      return 'runtime-pill--failed';
    case 'completed':
    case 'deployed':
      return 'runtime-pill--ready';
    case 'idle':
    case 'preview':
      return 'runtime-pill--idle';
  }
}

function App() {
  const [startupPage, setStartupPage] = useState<StartupPage>(getInitialStartupPage);
  const [projectDrafts, setProjectDrafts] = useState<Record<string, ProjectDraft>>(
    buildInitialProjectDrafts,
  );
  const [activeBoard, setActiveBoard] = useState<BoardItem | null>(null);
  const [sidebarSection, setSidebarSection] = useState<SidebarSection>(getInitialStartupPage);
  const [themeMode, setThemeMode] = useState<ThemeMode>(getInitialThemeMode);
  const [accentPreset, setAccentPreset] = useState<AccentPreset>(getInitialAccentPreset);
  const [customAccentHex, setCustomAccentHex] = useState<string>(getInitialCustomAccentHex);
  const [densityMode, setDensityMode] = useState<UiDensity>(getInitialUiDensity);
  const [motionMode, setMotionMode] = useState<MotionMode>(getInitialMotionMode);
  const [statusMessage, setStatusMessage] = useState(
    hasTauriRuntime()
      ? '等待进入工程。'
      : '当前运行在纯 Web 预览模式，调用 Tauri 命令会被跳过。',
  );
  const [deployInfo, setDeployInfo] = useState<DeployResponse | null>(null);
  const [results, setResults] = useState<WorkflowResult[]>([]);
  const [eventFeed, setEventFeed] = useState<string[]>([]);
  const [connections, setConnections] = useState<ConnectionRecord[]>([]);
  const [flowgramReloadVersion, setFlowgramReloadVersion] = useState(0);
  const [runtimeState, setRuntimeState] = useState<WorkflowRuntimeState>(EMPTY_RUNTIME_STATE);
  const [boardWorkspaceKey, setBoardWorkspaceKey] = useState(0);

  const currentBoardId = activeBoard?.id ?? DEFAULT_BOARD_ID;
  const currentProject = projectDrafts[currentBoardId] ?? {
    astText: SAMPLE_AST,
    payloadText: SAMPLE_PAYLOAD,
  };
  const astText = currentProject.astText;
  const payloadText = currentProject.payloadText;
  const deferredAstText = useDeferredValue(astText);
  const graphState = useMemo(() => parseWorkflowGraph(deferredAstText), [deferredAstText]);
  const accentHex = useMemo(
    () => getAccentHex(accentPreset, customAccentHex),
    [accentPreset, customAccentHex],
  );
  const accentThemeVariables = useMemo(
    () => buildAccentThemeVariables(accentHex, themeMode),
    [accentHex, themeMode],
  );

  useEffect(() => {
    document.documentElement.dataset.theme = themeMode;

    try {
      window.localStorage.setItem(THEME_STORAGE_KEY, themeMode);
    } catch {
      // Ignore storage failures in restricted runtimes.
    }
  }, [themeMode]);

  useEffect(() => {
    Object.entries(accentThemeVariables).forEach(([key, value]) => {
      document.documentElement.style.setProperty(key, value);
    });

    try {
      window.localStorage.setItem(ACCENT_PRESET_STORAGE_KEY, accentPreset);
      window.localStorage.setItem(CUSTOM_ACCENT_STORAGE_KEY, customAccentHex);
    } catch {
      // Ignore storage failures in restricted runtimes.
    }
  }, [accentPreset, accentThemeVariables, customAccentHex]);

  useEffect(() => {
    document.documentElement.dataset.uiDensity = densityMode;

    try {
      window.localStorage.setItem(UI_DENSITY_STORAGE_KEY, densityMode);
    } catch {
      // Ignore storage failures in restricted runtimes.
    }
  }, [densityMode]);

  useEffect(() => {
    document.documentElement.dataset.motionMode = motionMode;

    try {
      window.localStorage.setItem(MOTION_MODE_STORAGE_KEY, motionMode);
    } catch {
      // Ignore storage failures in restricted runtimes.
    }
  }, [motionMode]);

  useEffect(() => {
    try {
      window.localStorage.setItem(STARTUP_PAGE_STORAGE_KEY, startupPage);
    } catch {
      // Ignore storage failures in restricted runtimes.
    }
  }, [startupPage]);

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
      }

      setEventFeed((current) => [
        `${new Date().toLocaleTimeString()} ${JSON.stringify(payload)}`,
        ...current,
      ].slice(0, 14));
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
      setEventFeed([]);
      setResults([]);
      setRuntimeState(EMPTY_RUNTIME_STATE);
      setStatusMessage(`已部署 ${payload.nodeCount} 个节点，根节点: ${payload.rootNodes.join(', ')}`);
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
    setRuntimeState(EMPTY_RUNTIME_STATE);
    setStatusMessage(nextMessage);
  }

  function updateProjectDraft(boardId: string, nextDraft: Partial<ProjectDraft>) {
    setProjectDrafts((current) => ({
      ...current,
      [boardId]: {
        ...(current[boardId] ?? {
          astText: buildProjectAst(activeBoard?.name ?? '默认工作流'),
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
    setFlowgramReloadVersion((current) => current + 1);
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

  function handleToggleTheme() {
    setThemeMode((current) => (current === 'dark' ? 'light' : 'dark'));
  }

  function handleOpenBoard(board: BoardItem) {
    setActiveBoard(board);
    setSidebarSection('boards');
    setBoardWorkspaceKey((current) => current + 1);
    setFlowgramReloadVersion((current) => current + 1);
    resetWorkspaceRuntime(`已进入工程 ${board.name}。`);
  }

  function handleBackToBoards() {
    setSidebarSection('boards');
    setActiveBoard(null);
    setBoardWorkspaceKey((current) => current + 1);
    resetWorkspaceRuntime('已返回所有看板。');
  }

  async function handleDeploy() {
    if (!activeBoard) {
      setStatusMessage('请先从所有看板进入工程。');
      return;
    }

    if (graphState.error) {
      setStatusMessage(`AST 无法部署: ${graphState.error}`);
      return;
    }

    if (!hasTauriRuntime()) {
      setStatusMessage('纯 Web 预览模式下不会实际调用后端，已完成 AST 结构校验。');
      return;
    }

    try {
      const response = await deployWorkflow(astText);
      setDeployInfo(response);
      setEventFeed([]);
      setResults([]);
      setRuntimeState(EMPTY_RUNTIME_STATE);
      setStatusMessage(`部署完成，节点数 ${response.nodeCount}，边数 ${response.edgeCount}。`);
      await refreshConnections();
    } catch (error) {
      setStatusMessage(error instanceof Error ? error.message : '部署失败');
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
      return;
    }

    if (!deployInfo) {
      setStatusMessage('请先部署工作流，再发送测试消息。');
      return;
    }

    try {
      const response = await dispatchPayload(payload);
      setStatusMessage(`已提交 payload，trace_id=${response.traceId}`);
    } catch (error) {
      setStatusMessage(error instanceof Error ? error.message : '发送 payload 失败');
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
      setStatusMessage(error instanceof Error ? error.message : '加载连接列表失败');
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
  const runtimeSnapshot =
    runtimeState.lastNodeId && runtimeState.lastEventType
      ? `${runtimeState.lastEventType} @ ${runtimeState.lastNodeId}`
      : workflowStatusLabel;
  const runtimeUpdatedLabel = runtimeState.lastUpdatedAt
    ? new Date(runtimeState.lastUpdatedAt).toLocaleTimeString()
    : '尚无事件';
  const hasRuntimeDock = Boolean(activeBoard && deployInfo);
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
        <div key={`${activeBoard.id}-${boardWorkspaceKey}`} className="studio-board-workspace">
          <div
            className="studio-board-workspace__header window-safe-header"
            data-window-drag-region
          >
            <div>
              <h2>{activeBoard.name}</h2>
              <span>{activeBoard.updatedAt}</span>
            </div>
            <span className="panel__badge">{graphNodeCount} 节点</span>
          </div>

          <FlowgramCanvas
            graph={graphState.graph}
            reloadVersion={flowgramReloadVersion}
            runtimeState={runtimeState}
            workflowStatus={workflowStatus}
            accentHex={accentHex}
            nodeRhaiColor={accentThemeVariables['--node-rhai']}
            onGraphChange={handleGraphChange}
          />

          <StudioControlBar
            workflowStatusLabel={workflowStatusLabel}
            workflowStatusPillClass={workflowStatusPillClass}
            isTauriRuntime={isTauriRuntime}
            runtimeModeLabel={runtimeModeLabel}
            runtimeSnapshot={runtimeSnapshot}
            runtimeUpdatedLabel={runtimeUpdatedLabel}
            statusMessage={statusMessage}
            graphNodeCount={graphNodeCount}
            graphEdgeCount={graphEdgeCount}
            graphConnectionCount={graphConnectionCount}
            activeNodeCount={runtimeState.activeNodeIds.length}
            canDispatchPayload={canDispatchPayload}
            onDeploy={handleDeploy}
            onDispatchPayload={handleDispatchPayload}
            onRefreshConnections={refreshConnections}
          />

          {hasRuntimeDock ? (
            <RuntimeDock
              deployInfo={deployInfo}
              runtimeState={runtimeState}
              eventFeed={eventFeed}
              results={results}
              connectionPreview={connectionPreview}
            />
          ) : null}
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
                themeMode={themeMode}
                onThemeModeChange={setThemeMode}
                accentPreset={accentPreset}
                accentOptions={ACCENT_PRESET_OPTIONS}
                customAccentHex={customAccentHex}
                onAccentPresetChange={setAccentPreset}
                onCustomAccentChange={(hex) => {
                  setAccentPreset('custom');
                  setCustomAccentHex(normalizeCustomAccentHex(hex));
                }}
                densityMode={densityMode}
                onDensityModeChange={setDensityMode}
                motionMode={motionMode}
                onMotionModeChange={setMotionMode}
                startupPage={startupPage}
                onStartupPageChange={setStartupPage}
              />
            </div>
          </section>
        );
      case 'about':
        return (
          <section className="studio-content studio-content--panel">
            <div className="panel studio-content__panel studio-content__panel--scroll">
              <AboutPanel
                isTauriRuntime={isTauriRuntime}
                runtimeModeLabel={runtimeModeLabel}
                graphNodeCount={graphNodeCount}
                graphConnectionCount={graphConnectionCount}
                deployInfo={deployInfo}
              />
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
            themeMode={themeMode}
            onToggleTheme={handleToggleTheme}
            activeBoardName={activeBoard?.name ?? null}
            onBackToBoards={handleBackToBoards}
          />
        </aside>

        {renderStudioContent()}
      </section>
    </main>
  );
}

export default App;
