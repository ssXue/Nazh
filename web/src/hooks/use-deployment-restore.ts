import { useCallback, useEffect, useMemo, useRef, useState } from 'react';

import {
  clearDeploymentSession,
  loadDeploymentSessionState,
  removeDeploymentSession,
  saveDeploymentSession,
  setDeploymentSessionActiveProject,
  type PersistedDeploymentSession,
} from '../lib/deployment-session';
import {
  arePersistedDeploymentSessionStatesEqual,
  getPreferredRestoreSession,
  mergePersistedDeploymentSessionStates,
  normalizePersistedDeploymentSessionState,
  sortPersistedDeploymentSessions,
} from '../lib/persisted-deployment-state';
import {
  clearDeploymentSessionFile,
  deployWorkflow,
  hasTauriRuntime,
  loadDeploymentSessionStateFile,
  removeDeploymentSessionFile,
  saveDeploymentSessionFile,
  setDeploymentSessionActiveProjectFile,
} from '../lib/tauri';
import { describeUnknownError } from '../lib/workflow-events';
import type {
  AppErrorRecord,
  ConnectionDefinition,
  DeployResponse,
  RuntimeLogEntry,
} from '../types';

interface DeploymentRunSnapshot {
  projectId: string;
  projectName: string;
  environmentId: string;
  environmentName: string;
  runtimeAstText: string;
  runtimeConnections: ConnectionDefinition[];
}

interface UseDeploymentRestoreOptions {
  workspacePath: string;
  projects: Array<{ id: string }>;
  projectStorageReady: boolean;
  connectionStorageReady: boolean;
  deployInfo: DeployResponse | null;
  runtimeWorkflowCount: number;
  appendAppError: (scope: AppErrorRecord['scope'], title: string, detail?: string | null) => void;
  appendRuntimeLog: (
    source: string,
    level: RuntimeLogEntry['level'],
    message: string,
    detail?: string | null,
  ) => void;
  applyDeploymentState: (payload: DeployResponse, nextMessage?: string) => void;
  refreshConnections: () => Promise<void>;
  setStatusMessage: (message: string) => void;
  onRestoreProject: (projectId: string) => void;
}

type RestoreLookupStatus = 'idle' | 'loading' | 'prompted' | 'handled' | 'none';

export function useDeploymentRestore({
  workspacePath,
  projects,
  projectStorageReady,
  connectionStorageReady,
  deployInfo,
  runtimeWorkflowCount,
  appendAppError,
  appendRuntimeLog,
  applyDeploymentState,
  refreshConnections,
  setStatusMessage,
  onRestoreProject,
}: UseDeploymentRestoreOptions) {
  const [pendingRestoreSessions, setPendingRestoreSessions] = useState<PersistedDeploymentSession[]>([]);
  const [pendingRestoreActiveProjectId, setPendingRestoreActiveProjectId] = useState<string | null>(null);
  const [restoreCountdown, setRestoreCountdown] = useState(10);
  const [isRestoreCheckPaused, setIsRestoreCheckPaused] = useState(false);
  const deploymentRestoreScope = useMemo(() => workspacePath.trim() || '__default__', [workspacePath]);
  const restoreLookupRef = useRef<{
    scope: string | null;
    status: RestoreLookupStatus;
  }>({
    scope: null,
    status: 'idle',
  });

  const pendingRestoreLeadSession = useMemo(
    () => getPreferredRestoreSession(pendingRestoreSessions, pendingRestoreActiveProjectId),
    [pendingRestoreActiveProjectId, pendingRestoreSessions],
  );

  const clearPendingRestore = useCallback(() => {
    setPendingRestoreSessions([]);
    setPendingRestoreActiveProjectId(null);
    setRestoreCountdown(10);
  }, []);

  const persistDeploymentSnapshot = useCallback(
    async (snapshot: DeploymentRunSnapshot) => {
      const session = {
        version: 1 as const,
        projectId: snapshot.projectId,
        projectName: snapshot.projectName,
        environmentId: snapshot.environmentId,
        environmentName: snapshot.environmentName,
        deployedAt: new Date().toISOString(),
        runtimeAstText: snapshot.runtimeAstText,
        runtimeConnections: snapshot.runtimeConnections,
      };
      const activeProjectId = snapshot.projectId;

      saveDeploymentSession(workspacePath, session, activeProjectId);

      if (!hasTauriRuntime()) {
        return;
      }

      try {
        await saveDeploymentSessionFile(workspacePath, session, activeProjectId);
      } catch (error) {
        const { message, detail } = describeUnknownError(error);
        appendAppError('command', '写入部署会话失败，已降级为本地缓存', detail ?? message);
      }
    },
    [appendAppError, workspacePath],
  );

  const persistActiveDeploymentProject = useCallback(
    async (projectId: string | null) => {
      const targetProjectId = projectId?.trim() || null;
      setDeploymentSessionActiveProject(workspacePath, targetProjectId);

      if (!hasTauriRuntime()) {
        return;
      }

      try {
        await setDeploymentSessionActiveProjectFile(workspacePath, targetProjectId);
      } catch (error) {
        const { message, detail } = describeUnknownError(error);
        appendAppError('command', '更新主控工作流失败，已降级为本地缓存', detail ?? message);
      }
    },
    [appendAppError, workspacePath],
  );

  const removePersistedDeploymentSnapshot = useCallback(
    async (projectId: string) => {
      const targetProjectId = projectId.trim();
      if (!targetProjectId) {
        return;
      }

      removeDeploymentSession(workspacePath, targetProjectId);

      if (!hasTauriRuntime()) {
        return;
      }

      try {
        await removeDeploymentSessionFile(workspacePath, targetProjectId);
      } catch (error) {
        const { message, detail } = describeUnknownError(error);
        appendAppError('command', '清理部署会话失败', detail ?? message);
      }
    },
    [appendAppError, workspacePath],
  );

  const clearPersistedDeploymentSnapshots = useCallback(async () => {
    clearDeploymentSession(workspacePath);

    if (!hasTauriRuntime()) {
      return;
    }

    try {
      await clearDeploymentSessionFile(workspacePath);
    } catch (error) {
      const { message, detail } = describeUnknownError(error);
      appendAppError('command', '清理部署会话失败', detail ?? message);
    }
  }, [appendAppError, workspacePath]);

  const loadPersistedDeploymentSnapshots = useCallback(async () => {
    const localState = normalizePersistedDeploymentSessionState(
      loadDeploymentSessionState(workspacePath),
    );

    if (!hasTauriRuntime()) {
      return localState;
    }

    let fileState = normalizePersistedDeploymentSessionState({
      sessions: [],
      activeProjectId: null,
    });
    let fileLoaded = false;

    try {
      fileState = normalizePersistedDeploymentSessionState(
        await loadDeploymentSessionStateFile(workspacePath),
      );
      fileLoaded = true;
    } catch (error) {
      const { message, detail } = describeUnknownError(error);
      appendAppError('command', '读取部署会话失败，尝试使用本地缓存', detail ?? message);
    }

    const mergedState = mergePersistedDeploymentSessionStates(fileState, localState);

    if (!fileLoaded) {
      return mergedState;
    }

    const localHasFallbackState =
      localState.sessions.length > 0 || localState.activeProjectId !== null;
    const fileNeedsSync = !arePersistedDeploymentSessionStatesEqual(fileState, mergedState);

    if (fileNeedsSync) {
      try {
        if (mergedState.sessions.length === 0) {
          await clearDeploymentSessionFile(workspacePath);
        } else {
          for (const session of mergedState.sessions) {
            await saveDeploymentSessionFile(workspacePath, session);
          }
          await setDeploymentSessionActiveProjectFile(workspacePath, mergedState.activeProjectId);
        }
        clearDeploymentSession(workspacePath);
      } catch (error) {
        const { message, detail } = describeUnknownError(error);
        appendAppError('command', '迁移旧部署会话失败', detail ?? message);
        return mergedState;
      }
    } else if (localHasFallbackState) {
      clearDeploymentSession(workspacePath);
    }

    return mergedState;
  }, [appendAppError, workspacePath]);

  const runDeploymentSnapshot = useCallback(
    async (snapshot: DeploymentRunSnapshot, source: 'manual' | 'restore') => {
      if (!hasTauriRuntime()) {
        const statusMessage =
          source === 'restore'
            ? `预览模式下已跳过 ${snapshot.projectName} 的自动恢复部署。`
            : `预览模式下已完成 ${snapshot.environmentName} 的部署校验。`;
        setStatusMessage(statusMessage);
        appendRuntimeLog(
          'project',
          'info',
          source === 'restore' ? '预览模式下跳过自动恢复部署' : '预览模式下跳过实际部署',
          `${snapshot.projectName} · ${snapshot.environmentName}`,
        );
        return true;
      }

      try {
        const response = await deployWorkflow(
          snapshot.runtimeAstText,
          snapshot.runtimeConnections,
          {
            workspacePath,
            projectId: snapshot.projectId,
            projectName: snapshot.projectName,
            environmentId: snapshot.environmentId,
            environmentName: snapshot.environmentName,
            deploymentSource: source,
          },
          {
            workflowId: snapshot.projectId,
          },
        );
        applyDeploymentState(
          response,
          source === 'restore'
            ? `已恢复 ${snapshot.projectName} 的部署，节点数 ${response.nodeCount}，边数 ${response.edgeCount}，环境 ${snapshot.environmentName}。`
            : `部署完成，节点数 ${response.nodeCount}，边数 ${response.edgeCount}，环境 ${snapshot.environmentName}。`,
        );
        await persistDeploymentSnapshot(snapshot);
        if (source === 'restore') {
          appendRuntimeLog(
            'system',
            'success',
            '已恢复上次部署',
            `${snapshot.projectName} · ${snapshot.environmentName}`,
          );
        }
        await refreshConnections();
        return true;
      } catch (error) {
        const { message, detail } = describeUnknownError(error);
        appendAppError(
          'command',
          source === 'restore' ? '自动恢复部署失败' : '部署工作流失败',
          detail ?? message,
        );
        setStatusMessage(source === 'restore' ? `自动恢复部署失败: ${message}` : message);
        return false;
      }
    },
    [
      appendAppError,
      appendRuntimeLog,
      applyDeploymentState,
      persistDeploymentSnapshot,
      refreshConnections,
      setStatusMessage,
      workspacePath,
    ],
  );

  const beginRestoreCheckPause = useCallback(() => {
    restoreLookupRef.current = {
      scope: null,
      status: 'handled',
    };
    setIsRestoreCheckPaused(true);
    clearPendingRestore();
  }, [clearPendingRestore]);

  const endRestoreCheckPause = useCallback(() => {
    restoreLookupRef.current = {
      scope: deploymentRestoreScope,
      status: 'idle',
    };
    setIsRestoreCheckPaused(false);
  }, [deploymentRestoreScope]);

  const handleSkipRestore = useCallback(async () => {
    if (pendingRestoreSessions.length === 0) {
      return;
    }

    const skippedSessions = pendingRestoreSessions;
    const leadSession = getPreferredRestoreSession(
      skippedSessions,
      pendingRestoreActiveProjectId,
    );
    restoreLookupRef.current = {
      scope: deploymentRestoreScope,
      status: 'handled',
    };
    clearPendingRestore();
    await clearPersistedDeploymentSnapshots();
    setStatusMessage(
      skippedSessions.length > 1
        ? `已取消恢复最近 ${skippedSessions.length} 个工程的上次部署。`
        : `已取消恢复 ${leadSession?.projectName ?? '当前工程'} 的上次部署。`,
    );
    appendRuntimeLog(
      'system',
      'info',
      '已取消自动恢复部署',
      skippedSessions.length > 1
        ? `共 ${skippedSessions.length} 个工程`
        : `${leadSession?.projectName ?? '未知工程'} · ${leadSession?.environmentName ?? '默认环境'}`,
    );
  }, [
    appendRuntimeLog,
    clearPendingRestore,
    clearPersistedDeploymentSnapshots,
    deploymentRestoreScope,
    pendingRestoreActiveProjectId,
    pendingRestoreSessions,
    setStatusMessage,
  ]);

  const handleConfirmRestore = useCallback(
    async (sessions = pendingRestoreSessions) => {
      const restoreSessions = sortPersistedDeploymentSessions(sessions);
      if (restoreSessions.length === 0) {
        return;
      }

      const validSessions: PersistedDeploymentSession[] = [];
      const missingSessions: PersistedDeploymentSession[] = [];

      for (const session of restoreSessions) {
        const targetProject = projects.find((project) => project.id === session.projectId);
        if (targetProject) {
          validSessions.push(session);
        } else {
          missingSessions.push(session);
        }
      }

      restoreLookupRef.current = {
        scope: deploymentRestoreScope,
        status: 'handled',
      };
      clearPendingRestore();

      for (const missingSession of missingSessions) {
        await removePersistedDeploymentSnapshot(missingSession.projectId);
        appendRuntimeLog(
          'system',
          'warn',
          '恢复目标不存在，已清理部署记录',
          missingSession.projectName,
        );
      }

      if (validSessions.length === 0) {
        setStatusMessage('恢复失败：目标工程不存在，已清理部署记录。');
        return;
      }

      const restoreState = normalizePersistedDeploymentSessionState({
        sessions: validSessions,
        activeProjectId: pendingRestoreActiveProjectId,
      });
      const leadSession = getPreferredRestoreSession(
        restoreState.sessions,
        restoreState.activeProjectId,
      );
      const restoreQueue = [
        ...[
          ...restoreState.sessions.filter((session) => session.projectId !== leadSession?.projectId),
        ].reverse(),
        ...(leadSession ? [leadSession] : []),
      ];
      appendRuntimeLog(
        'system',
        'info',
        validSessions.length > 1 ? '正在批量恢复上次部署' : '正在恢复上次部署',
        validSessions.length > 1
          ? `共 ${validSessions.length} 个工程，主控工程为 ${leadSession?.projectName ?? validSessions[0].projectName}`
          : `${leadSession?.projectName ?? validSessions[0].projectName} · ${leadSession?.environmentName ?? validSessions[0].environmentName}`,
      );

      let restoredCount = 0;
      let lastSuccessfulSession: PersistedDeploymentSession | null = null;
      for (const session of restoreQueue) {
        const restored = await runDeploymentSnapshot(
          {
            projectId: session.projectId,
            projectName: session.projectName,
            environmentId: session.environmentId,
            environmentName: session.environmentName,
            runtimeAstText: session.runtimeAstText,
            runtimeConnections: session.runtimeConnections,
          },
          'restore',
        );
        if (restored) {
          restoredCount += 1;
          lastSuccessfulSession = session;
        }
      }

      if (lastSuccessfulSession) {
        onRestoreProject(lastSuccessfulSession.projectId);
      }

      if (validSessions.length > 1) {
        appendRuntimeLog(
          'system',
          restoredCount === validSessions.length ? 'success' : 'warn',
          '批量恢复完成',
          `成功 ${restoredCount}/${validSessions.length} 个工程`,
        );
      }
    },
    [
      appendRuntimeLog,
      clearPendingRestore,
      deploymentRestoreScope,
      onRestoreProject,
      pendingRestoreActiveProjectId,
      pendingRestoreSessions,
      projects,
      removePersistedDeploymentSnapshot,
      runDeploymentSnapshot,
      setStatusMessage,
    ],
  );

  useEffect(() => {
    restoreLookupRef.current = {
      scope: deploymentRestoreScope,
      status: 'idle',
    };
    clearPendingRestore();
    setIsRestoreCheckPaused(false);
  }, [clearPendingRestore, deploymentRestoreScope]);

  const handleConfirmRestoreRef = useRef(handleConfirmRestore);
  handleConfirmRestoreRef.current = handleConfirmRestore;

  useEffect(() => {
    if (pendingRestoreSessions.length === 0) {
      return;
    }

    if (restoreCountdown <= 0) {
      void handleConfirmRestoreRef.current(pendingRestoreSessions);
      return;
    }

    const timeoutId = window.setTimeout(() => {
      setRestoreCountdown((current) => current - 1);
    }, 1000);

    return () => {
      window.clearTimeout(timeoutId);
    };
  }, [pendingRestoreSessions, restoreCountdown]);

  useEffect(() => {
    if (pendingRestoreSessions.length === 0) {
      return;
    }

    const handleKeyDown = (event: KeyboardEvent) => {
      if (event.key === 'Escape') {
        void handleSkipRestore();
      }
    };

    window.addEventListener('keydown', handleKeyDown);
    return () => window.removeEventListener('keydown', handleKeyDown);
  }, [handleSkipRestore, pendingRestoreSessions]);

  useEffect(() => {
    if (!deployInfo || pendingRestoreSessions.length === 0) {
      return;
    }

    clearPendingRestore();
  }, [clearPendingRestore, deployInfo, pendingRestoreSessions]);

  useEffect(() => {
    if (
      !hasTauriRuntime() ||
      !projectStorageReady ||
      !connectionStorageReady ||
      isRestoreCheckPaused ||
      deployInfo ||
      runtimeWorkflowCount > 0
    ) {
      return;
    }

    if (restoreLookupRef.current.scope !== deploymentRestoreScope) {
      restoreLookupRef.current = {
        scope: deploymentRestoreScope,
        status: 'idle',
      };
    }

    if (restoreLookupRef.current.status !== 'idle') {
      return;
    }

    restoreLookupRef.current = {
      scope: deploymentRestoreScope,
      status: 'loading',
    };

    void loadPersistedDeploymentSnapshots().then((restoredState) => {
      if (restoreLookupRef.current.scope !== deploymentRestoreScope) {
        return;
      }

      if (restoredState.sessions.length === 0) {
        restoreLookupRef.current = {
          scope: deploymentRestoreScope,
          status: 'none',
        };
        return;
      }

      const knownSessions = restoredState.sessions.filter((session) =>
        projects.some((project) => project.id === session.projectId),
      );
      const unknownSessions = restoredState.sessions.filter(
        (session) => !knownSessions.some((item) => item.projectId === session.projectId),
      );

      if (unknownSessions.length > 0) {
        for (const session of unknownSessions) {
          void removePersistedDeploymentSnapshot(session.projectId);
        }
        appendRuntimeLog(
          'system',
          'warn',
          '已清理失效部署记录',
          unknownSessions.map((session) => session.projectName).join('、'),
        );
      }

      if (knownSessions.length === 0) {
        restoreLookupRef.current = {
          scope: deploymentRestoreScope,
          status: 'handled',
        };
        return;
      }

      const promptState = normalizePersistedDeploymentSessionState({
        sessions: knownSessions,
        activeProjectId: restoredState.activeProjectId,
      });
      const leadSession = getPreferredRestoreSession(
        promptState.sessions,
        promptState.activeProjectId,
      );
      appendRuntimeLog(
        'system',
        'warn',
        '检测到可恢复部署',
        knownSessions.length > 1
          ? `共 ${knownSessions.length} 个工程，主控工程为 ${leadSession?.projectName ?? knownSessions[0].projectName}`
          : `${leadSession?.projectName ?? knownSessions[0].projectName} · ${leadSession?.environmentName ?? knownSessions[0].environmentName}`,
      );
      setStatusMessage(
        knownSessions.length > 1
          ? `检测到 ${knownSessions.length} 个工程的上次部署，10 秒后将自动恢复。`
          : `检测到 ${leadSession?.projectName ?? knownSessions[0].projectName} 的上次部署，10 秒后将自动恢复。`,
      );
      restoreLookupRef.current = {
        scope: deploymentRestoreScope,
        status: 'prompted',
      };
      setPendingRestoreSessions(promptState.sessions);
      setPendingRestoreActiveProjectId(promptState.activeProjectId);
      setRestoreCountdown(10);
    });
  }, [
    appendRuntimeLog,
    connectionStorageReady,
    deployInfo,
    deploymentRestoreScope,
    isRestoreCheckPaused,
    loadPersistedDeploymentSnapshots,
    projectStorageReady,
    projects,
    removePersistedDeploymentSnapshot,
    runtimeWorkflowCount,
    setStatusMessage,
  ]);

  return {
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
  };
}
