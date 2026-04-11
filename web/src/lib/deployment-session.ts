import type { ConnectionDefinition } from '../types';

export const DEPLOYMENT_SESSION_STORAGE_KEY = 'nazh.deployment-session';

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

export function loadDeploymentSession(workspacePath = ''): PersistedDeploymentSession | null {
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
    if (!isPersistedDeploymentSession(parsed)) {
      window.localStorage.removeItem(storageKey);
      return null;
    }

    return parsed;
  } catch {
    window.localStorage.removeItem(storageKey);
    return null;
  }
}

export function saveDeploymentSession(
  workspacePath: string,
  session: PersistedDeploymentSession,
) {
  if (typeof window === 'undefined') {
    return;
  }

  try {
    window.localStorage.setItem(buildStorageKey(workspacePath), JSON.stringify(session));
  } catch {
    // Ignore preview persistence failures.
  }
}

export function clearDeploymentSession(workspacePath = '') {
  if (typeof window === 'undefined') {
    return;
  }

  window.localStorage.removeItem(buildStorageKey(workspacePath));
}
