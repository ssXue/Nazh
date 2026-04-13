import type { ConnectionDefinition } from '../types';

export const DEPLOYMENT_SESSION_STORAGE_KEY = 'nazh.deployment-session';
const DEPLOYMENT_SESSION_COLLECTION_VERSION = 2;

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
  version: 2;
  sessions: PersistedDeploymentSession[];
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
    record.sessions.every((entry) => isPersistedDeploymentSession(entry))
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

function readStoredSessions(workspacePath = ''): PersistedDeploymentSession[] | null {
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
      return normalizeSessions(parsed.sessions);
    }

    if (isPersistedDeploymentSession(parsed)) {
      return [parsed];
    }

    if (Array.isArray(parsed) && parsed.every((entry) => isPersistedDeploymentSession(entry))) {
      return normalizeSessions(parsed);
    }

    if (!isPersistedDeploymentSession(parsed)) {
      window.localStorage.removeItem(storageKey);
      return null;
    }

    return [parsed];
  } catch {
    window.localStorage.removeItem(storageKey);
    return null;
  }
}

function writeStoredSessions(
  workspacePath: string,
  sessions: PersistedDeploymentSession[],
) {
  if (typeof window === 'undefined') {
    return;
  }

  try {
    const payload: PersistedDeploymentSessionCollection = {
      version: DEPLOYMENT_SESSION_COLLECTION_VERSION,
      sessions: normalizeSessions(sessions),
    };
    window.localStorage.setItem(buildStorageKey(workspacePath), JSON.stringify(payload));
  } catch {
    // Ignore preview persistence failures.
  }
}

export function loadDeploymentSessions(workspacePath = ''): PersistedDeploymentSession[] {
  return readStoredSessions(workspacePath) ?? [];
}

export function loadDeploymentSession(workspacePath = ''): PersistedDeploymentSession | null {
  return loadDeploymentSessions(workspacePath)[0] ?? null;
}

export function saveDeploymentSession(
  workspacePath: string,
  session: PersistedDeploymentSession,
) {
  const current = loadDeploymentSessions(workspacePath).filter(
    (entry) => entry.projectId !== session.projectId,
  );
  writeStoredSessions(workspacePath, [session, ...current]);
}

export function removeDeploymentSession(workspacePath: string, projectId: string) {
  const nextSessions = loadDeploymentSessions(workspacePath).filter(
    (entry) => entry.projectId !== projectId,
  );

  if (nextSessions.length === 0) {
    clearDeploymentSession(workspacePath);
    return;
  }

  writeStoredSessions(workspacePath, nextSessions);
}

export function clearDeploymentSession(workspacePath = '') {
  if (typeof window === 'undefined') {
    return;
  }

  window.localStorage.removeItem(buildStorageKey(workspacePath));
}
