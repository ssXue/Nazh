import type {
  PersistedDeploymentSession,
  PersistedDeploymentSessionState,
} from './deployment-session';

export function sortPersistedDeploymentSessions(
  sessions: PersistedDeploymentSession[],
): PersistedDeploymentSession[] {
  return [...sessions].sort((left, right) => {
    const leftTime = Date.parse(left.deployedAt);
    const rightTime = Date.parse(right.deployedAt);

    if (Number.isNaN(leftTime) || Number.isNaN(rightTime)) {
      return right.projectId.localeCompare(left.projectId);
    }

    return rightTime - leftTime;
  });
}

export function normalizePersistedDeploymentSessionState(state: {
  sessions: PersistedDeploymentSession[];
  activeProjectId?: string | null;
}): PersistedDeploymentSessionState {
  const deduped = new Map<string, PersistedDeploymentSession>();

  for (const session of sortPersistedDeploymentSessions(state.sessions)) {
    if (!deduped.has(session.projectId)) {
      deduped.set(session.projectId, session);
    }
  }

  const sessions = [...deduped.values()];
  const activeProjectId = state.activeProjectId?.trim() || null;

  return {
    version: 3,
    sessions,
    activeProjectId:
      activeProjectId && sessions.some((session) => session.projectId === activeProjectId)
        ? activeProjectId
        : null,
  };
}

export function mergePersistedDeploymentSessionStates(
  fileState: PersistedDeploymentSessionState,
  localFallbackState: PersistedDeploymentSessionState,
): PersistedDeploymentSessionState {
  const merged = normalizePersistedDeploymentSessionState({
    sessions: [...fileState.sessions, ...localFallbackState.sessions],
  });
  const activeProjectId =
    [localFallbackState.activeProjectId, fileState.activeProjectId]
      .map((value) => value?.trim() || null)
      .find(
        (value): value is string =>
          Boolean(value) && merged.sessions.some((session) => session.projectId === value),
      ) ?? null;

  return {
    ...merged,
    activeProjectId,
  };
}

export function arePersistedDeploymentSessionStatesEqual(
  left: PersistedDeploymentSessionState,
  right: PersistedDeploymentSessionState,
) {
  return JSON.stringify(left) === JSON.stringify(right);
}

export function getPreferredRestoreSession(
  sessions: PersistedDeploymentSession[],
  activeProjectId: string | null,
) {
  const normalizedSessions = sortPersistedDeploymentSessions(sessions);
  const targetProjectId = activeProjectId?.trim() || null;

  if (targetProjectId) {
    const matchedSession = normalizedSessions.find((session) => session.projectId === targetProjectId);
    if (matchedSession) {
      return matchedSession;
    }
  }

  return normalizedSessions[0] ?? null;
}
