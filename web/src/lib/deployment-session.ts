import type { ConnectionDefinition } from '../types';

export const DEPLOYMENT_SESSION_STORAGE_KEY = 'nazh.deployment-session';
const DEPLOYMENT_SESSION_COLLECTION_VERSION = 3;

export interface PersistedDeploymentSession {
  version: 1;
  projectId: string;
  projectName: string;
  environmentId: string;
  environmentName: string;
  deployedAt: string;
  runtimeAstText: string;
  runtimeConnections: ConnectionDefinition[];
}

interface PersistedDeploymentSessionCollection {
  version: 3;
  sessions: PersistedDeploymentSession[];
  activeProjectId?: string | null;
}

function buildStorageKey(workspacePath: string): string {
  return workspacePath.trim()
    ? `${DEPLOYMENT_SESSION_STORAGE_KEY}:${workspacePath.trim()}`
    : DEPLOYMENT_SESSION_STORAGE_KEY;
}

function isPersistedDeploymentSession(value: unknown): value is PersistedDeploymentSession {
  if (!value || typeof value !== 'object') {
    return false;
  }

  const record = value as Record<string, unknown>;
  return (
    record.version === 1 &&
    typeof record.projectId === 'string' &&
    typeof record.projectName === 'string' &&
    typeof record.environmentId === 'string' &&
    typeof record.environmentName === 'string' &&
    typeof record.deployedAt === 'string' &&
    typeof record.runtimeAstText === 'string' &&
    Array.isArray(record.runtimeConnections)
  );
}

function isPersistedDeploymentSessionCollection(
  value: unknown,
): value is PersistedDeploymentSessionCollection {
  if (!value || typeof value !== 'object') {
    return false;
  }

  const record = value as Record<string, unknown>;
  return (
    record.version === DEPLOYMENT_SESSION_COLLECTION_VERSION &&
    Array.isArray(record.sessions) &&
    record.sessions.every((entry) => isPersistedDeploymentSession(entry)) &&
    (record.activeProjectId === undefined ||
      record.activeProjectId === null ||
      typeof record.activeProjectId === 'string')
  );
}

function sortSessionsByFreshness(sessions: PersistedDeploymentSession[]) {
  return sessions.sort((left, right) => {
    const leftTime = Date.parse(left.deployedAt);
    const rightTime = Date.parse(right.deployedAt);

    if (Number.isNaN(leftTime) || Number.isNaN(rightTime)) {
      return right.projectId.localeCompare(left.projectId);
    }

    return rightTime - leftTime;
  });
}

function normalizeSessions(
  sessions: PersistedDeploymentSession[],
): PersistedDeploymentSession[] {
  const deduped = new Map<string, PersistedDeploymentSession>();

  for (const session of sortSessionsByFreshness([...sessions])) {
    if (!deduped.has(session.projectId)) {
      deduped.set(session.projectId, session);
    }
  }

  return [...deduped.values()];
}

export interface PersistedDeploymentSessionState {
  version: 3;
  sessions: PersistedDeploymentSession[];
  activeProjectId: string | null;
}

function normalizeState(state: {
  sessions: PersistedDeploymentSession[];
  activeProjectId?: string | null;
}): PersistedDeploymentSessionState {
  const sessions = normalizeSessions(state.sessions);
  const activeProjectId =
    state.activeProjectId?.trim() && sessions.some((session) => session.projectId === state.activeProjectId)
      ? state.activeProjectId
      : null;

  return {
    version: DEPLOYMENT_SESSION_COLLECTION_VERSION,
    sessions,
    activeProjectId,
  };
}

function readStoredState(workspacePath = ''): PersistedDeploymentSessionState | null {
  if (typeof window === 'undefined') {
    return null;
  }

  const storageKey = buildStorageKey(workspacePath);

  try {
    const raw = window.localStorage.getItem(storageKey);
    if (!raw) {
      return null;
    }

    const parsed = JSON.parse(raw) as unknown;
    if (isPersistedDeploymentSessionCollection(parsed)) {
      return normalizeState(parsed);
    }

    if (isPersistedDeploymentSession(parsed)) {
      return normalizeState({ sessions: [parsed] });
    }

    if (Array.isArray(parsed) && parsed.every((entry) => isPersistedDeploymentSession(entry))) {
      return normalizeState({ sessions: parsed });
    }

    if (!isPersistedDeploymentSession(parsed)) {
      window.localStorage.removeItem(storageKey);
      return null;
    }

    return normalizeState({ sessions: [parsed] });
  } catch {
    window.localStorage.removeItem(storageKey);
    return null;
  }
}

function writeStoredState(
  workspacePath: string,
  state: PersistedDeploymentSessionState,
) {
  if (typeof window === 'undefined') {
    return;
  }

  try {
    const normalized = normalizeState(state);
    const payload: PersistedDeploymentSessionCollection = normalized;
    window.localStorage.setItem(buildStorageKey(workspacePath), JSON.stringify(payload));
  } catch {
    // Ignore preview persistence failures.
  }
}

export function loadDeploymentSessionState(workspacePath = ''): PersistedDeploymentSessionState {
  return (
    readStoredState(workspacePath) ?? {
      version: DEPLOYMENT_SESSION_COLLECTION_VERSION,
      sessions: [],
      activeProjectId: null,
    }
  );
}

export function loadDeploymentSessions(workspacePath = ''): PersistedDeploymentSession[] {
  return loadDeploymentSessionState(workspacePath).sessions;
}

export function loadDeploymentSession(workspacePath = ''): PersistedDeploymentSession | null {
  return loadDeploymentSessions(workspacePath)[0] ?? null;
}

export function saveDeploymentSession(
  workspacePath: string,
  session: PersistedDeploymentSession,
  activeProjectId?: string | null,
) {
  const state = loadDeploymentSessionState(workspacePath);
  const current = state.sessions.filter(
    (entry) => entry.projectId !== session.projectId,
  );
  writeStoredState(workspacePath, {
    version: DEPLOYMENT_SESSION_COLLECTION_VERSION,
    sessions: [session, ...current],
    activeProjectId: activeProjectId === undefined ? state.activeProjectId : activeProjectId,
  });
}

export function setDeploymentSessionActiveProject(
  workspacePath: string,
  projectId: string | null,
) {
  const state = loadDeploymentSessionState(workspacePath);
  const activeProjectId = projectId?.trim() || null;

  if (state.sessions.length === 0 && !activeProjectId) {
    clearDeploymentSession(workspacePath);
    return;
  }

  writeStoredState(workspacePath, {
    ...state,
    activeProjectId,
  });
}

export function removeDeploymentSession(workspacePath: string, projectId: string) {
  const state = loadDeploymentSessionState(workspacePath);
  const nextSessions = state.sessions.filter(
    (entry) => entry.projectId !== projectId,
  );

  if (nextSessions.length === 0) {
    clearDeploymentSession(workspacePath);
    return;
  }

  writeStoredState(workspacePath, {
    version: DEPLOYMENT_SESSION_COLLECTION_VERSION,
    sessions: nextSessions,
    activeProjectId:
      state.activeProjectId && state.activeProjectId !== projectId ? state.activeProjectId : null,
  });
}

export function clearDeploymentSession(workspacePath = '') {
  if (typeof window === 'undefined') {
    return;
  }

  window.localStorage.removeItem(buildStorageKey(workspacePath));
}
