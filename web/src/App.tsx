import { useDeferredValue, useEffect, useMemo, useState } from 'react';

import { AboutPanel } from './components/app/AboutPanel';
import { OverviewPanel } from './components/app/OverviewPanel';
import { PayloadPanel } from './components/app/PayloadPanel';
import { RuntimeDock } from './components/app/RuntimeDock';
import { SettingsPanel } from './components/app/SettingsPanel';
import { SidebarNav } from './components/app/SidebarNav';
import { SourcePanel } from './components/app/SourcePanel';
import { StudioControlBar } from './components/app/StudioControlBar';
import { StudioTitleBar } from './components/app/StudioTitleBar';
import type { SidebarSection, SidebarSectionConfig, ThemeMode } from './components/app/types';
import { ConnectionStudio } from './components/ConnectionStudio';
import { FlowgramCanvas } from './components/FlowgramCanvas';
import { parseWorkflowGraph } from './lib/graph';
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
const CURRENT_USER_NAME = 'ssxue';

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

function buildSidebarSections(
  workflowStatusLabel: string,
  graphError: string | null,
  graphNodeCount: number,
  graphConnectionCount: number,
  deployInfo: DeployResponse | null,
): SidebarSectionConfig[] {
  return [
    {
      key: 'overview',
      group: 'main',
      label: '管理总览',
      badge: workflowStatusLabel,
    },
    {
      key: 'canvas',
      group: 'main',
      label: '画布编辑',
      badge: `${graphNodeCount} 节点`,
    },
    {
      key: 'source',
      group: 'main',
      label: '流程源配置',
      badge: graphError ? '有错误' : '单一事实源',
    },
    {
      key: 'connections',
      group: 'main',
      label: '连接资源',
      badge: `${graphConnectionCount} 个`,
    },
    {
      key: 'payload',
      group: 'main',
      label: '测试载荷',
      badge: deployInfo ? '可发送' : '待部署',
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
  deployInfo: DeployResponse | null,
  runtimeState: WorkflowRuntimeState,
): WorkflowWindowStatus {
  if (!tauriRuntime) {
    return 'preview';
  }

  if (!deployInfo) {
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
  const [astText, setAstText] = useState(SAMPLE_AST);
  const [payloadText, setPayloadText] = useState(SAMPLE_PAYLOAD);
  const [sidebarSection, setSidebarSection] = useState<SidebarSection>('overview');
  const [themeMode, setThemeMode] = useState<ThemeMode>(getInitialThemeMode);
  const [statusMessage, setStatusMessage] = useState(
    hasTauriRuntime()
      ? '等待部署工作流。'
      : '当前运行在纯 Web 预览模式，调用 Tauri 命令会被跳过。',
  );
  const [deployInfo, setDeployInfo] = useState<DeployResponse | null>(null);
  const [results, setResults] = useState<WorkflowResult[]>([]);
  const [eventFeed, setEventFeed] = useState<string[]>([]);
  const [connections, setConnections] = useState<ConnectionRecord[]>([]);
  const [flowgramReloadVersion, setFlowgramReloadVersion] = useState(0);
  const [runtimeState, setRuntimeState] = useState<WorkflowRuntimeState>(EMPTY_RUNTIME_STATE);

  const deferredAstText = useDeferredValue(astText);
  const graphState = useMemo(() => parseWorkflowGraph(deferredAstText), [deferredAstText]);

  useEffect(() => {
    document.documentElement.dataset.theme = themeMode;

    try {
      window.localStorage.setItem(THEME_STORAGE_KEY, themeMode);
    } catch {
      // Ignore storage failures in restricted runtimes.
    }
  }, [themeMode]);

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

  function handleAstTextChange(nextText: string) {
    setAstText(nextText);
    setFlowgramReloadVersion((current) => current + 1);
  }

  function applyStructuredGraphChange(nextAstText: string, nextStatusMessage: string) {
    if (nextAstText === astText) {
      return;
    }

    setAstText(nextAstText);
    setStatusMessage(nextStatusMessage);
  }

  function handleGraphChange(nextAstText: string) {
    applyStructuredGraphChange(nextAstText, '画布变更已同步回 AST 文本。');
  }

  function handleConnectionGraphChange(nextAstText: string, nextStatusMessage: string) {
    applyStructuredGraphChange(nextAstText, nextStatusMessage);
  }

  function handleToggleTheme() {
    setThemeMode((current) => (current === 'dark' ? 'light' : 'dark'));
  }

  async function handleDeploy() {
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
  const workflowStatus = deriveWorkflowStatus(isTauriRuntime, deployInfo, runtimeState);
  const workflowStatusLabel = getWorkflowStatusLabel(workflowStatus);
  const workflowStatusPillClass = getWorkflowStatusPillClass(workflowStatus);
  const runtimeSnapshot =
    runtimeState.lastNodeId && runtimeState.lastEventType
      ? `${runtimeState.lastEventType} @ ${runtimeState.lastNodeId}`
      : workflowStatusLabel;
  const runtimeUpdatedLabel = runtimeState.lastUpdatedAt
    ? new Date(runtimeState.lastUpdatedAt).toLocaleTimeString()
    : '尚无事件';
  const hasRuntimeDock = Boolean(deployInfo);
  const canDispatchPayload = !isTauriRuntime || Boolean(deployInfo);
  const connectionPreview = connections.slice(0, 4);
  const sidebarSections = buildSidebarSections(
    workflowStatusLabel,
    graphState.error,
    graphNodeCount,
    graphConnectionCount,
    deployInfo,
  );

  function renderStudioContent() {
    switch (sidebarSection) {
      case 'canvas':
        return (
          <section className="studio-content studio-content--canvas">
            <FlowgramCanvas
              graph={graphState.graph}
              reloadVersion={flowgramReloadVersion}
              runtimeState={runtimeState}
              workflowStatus={workflowStatus}
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
          </section>
        );
      case 'overview':
        return (
          <section className="studio-content studio-content--panel">
            <div className="panel studio-content__panel studio-content__panel--scroll">
              <OverviewPanel
                graphNodeCount={graphNodeCount}
                graphEdgeCount={graphEdgeCount}
                graphConnectionCount={graphConnectionCount}
                activeNodeCount={runtimeState.activeNodeIds.length}
                workflowStatusLabel={workflowStatusLabel}
                workflowStatusPillClass={workflowStatusPillClass}
                statusMessage={statusMessage}
                runtimeSnapshot={runtimeSnapshot}
                runtimeUpdatedLabel={runtimeUpdatedLabel}
                deployInfo={deployInfo}
              />
            </div>
          </section>
        );
      case 'source':
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
        return (
          <section className="studio-content studio-content--panel">
            <div className="panel studio-content__panel studio-content__panel--editor">
              <PayloadPanel
                payloadText={payloadText}
                deployInfo={deployInfo}
                onPayloadTextChange={setPayloadText}
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
      <StudioTitleBar
        isTauriRuntime={isTauriRuntime}
        runtimeModeLabel={runtimeModeLabel}
        workflowStatusLabel={workflowStatusLabel}
        workflowStatusPillClass={workflowStatusPillClass}
        themeMode={themeMode}
        onToggleTheme={handleToggleTheme}
      />

      <section className="studio-frame">
        <aside className="studio-nav-sidebar">
          <SidebarNav
            activeSection={sidebarSection}
            sections={sidebarSections}
            onSectionChange={setSidebarSection}
            userName={CURRENT_USER_NAME}
            userRole={currentUserRole}
            onUserSwitch={() => setSidebarSection('settings')}
          />
        </aside>

        {renderStudioContent()}
      </section>
    </main>
  );
}

export default App;
