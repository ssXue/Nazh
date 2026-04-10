import { useEffect, useMemo, useRef, useState } from 'react';

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
import type {
  MotionMode,
  SidebarSection,
  SidebarSectionConfig,
  StartupPage,
  ThemeMode,
  UiDensity,
} from './components/app/types';
import { ConnectionStudio } from './components/ConnectionStudio';
import { FlowgramCanvas, type FlowgramCanvasHandle } from './components/FlowgramCanvas';
import { parseWorkflowGraph } from './lib/graph';
import { formatWorkflowGraph } from './lib/flowgram';
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
  onWorkflowUndeployed,
  undeployWorkflow,
} from './lib/tauri';
import type {
  AppErrorRecord,
  ConnectionRecord,
  DeployResponse,
  JsonValue,
  RuntimeLogEntry,
  ExecutionEvent,
  WorkflowGraph,
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

function buildIndustrialAlarmExample(boardName: string): WorkflowGraph {
  return {
    name: boardName,
    connections: [
      {
        id: 'plc-main',
        type: 'modbus',
        metadata: {
          host: '192.168.10.11',
          port: 502,
          unit_id: 1,
          register: 40001,
        },
      },
    ],
    nodes: {
      timer_trigger: {
        type: 'timer',
        ai_description: 'Poll the PLC on a steady interval and seed runtime metadata.',
        config: {
          interval_ms: 5000,
          immediate: true,
          inject: {
            gateway: 'edge-a',
            scene: boardName,
          },
        },
        meta: {
          position: { x: 48, y: 88 },
        },
      },
      modbus_read: {
        type: 'modbusRead',
        connection_id: 'plc-main',
        ai_description: 'Read a simulated Modbus register from the main PLC.',
        timeout_ms: 1000,
        config: {
          unit_id: 1,
          register: 40001,
          quantity: 1,
          base_value: 68,
          amplitude: 6,
        },
        meta: {
          position: { x: 348, y: 88 },
        },
      },
      code_clean: {
        type: 'code',
        ai_description: 'Normalize the PLC value and derive route-ready severity fields.',
        timeout_ms: 1000,
        config: {
          script:
            'let value = payload["value"]; payload["temperature_c"] = value; payload["temperature_f"] = (value * 1.8) + 32.0; payload["severity"] = value > 120 ? "alert" : "nominal"; payload["route"] = payload["severity"]; payload["tag"] = `${payload["gateway"]}:boiler-a`; payload',
        },
        meta: {
          position: { x: 648, y: 88 },
        },
      },
      route_switch: {
        type: 'switch',
        ai_description: 'Route nominal telemetry into SQLite and alert telemetry into DingTalk.',
        timeout_ms: 1000,
        config: {
          script: 'payload["route"]',
          branches: [
            { key: 'nominal', label: 'Nominal' },
            { key: 'alert', label: 'Alert' },
          ],
        },
        meta: {
          position: { x: 968, y: 72 },
        },
      },
      sql_writer: {
        type: 'sqlWriter',
        ai_description: 'Persist nominal telemetry into a local SQLite audit table.',
        timeout_ms: 1500,
        config: {
          database_path: './data/edge-runtime.sqlite3',
          table: 'temperature_audit',
        },
        meta: {
          position: { x: 1288, y: 176 },
        },
      },
      http_alarm: {
        type: 'httpClient',
        ai_description: 'Send high severity telemetry to a DingTalk robot webhook with a rendered markdown alarm body.',
        timeout_ms: 1500,
        config: {
          method: 'POST',
          url: 'https://oapi.dingtalk.com/robot/send?access_token=replace_me',
          webhook_kind: 'dingtalk',
          body_mode: 'dingtalk_markdown',
          content_type: 'application/json',
          request_timeout_ms: 4000,
          title_template: 'Nazh 工业告警 · {{payload.tag}} · {{payload.severity}}',
          body_template:
            '### Nazh 工业告警\n- 设备：{{payload.tag}}\n- 场景：{{payload.scene}}\n- 温度：{{payload.temperature_c}} °C / {{payload.temperature_f}} °F\n- 严重级别：{{payload.severity}}\n- Trace：{{trace_id}}\n- 时间：{{timestamp}}',
          at_mobiles: [],
          at_all: false,
          headers: {
            'X-Alarm-Source': 'nazh',
          },
        },
        meta: {
          position: { x: 1288, y: -8 },
        },
      },
      debug_console: {
        type: 'debugConsole',
        ai_description: 'Mirror the final branch payload into the desktop debug console.',
        timeout_ms: 500,
        config: {
          label: 'final-output',
          pretty: true,
        },
        meta: {
          position: { x: 1608, y: 88 },
        },
      },
    },
    edges: [
      { from: 'timer_trigger', to: 'modbus_read' },
      { from: 'modbus_read', to: 'code_clean' },
      { from: 'code_clean', to: 'route_switch' },
      { from: 'route_switch', to: 'sql_writer', source_port_id: 'nominal' },
      { from: 'route_switch', to: 'http_alarm', source_port_id: 'alert' },
      { from: 'sql_writer', to: 'debug_console' },
      { from: 'http_alarm', to: 'debug_console' },
    ],
  };
}

function buildProjectAst(boardId: string, boardName: string): string {
  if (boardId === 'default') {
    return JSON.stringify(buildIndustrialAlarmExample(boardName), null, 2);
  }

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
      astText: buildProjectAst(board.id, board.name),
      payloadText:
        board.id === 'default'
          ? JSON.stringify(
              {
                manual: true,
                operator: CURRENT_USER_NAME,
                reason: 'manual override',
              },
              null,
              2,
            )
          : SAMPLE_PAYLOAD,
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
      label: '关于',
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

function createClientEntryId(prefix: string): string {
  return `${prefix}-${Date.now()}-${Math.random().toString(16).slice(2, 8)}`;
}

function describeUnknownError(error: unknown): { message: string; detail?: string | null } {
  if (error instanceof Error) {
    return {
      message: error.message || '未知错误',
      detail: error.stack ?? null,
    };
  }

  if (typeof error === 'string') {
    return { message: error };
  }

  if (error && typeof error === 'object') {
    try {
      return {
        message: JSON.stringify(error),
      };
    } catch {
      return {
        message: '发生了无法序列化的异常对象',
      };
    }
  }

  return { message: '未知错误' };
}

function buildRuntimeLogEntry(
  source: string,
  level: RuntimeLogEntry['level'],
  message: string,
  detail?: string | null,
): RuntimeLogEntry {
  return {
    id: createClientEntryId('log'),
    timestamp: Date.now(),
    level,
    source,
    message,
    detail: detail ?? null,
  };
}

function buildAppErrorRecord(
  scope: AppErrorRecord['scope'],
  title: string,
  detail?: string | null,
): AppErrorRecord {
  return {
    id: createClientEntryId('error'),
    timestamp: Date.now(),
    scope,
    title,
    detail: detail ?? null,
  };
}

function parseWorkflowEventPayload(payload: unknown): ParsedWorkflowEvent | null {
  if (!payload || typeof payload !== 'object') {
    return null;
  }

  const event = payload as ExecutionEvent;

  if ('Started' in event) {
    return {
      kind: 'started',
      nodeId: event.Started.stage,
      traceId: event.Started.trace_id,
    };
  }

  if ('Completed' in event) {
    return {
      kind: 'completed',
      nodeId: event.Completed.stage,
      traceId: event.Completed.trace_id,
    };
  }

  if ('Failed' in event) {
    return {
      kind: 'failed',
      nodeId: event.Failed.stage,
      traceId: event.Failed.trace_id,
      error: event.Failed.error,
    };
  }

  if ('Output' in event) {
    return {
      kind: 'output',
      nodeId: event.Output.stage,
      traceId: event.Output.trace_id,
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
  const accentHex = useMemo(
    () => getAccentHex(accentPreset, customAccentHex),
    [accentPreset, customAccentHex],
  );
  const accentThemeVariables = useMemo(
    () => buildAccentThemeVariables(accentHex, themeMode),
    [accentHex, themeMode],
  );

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
        payload.hadWorkflow ? `已停止 ${payload.abortedTimerCount} 个定时任务` : null,
      );
      setStatusMessage(
        payload.hadWorkflow
          ? `工作流已反部署，已停止 ${payload.abortedTimerCount} 个定时任务。`
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

  function handleToggleTheme() {
    setThemeMode((current) => (current === 'dark' ? 'light' : 'dark'));
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
            accentHex={accentHex}
            nodeRhaiColor={accentThemeVariables['--node-rhai']}
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
            themeMode={themeMode}
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
            themeMode={themeMode}
            onToggleTheme={handleToggleTheme}
          />
        </aside>

        {renderStudioContent()}
      </section>
    </main>
  );
}

export default App;
