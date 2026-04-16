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
  libraryFilePath: string;
  usingDefaultLocation: boolean;
  libraryExists: boolean;
}

export interface ProjectWorkspaceLoadResult {
  storage: ProjectWorkspaceStorageInfo;
  libraryText: string | null;
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
): Promise<ConnectionDefinition[]> {
  return invoke<ConnectionDefinition[]>('load_connection_definitions', {
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

export async function loadProjectLibraryFile(
  workspacePath: string,
): Promise<ProjectWorkspaceLoadResult> {
  return invoke<ProjectWorkspaceLoadResult>('load_project_library_file', {
    workspacePath: workspacePath.trim() || null,
  });
}

export async function saveProjectLibraryFile(
  workspacePath: string,
  libraryText: string,
): Promise<ProjectWorkspaceStorageInfo> {
  return invoke<ProjectWorkspaceStorageInfo>('save_project_library_file', {
    workspacePath: workspacePath.trim() || null,
    libraryText,
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
  handler: (payload: unknown) => void,
): Promise<() => void> {
  const unlisten = await listen('workflow://node-status', (event) => {
    handler(event.payload);
  });

  return () => {
    unlisten();
  };
}

export async function onWorkflowResult(
  handler: (payload: WorkflowResult) => void,
): Promise<() => void> {
  const unlisten = await listen<WorkflowResult>('workflow://result', (event) => {
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
