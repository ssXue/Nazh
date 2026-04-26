import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { currentMonitor, getCurrentWindow, LogicalSize } from '@tauri-apps/api/window';

import type {
  AiCompletionRequest,
  AiCompletionResponse,
  AiConfigUpdate,
  AiConfigView,
  AiProviderDraft,
  AiTestResult,
  ConnectionDefinition,
  ConnectionRecord,
  DeadLetterRecord,
  DeployResponse,
  DescribeNodePinsRequest,
  DescribeNodePinsResponse,
  DispatchResponse,
  ListNodeTypesResponse,
  ObservabilityQueryResult,
  RuntimeBackpressureStrategy,
  RuntimeWorkflowSummary,
  UndeployResponse,
  WorkflowResult,
} from '../types';
import type {
  PersistedDeploymentSession,
  PersistedDeploymentSessionState,
} from './deployment-session';

export interface WorkflowRuntimePolicyInput {
  manualQueueCapacity?: number;
  triggerQueueCapacity?: number;
  manualBackpressureStrategy?: RuntimeBackpressureStrategy;
  triggerBackpressureStrategy?: RuntimeBackpressureStrategy;
  maxRetryAttempts?: number;
  initialRetryBackoffMs?: number;
  maxRetryBackoffMs?: number;
}

export interface ProjectWorkspaceStorageInfo {
  workspacePath: string;
  boardsDirectoryPath: string;
  usingDefaultLocation: boolean;
  boardFileCount: number;
}

export interface ProjectWorkspaceBoardFile {
  fileName: string;
  text: string;
}

export interface ProjectWorkspaceLoadResult {
  storage: ProjectWorkspaceStorageInfo;
  boardFiles: ProjectWorkspaceBoardFile[];
}

export interface SavedWorkspaceFile {
  filePath: string;
}

export interface ConnectionDefinitionsLoadResult {
  definitions: ConnectionDefinition[];
  fileExists: boolean;
}

export interface ScopedWorkflowEvent {
  workflowId: string;
  event: unknown;
}

export interface ScopedWorkflowResult {
  workflowId: string;
  result: WorkflowResult;
}

export function hasTauriRuntime(): boolean {
  if (typeof window === 'undefined') {
    return false;
  }

  return '__TAURI_INTERNALS__' in window || '__TAURI__' in window;
}

export function installDesktopShellGuards(): () => void {
  if (!hasTauriRuntime() || typeof window === 'undefined' || typeof document === 'undefined') {
    return () => {};
  }

  const appWindow = getCurrentWindow();

  document.documentElement.classList.add('is-tauri-runtime');
  document.body.classList.add('is-tauri-runtime');

  const preventContextMenu = (event: MouseEvent) => {
    event.preventDefault();
  };
  const preventGestureZoom = (event: Event) => {
    event.preventDefault();
  };
  const preventWheelZoom = (event: WheelEvent) => {
    if (event.ctrlKey || event.metaKey) {
      event.preventDefault();
    }
  };
  const handleWindowDragMouseDown = (event: MouseEvent) => {
    const target = event.target;
    if (!(target instanceof Element)) {
      return;
    }

    if (
      target.closest(
        'button, input, textarea, select, option, a, summary, [role="button"], [data-no-window-drag]',
      )
    ) {
      return;
    }

    if (!target.closest('[data-window-drag-region]') || event.buttons !== 1) {
      return;
    }

    if (event.detail === 2) {
      void appWindow.toggleMaximize();
      return;
    }

    void appWindow.startDragging();
  };

  document.addEventListener('contextmenu', preventContextMenu);
  document.addEventListener('gesturestart', preventGestureZoom, { passive: false });
  document.addEventListener('gesturechange', preventGestureZoom, { passive: false });
  document.addEventListener('gestureend', preventGestureZoom, { passive: false });
  document.addEventListener('mousedown', handleWindowDragMouseDown);
  window.addEventListener('wheel', preventWheelZoom, { passive: false });

  return () => {
    document.removeEventListener('contextmenu', preventContextMenu);
    document.removeEventListener('gesturestart', preventGestureZoom);
    document.removeEventListener('gesturechange', preventGestureZoom);
    document.removeEventListener('gestureend', preventGestureZoom);
    document.removeEventListener('mousedown', handleWindowDragMouseDown);
    window.removeEventListener('wheel', preventWheelZoom);
    document.documentElement.classList.remove('is-tauri-runtime');
    document.body.classList.remove('is-tauri-runtime');
  };
}

const WINDOW_MIN_WIDTH = 1040;
const WINDOW_MIN_HEIGHT = 700;
const WINDOW_WIDTH_RATIO = 0.92;
const WINDOW_HEIGHT_RATIO = 0.88;
const WINDOW_MAX_RATIO = 0.97;
const WINDOW_SYNC_DELAY_MS = 120;

function clamp(value: number, min: number, max: number): number {
  return Math.min(Math.max(value, min), max);
}

function toMonitorKey(monitor: Awaited<ReturnType<typeof currentMonitor>>): string | null {
  if (!monitor) {
    return null;
  }

  return `${monitor.name ?? 'monitor'}:${monitor.position.x}:${monitor.position.y}:${monitor.scaleFactor}`;
}

export async function enableAdaptiveWindowSizing(): Promise<() => void> {
  if (!hasTauriRuntime() || typeof window === 'undefined') {
    return () => {};
  }

  const appWindow = getCurrentWindow();
  let disposed = false;
  let lastMonitorKey: string | null = null;
  let syncTimer: number | null = null;
  let syncInFlight = false;

  const syncWindowBounds = async (reason: 'init' | 'move' | 'resize' | 'scale') => {
    if (disposed || syncInFlight) {
      return;
    }

    syncInFlight = true;

    try {
      const monitor = await currentMonitor();
      if (!monitor) {
        return;
      }

      const logicalWorkWidth = Math.floor(monitor.workArea.size.width / monitor.scaleFactor);
      const logicalWorkHeight = Math.floor(monitor.workArea.size.height / monitor.scaleFactor);
      const maxWidth = Math.max(WINDOW_MIN_WIDTH, Math.floor(logicalWorkWidth * WINDOW_MAX_RATIO));
      const maxHeight = Math.max(WINDOW_MIN_HEIGHT, Math.floor(logicalWorkHeight * WINDOW_MAX_RATIO));
      const preferredWidth = clamp(
        Math.floor(logicalWorkWidth * WINDOW_WIDTH_RATIO),
        WINDOW_MIN_WIDTH,
        maxWidth,
      );
      const preferredHeight = clamp(
        Math.floor(logicalWorkHeight * WINDOW_HEIGHT_RATIO),
        WINDOW_MIN_HEIGHT,
        maxHeight,
      );

      await appWindow.setSizeConstraints({
        minWidth: WINDOW_MIN_WIDTH,
        minHeight: WINDOW_MIN_HEIGHT,
        maxWidth,
        maxHeight,
      });

      const currentSize = await appWindow.innerSize();
      const currentWidth = Math.round(currentSize.width / monitor.scaleFactor);
      const currentHeight = Math.round(currentSize.height / monitor.scaleFactor);
      const nextMonitorKey = toMonitorKey(monitor);
      const movedToNewMonitor = lastMonitorKey !== null && nextMonitorKey !== lastMonitorKey;
      const nextWidth =
        reason === 'init'
          ? preferredWidth
          : clamp(currentWidth, WINDOW_MIN_WIDTH, maxWidth);
      const nextHeight =
        reason === 'init' || movedToNewMonitor
          ? preferredHeight
          : clamp(currentHeight, WINDOW_MIN_HEIGHT, maxHeight);

      if (nextWidth !== currentWidth || nextHeight !== currentHeight) {
        await appWindow.setSize(new LogicalSize(nextWidth, nextHeight));
      }

      lastMonitorKey = nextMonitorKey;
    } catch (error) {
      console.warn('Failed to sync adaptive window bounds', error);
    } finally {
      syncInFlight = false;
    }
  };

  const scheduleSync = (reason: 'move' | 'resize' | 'scale') => {
    if (disposed) {
      return;
    }

    if (syncTimer !== null) {
      window.clearTimeout(syncTimer);
    }

    syncTimer = window.setTimeout(() => {
      syncTimer = null;
      void syncWindowBounds(reason);
    }, WINDOW_SYNC_DELAY_MS);
  };

  await syncWindowBounds('init');

  const unlistenMoved = await appWindow.onMoved(() => {
    scheduleSync('move');
  });
  const unlistenResized = await appWindow.onResized(() => {
    scheduleSync('resize');
  });
  const unlistenScaleChanged = await appWindow.onScaleChanged(() => {
    scheduleSync('scale');
  });

  return () => {
    disposed = true;

    if (syncTimer !== null) {
      window.clearTimeout(syncTimer);
    }

    unlistenMoved();
    unlistenResized();
    unlistenScaleChanged();
  };
}

export async function minimizeCurrentWindow(): Promise<void> {
  if (!hasTauriRuntime()) {
    return;
  }

  try {
    await getCurrentWindow().minimize();
  } catch (error) {
    console.error('Failed to minimize current window', error);
  }
}

export async function toggleCurrentWindowMaximize(): Promise<void> {
  if (!hasTauriRuntime()) {
    return;
  }

  try {
    await getCurrentWindow().toggleMaximize();
  } catch (error) {
    console.error('Failed to toggle maximize on current window', error);
  }
}

export async function closeCurrentWindow(): Promise<void> {
  if (!hasTauriRuntime()) {
    return;
  }

  try {
    await getCurrentWindow().close();
  } catch (error) {
    console.error('Failed to close current window', error);
  }
}

export async function watchCurrentWindowMaximized(
  handler: (isMaximized: boolean) => void,
): Promise<() => void> {
  if (!hasTauriRuntime()) {
    handler(false);
    return () => {};
  }

  const appWindow = getCurrentWindow();

  const syncState = async () => {
    try {
      handler(await appWindow.isMaximized());
    } catch (error) {
      console.warn('Failed to read current window maximize state', error);
    }
  };

  await syncState();

  const unlistenResized = await appWindow.onResized(() => {
    void syncState();
  });

  return () => {
    unlistenResized();
  };
}

export async function deployWorkflow(
  ast: string,
  connectionDefinitions?: ConnectionDefinition[],
  observabilityContext?: {
    workspacePath: string;
    projectId: string;
    projectName: string;
    environmentId: string;
    environmentName: string;
    deploymentSource?: string;
  },
  runtimeOptions?: {
    workflowId?: string;
    runtimePolicy?: WorkflowRuntimePolicyInput;
  },
): Promise<DeployResponse> {
  return invoke<DeployResponse>('deploy_workflow', {
    ast,
    connectionDefinitions: connectionDefinitions ?? null,
    observabilityContext: observabilityContext
      ? {
          workspacePath: observabilityContext.workspacePath.trim(),
          projectId: observabilityContext.projectId,
          projectName: observabilityContext.projectName,
          environmentId: observabilityContext.environmentId,
          environmentName: observabilityContext.environmentName,
          deploymentSource: observabilityContext.deploymentSource ?? 'manual',
        }
      : null,
    workflowId: runtimeOptions?.workflowId?.trim() ? runtimeOptions.workflowId.trim() : null,
    runtimePolicy: runtimeOptions?.runtimePolicy ?? null,
  });
}

export async function dispatchPayload(
  payload: unknown,
  workflowId?: string | null,
): Promise<DispatchResponse> {
  return invoke<DispatchResponse>('dispatch_payload', {
    payload,
    workflowId: workflowId?.trim() ? workflowId.trim() : null,
  });
}

export async function undeployWorkflow(workflowId?: string | null): Promise<UndeployResponse> {
  return invoke<UndeployResponse>('undeploy_workflow', {
    workflowId: workflowId?.trim() ? workflowId.trim() : null,
  });
}

export async function listConnections(): Promise<ConnectionRecord[]> {
  return invoke<ConnectionRecord[]>('list_connections');
}

export async function listNodeTypes(): Promise<ListNodeTypesResponse> {
  return invoke<ListNodeTypesResponse>('list_node_types');
}

/**
 * 给定节点类型 + config，返回该节点实例的 input/output pin schema。
 *
 * 用于 ADR-0010 Phase 2 前端连接期校验：FlowGram `canAddLine` 钩子
 * 通过缓存的 pin schema 即时判断"上游产出 → 下游期望"是否兼容。
 *
 * 实例化无副作用（节点构造器只读 config + 资源句柄克隆）。
 * 失败时调用方应 fallback 到 `Any/Any`，部署期 backstop 兜底。
 */
export async function describeNodePins(
  nodeType: string,
  config: Record<string, unknown>,
): Promise<DescribeNodePinsResponse> {
  const request: DescribeNodePinsRequest = {
    nodeType,
    config: config as DescribeNodePinsRequest['config'],
  };
  return invoke<DescribeNodePinsResponse>('describe_node_pins', { request });
}

export async function listRuntimeWorkflows(): Promise<RuntimeWorkflowSummary[]> {
  return invoke<RuntimeWorkflowSummary[]>('list_runtime_workflows');
}

export async function setActiveRuntimeWorkflow(
  workflowId: string,
): Promise<RuntimeWorkflowSummary> {
  return invoke<RuntimeWorkflowSummary>('set_active_runtime_workflow', {
    workflowId: workflowId.trim(),
  });
}

export async function listDeadLetters(
  workspacePath: string,
  workflowId?: string | null,
  limit = 120,
): Promise<DeadLetterRecord[]> {
  return invoke<DeadLetterRecord[]>('list_dead_letters', {
    workspacePath: workspacePath.trim() || null,
    workflowId: workflowId?.trim() ? workflowId.trim() : null,
    limit,
  });
}

export async function queryObservability(
  workspacePath: string,
  traceId?: string | null,
  search?: string | null,
  limit = 240,
): Promise<ObservabilityQueryResult> {
  return invoke<ObservabilityQueryResult>('query_observability', {
    workspacePath: workspacePath.trim() || null,
    traceId: traceId?.trim() ? traceId.trim() : null,
    search: search?.trim() ? search.trim() : null,
    limit,
  });
}

export async function loadConnectionDefinitions(
  workspacePath: string,
): Promise<ConnectionDefinitionsLoadResult> {
  return invoke<ConnectionDefinitionsLoadResult>('load_connection_definitions', {
    workspacePath: workspacePath.trim() || null,
  });
}

export async function saveConnectionDefinitions(
  workspacePath: string,
  definitions: ConnectionDefinition[],
): Promise<void> {
  return invoke<void>('save_connection_definitions', {
    workspacePath: workspacePath.trim() || null,
    definitions,
  });
}

export async function loadProjectBoardFiles(
  workspacePath: string,
): Promise<ProjectWorkspaceLoadResult> {
  return invoke<ProjectWorkspaceLoadResult>('load_project_board_files', {
    workspacePath: workspacePath.trim() || null,
  });
}

export async function saveProjectBoardFiles(
  workspacePath: string,
  boardFiles: ProjectWorkspaceBoardFile[],
): Promise<ProjectWorkspaceStorageInfo> {
  return invoke<ProjectWorkspaceStorageInfo>('save_project_board_files', {
    workspacePath: workspacePath.trim() || null,
    boardFiles,
  });
}

export async function saveFlowgramExportFile(
  workspacePath: string,
  fileName: string,
  payload: {
    text?: string;
    bytes?: number[];
  },
): Promise<SavedWorkspaceFile> {
  return invoke<SavedWorkspaceFile>('save_flowgram_export_file', {
    workspacePath: workspacePath.trim() || null,
    fileName,
    text: payload.text ?? null,
    bytes: payload.bytes ?? null,
  });
}

export async function loadDeploymentSessionFile(
  workspacePath: string,
): Promise<PersistedDeploymentSession | null> {
  return invoke<PersistedDeploymentSession | null>('load_deployment_session_file', {
    workspacePath: workspacePath.trim() || null,
  });
}

export async function loadDeploymentSessionStateFile(
  workspacePath: string,
): Promise<PersistedDeploymentSessionState> {
  return invoke<PersistedDeploymentSessionState>('load_deployment_session_state_file', {
    workspacePath: workspacePath.trim() || null,
  });
}

export async function listDeploymentSessionsFile(
  workspacePath: string,
): Promise<PersistedDeploymentSession[]> {
  return invoke<PersistedDeploymentSession[]>('list_deployment_sessions_file', {
    workspacePath: workspacePath.trim() || null,
  });
}

export async function saveDeploymentSessionFile(
  workspacePath: string,
  session: PersistedDeploymentSession,
  activeProjectId?: string | null,
): Promise<void> {
  return invoke<void>('save_deployment_session_file', {
    workspacePath: workspacePath.trim() || null,
    session,
    activeProjectId: activeProjectId === undefined ? null : activeProjectId,
  });
}

export async function setDeploymentSessionActiveProjectFile(
  workspacePath: string,
  projectId: string | null,
): Promise<void> {
  return invoke<void>('set_deployment_session_active_project_file', {
    workspacePath: workspacePath.trim() || null,
    projectId: projectId?.trim() ? projectId.trim() : null,
  });
}

export async function removeDeploymentSessionFile(
  workspacePath: string,
  projectId: string,
): Promise<void> {
  return invoke<void>('remove_deployment_session_file', {
    workspacePath: workspacePath.trim() || null,
    projectId: projectId.trim(),
  });
}

export async function clearDeploymentSessionFile(workspacePath: string): Promise<void> {
  return invoke<void>('clear_deployment_session_file', {
    workspacePath: workspacePath.trim() || null,
  });
}

export interface SerialPortInfo {
  path: string;
  portType: string;
  description: string;
}

export async function listSerialPorts(): Promise<SerialPortInfo[]> {
  return invoke<SerialPortInfo[]>('list_serial_ports');
}

export interface TestSerialResult {
  ok: boolean;
  message: string;
}

export async function testSerialConnection(
  portPath: string,
  baudRate: number,
  dataBits: number,
  parity: string,
  stopBits: number,
  flowControl: string,
): Promise<TestSerialResult> {
  return invoke<TestSerialResult>('test_serial_connection', {
    portPath,
    baudRate,
    dataBits,
    parity,
    stopBits,
    flowControl,
  });
}

export async function onWorkflowEvent(
  handler: (payload: ScopedWorkflowEvent) => void,
): Promise<() => void> {
  const unlisten = await listen<ScopedWorkflowEvent>('workflow://node-status', (event) => {
    handler(event.payload);
  });

  return () => {
    unlisten();
  };
}

export async function onWorkflowResult(
  handler: (payload: ScopedWorkflowResult) => void,
): Promise<() => void> {
  const unlisten = await listen<ScopedWorkflowResult>('workflow://result', (event) => {
    handler(event.payload);
  });

  return () => {
    unlisten();
  };
}

export async function onWorkflowDeployed(
  handler: (payload: DeployResponse) => void,
): Promise<() => void> {
  const unlisten = await listen<DeployResponse>('workflow://deployed', (event) => {
    handler(event.payload);
  });

  return () => {
    unlisten();
  };
}

export async function onWorkflowUndeployed(
  handler: (payload: UndeployResponse) => void,
): Promise<() => void> {
  const unlisten = await listen<UndeployResponse>('workflow://undeployed', (event) => {
    handler(event.payload);
  });

  return () => {
    unlisten();
  };
}

export async function onRuntimeWorkflowFocus(
  handler: (payload: RuntimeWorkflowSummary) => void,
): Promise<() => void> {
  const unlisten = await listen<RuntimeWorkflowSummary>('workflow://runtime-focus', (event) => {
    handler(event.payload);
  });

  return () => {
    unlisten();
  };
}

// ── AI Copilot IPC 包装函数 ────────────────────────────────

export async function loadAiConfig(): Promise<AiConfigView> {
  return invoke<AiConfigView>('load_ai_config');
}

export async function saveAiConfig(update: AiConfigUpdate): Promise<AiConfigView> {
  return invoke<AiConfigView>('save_ai_config', { update });
}

export async function testAiProvider(draft: AiProviderDraft): Promise<AiTestResult> {
  return invoke<AiTestResult>('test_ai_provider', { draft });
}

export async function copilotComplete(request: AiCompletionRequest): Promise<AiCompletionResponse> {
  return invoke<AiCompletionResponse>('copilot_complete', { request });
}

function createCopilotStreamId(): string {
  const randomId = globalThis.crypto?.randomUUID?.();
  if (typeof randomId === 'string' && randomId.trim()) {
    return randomId;
  }
  return `copilot-${Date.now()}-${Math.random().toString(16).slice(2)}`;
}

function toError(error: unknown): Error {
  if (error instanceof Error) {
    return error;
  }
  return new Error(String(error));
}

export interface CopilotStreamResult {
  text: string;
  finishReason?: string;
}

export interface CopilotStreamRetryOptions {
  maxRetries?: number;
  onRetryStart?: (attempt: number, error: Error) => void | Promise<void>;
}

function isRecoverableCopilotStreamError(error: Error): boolean {
  const message = error.message.trim().toLowerCase();
  return [
    'error decoding response body',
    '未收到结束信号',
    'connection reset',
    'broken pipe',
    'unexpected eof',
    'unexpected end of file',
    'connection closed before message completed',
    'stream interrupted',
  ].some((pattern) => message.includes(pattern));
}

async function waitForCopilotRetry(delayMs: number): Promise<void> {
  await new Promise<void>((resolve) => {
    globalThis.setTimeout(resolve, delayMs);
  });
}

async function runCopilotStreamAttempt(
  request: AiCompletionRequest,
  onDelta: (text: string) => void,
  onThinking?: (text: string) => void,
): Promise<CopilotStreamResult> {
  const streamId = createCopilotStreamId();
  const eventName = `copilot://stream/${streamId}`;
  let accumulated = '';
  let thinkingAccumulated = '';
  let finishReason: string | undefined;
  let stopListening: (() => void) | null = null;
  let settled = false;
  let resolvePromise!: (value: CopilotStreamResult) => void;
  let rejectPromise!: (reason?: unknown) => void;

  const completion = new Promise<CopilotStreamResult>((resolve, reject) => {
    resolvePromise = resolve;
    rejectPromise = reject;
  });

  const cleanup = () => {
    if (stopListening) {
      const nextStop = stopListening;
      stopListening = null;
      nextStop();
    }
  };

  const resolveStream = (value: string) => {
    if (settled) {
      return;
    }
    settled = true;
    cleanup();
    resolvePromise({
      text: value,
      finishReason,
    });
  };

  const rejectStream = (error: unknown) => {
    if (settled) {
      return;
    }
    settled = true;
    cleanup();
    rejectPromise(toError(error));
  };

  stopListening = await listen<{
    delta?: string;
    thinking?: string;
    done?: boolean;
    error?: string;
    finishReason?: string;
  }>(
    eventName,
    (event) => {
      const payload = event.payload;
      if (payload.error) {
        rejectStream(payload.error);
        return;
      }
      if (payload.finishReason?.trim()) {
        finishReason = payload.finishReason.trim();
      }
      if (payload.thinking && onThinking) {
        thinkingAccumulated += payload.thinking;
        onThinking(thinkingAccumulated);
      }
      if (payload.delta) {
        accumulated += payload.delta;
        onDelta(accumulated);
      }
      if (payload.done) {
        resolveStream(accumulated);
      }
    },
  );

  try {
    await invoke<void>('copilot_complete_stream', { request, streamId });
  } catch (error) {
    rejectStream(error);
  }

  return completion;
}

export async function copilotCompleteStream(
  request: AiCompletionRequest,
  onDelta: (text: string) => void,
  onThinking?: (text: string) => void,
  retryOptions?: CopilotStreamRetryOptions,
): Promise<CopilotStreamResult> {
  const maxRetries = Math.max(0, Math.floor(retryOptions?.maxRetries ?? 1));

  for (let attempt = 0; attempt <= maxRetries; attempt += 1) {
    try {
      return await runCopilotStreamAttempt(request, onDelta, onThinking);
    } catch (error) {
      const normalizedError = toError(error);
      const shouldRetry =
        attempt < maxRetries && isRecoverableCopilotStreamError(normalizedError);

      if (!shouldRetry) {
        throw normalizedError;
      }

      await retryOptions?.onRetryStart?.(attempt + 1, normalizedError);
      onDelta('');
      onThinking?.('');
      await waitForCopilotRetry(350);
    }
  }

  throw new Error('AI 流式输出重试失败');
}
