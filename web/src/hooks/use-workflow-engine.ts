//! 工作流生命周期管理 hook。
//!
//! 从 App.tsx 提取所有工作流运行时状态（部署信息、事件流、错误记录、
//! 连接列表、运行时状态机）及其对应的 useEffect 副作用逻辑，
//! 包括 Tauri 事件监听、全局错误捕获和自适应窗口大小。

import { useEffect, useRef, useState } from 'react';

import type { BoardItem } from '../components/app/BoardsPanel';
import type { SidebarSection } from '../components/app/types';
import {
  enableAdaptiveWindowSizing,
  hasTauriRuntime,
  listConnections,
  listRuntimeWorkflows,
  onWorkflowDeployed,
  onWorkflowEvent,
  onRuntimeWorkflowFocus,
  onWorkflowResult,
  onWorkflowUndeployed,
} from '../lib/tauri';
import {
  buildAppErrorRecord,
  buildRuntimeLogEntry,
  describeUnknownError,
  EMPTY_RUNTIME_STATE,
  parseWorkflowEventPayload,
  reduceRuntimeState,
} from '../lib/workflow-events';
import type {
  AppErrorRecord,
  ConnectionRecord,
  DeployResponse,
  RuntimeLogEntry,
  WorkflowResult,
  WorkflowRuntimeState,
} from '../types';

function toDeployInfoFromSummary(summary: {
  workflowId: string;
  projectId?: string | null;
  nodeCount: number;
  edgeCount: number;
  rootNodes: string[];
}): DeployResponse {
  return {
    nodeCount: summary.nodeCount,
    edgeCount: summary.edgeCount,
    rootNodes: summary.rootNodes,
    projectId: summary.projectId ?? undefined,
    workflowId: summary.workflowId,
    replacedExisting: undefined,
  };
}

/** 工作流引擎的只读状态快照。 */
export interface WorkflowEngineState {
  statusMessage: string;
  deployInfo: DeployResponse | null;
  results: WorkflowResult[];
  eventFeed: RuntimeLogEntry[];
  appErrors: AppErrorRecord[];
  connections: ConnectionRecord[];
  runtimeState: WorkflowRuntimeState;
  isRuntimeDockCollapsed: boolean;
}

/** 操作工作流引擎状态的回调集合。 */
export interface WorkflowEngineActions {
  setStatusMessage: (message: string) => void;
  appendRuntimeLog: (source: string, level: RuntimeLogEntry['level'], message: string, detail?: string | null) => void;
  appendAppError: (scope: AppErrorRecord['scope'], title: string, detail?: string | null) => void;
  handleFlowgramError: (title: string, detail?: string | null) => void;
  resetWorkspaceRuntime: (nextMessage: string) => void;
  applyDeploymentState: (payload: DeployResponse, nextMessage?: string) => void;
  addResult: (result: WorkflowResult) => void;
  refreshConnections: () => Promise<void>;
  setIsRuntimeDockCollapsed: React.Dispatch<React.SetStateAction<boolean>>;
}

/** 工作流引擎 hook 的完整返回类型（状态 + 操作）。 */
export type UseWorkflowEngineResult = WorkflowEngineState & WorkflowEngineActions;

/**
 * 管理工作流生命周期的所有运行时状态与副作用。
 *
 * 包含以下职责：
 * - 部署信息、事件流、错误记录、连接列表、运行时状态机的 useState 声明
 * - Tauri 事件监听（node-status、result、deployed、undeployed）
 * - 全局 window error / unhandledrejection 捕获
 * - 自适应窗口大小调整
 * - 连接列表自动刷新
 * - RuntimeDock 折叠状态
 */
export function useWorkflowEngine(
  activeBoard: BoardItem | null,
  sidebarSection: SidebarSection,
): UseWorkflowEngineResult {
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
  const [isRuntimeDockCollapsed, setIsRuntimeDockCollapsed] = useState(true);
  const activeWorkflowIdRef = useRef<string | null>(null);

  useEffect(() => {
    activeWorkflowIdRef.current = deployInfo?.workflowId?.trim() || null;
  }, [deployInfo?.workflowId]);

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

  function resetWorkspaceRuntime(nextMessage: string) {
    setDeployInfo(null);
    setResults([]);
    setEventFeed([]);
    setAppErrors([]);
    setRuntimeState(EMPTY_RUNTIME_STATE);
    setStatusMessage(nextMessage);
  }

  function applyDeploymentState(payload: DeployResponse, nextMessage?: string) {
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
    setStatusMessage(
      nextMessage ?? `已部署 ${payload.nodeCount} 个节点，根节点: ${payload.rootNodes.join(', ')}`,
    );
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

  // 切换工程时默认隐藏运行观测，避免遮挡工作面。
  useEffect(() => {
    setIsRuntimeDockCollapsed(true);
  }, [activeBoard?.id]);

  // 连接列表自动刷新（进入工程 / 部署变更 / 打开连接面板时触发）。
  useEffect(() => {
    if (!hasTauriRuntime()) {
      return;
    }

    if (sidebarSection !== 'connections' && !deployInfo) {
      return;
    }

    void refreshConnections();
  }, [activeBoard?.id, deployInfo, sidebarSection]);

  // 连接健康状态轮询（连接页常驻可见，运行中持续刷新）。
  useEffect(() => {
    if (!hasTauriRuntime()) {
      return;
    }

    if (sidebarSection !== 'connections' && !deployInfo) {
      return;
    }

    const timer = window.setInterval(() => {
      void refreshConnections();
    }, 2_500);

    return () => {
      window.clearInterval(timer);
    };
  }, [deployInfo, sidebarSection]);

  // 自适应窗口大小调整。
  useEffect(() => {
    if (!hasTauriRuntime()) {
      return;
    }

    let alive = true;
    let cleanup: (() => void) | null = null;

    void enableAdaptiveWindowSizing().then((nextCleanup) => {
      if (alive) {
        cleanup = nextCleanup;
      } else {
        nextCleanup();
      }
    });

    return () => {
      alive = false;
      cleanup?.();
    };
  }, []);

  // 全局 window error / unhandledrejection 捕获。
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

  // Tauri 事件监听（node-status、result、deployed、undeployed）。
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
      const activeWorkflowId = activeWorkflowIdRef.current;
      if (activeWorkflowId && payload.workflowId !== activeWorkflowId) {
        return;
      }

      const parsedEvent = parseWorkflowEventPayload(payload.event);
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
      const activeWorkflowId = activeWorkflowIdRef.current;
      if (activeWorkflowId && payload.workflowId !== activeWorkflowId) {
        return;
      }

      setResults((current) => [payload.result, ...current].slice(0, 8));
    }).then((cleanup) => {
      if (alive) {
        cleanups.push(cleanup);
      }
    });

    void onWorkflowDeployed((payload) => {
      if (!alive) {
        return;
      }

      applyDeploymentState(payload);
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

    void onRuntimeWorkflowFocus((payload) => {
      if (!alive) {
        return;
      }

      setDeployInfo(toDeployInfoFromSummary(payload));
      setEventFeed([
        buildRuntimeLogEntry(
          'system',
          'info',
          '已切换当前运行工作流',
          payload.projectName ?? payload.workflowId,
        ),
      ]);
      setAppErrors([]);
      setResults([]);
      setRuntimeState(EMPTY_RUNTIME_STATE);
      setStatusMessage(`已切换到 ${payload.projectName ?? payload.workflowId} 的运行上下文。`);
    }).then((cleanup) => {
      if (alive) {
        cleanups.push(cleanup);
      }
    });

    void listRuntimeWorkflows()
      .then((workflows) => {
        if (!alive) {
          return;
        }

        const activeWorkflow = workflows.find((workflow) => workflow.active);
        if (activeWorkflow) {
          setDeployInfo(toDeployInfoFromSummary(activeWorkflow));
        } else {
          setDeployInfo(null);
        }
      })
      .catch(() => {
        if (alive) {
          setDeployInfo(null);
        }
      });

    return () => {
      alive = false;
      for (const cleanup of cleanups) {
        cleanup();
      }
    };
  }, []);

  return {
    statusMessage,
    deployInfo,
    results,
    eventFeed,
    appErrors,
    connections,
    runtimeState,
    isRuntimeDockCollapsed,
    setStatusMessage,
    appendRuntimeLog,
    appendAppError,
    handleFlowgramError,
    resetWorkspaceRuntime,
    applyDeploymentState,
    addResult: (result: WorkflowResult) => {
      setResults((current) => [result, ...current].slice(0, 8));
    },
    refreshConnections,
    setIsRuntimeDockCollapsed,
  };
}
