import { useEffect, useMemo, useRef, useState, type Dispatch, type SetStateAction } from 'react';

import type { ConnectionDefinition } from '../types';
import {
  hasTauriRuntime,
  loadConnectionDefinitions,
  saveConnectionDefinitions,
} from '../lib/tauri';

const CONNECTION_LIBRARY_STORAGE_KEY = 'nazh.connection-library';

export interface ConnectionLibraryStorageState {
  isReady: boolean;
  isSyncing: boolean;
  error: string | null;
}

export interface UseConnectionLibraryResult {
  connections: ConnectionDefinition[];
  setConnections: Dispatch<SetStateAction<ConnectionDefinition[]>>;
  storage: ConnectionLibraryStorageState;
  refreshConnections: () => Promise<void>;
}

function buildLocalStorageKey(workspacePath: string): string {
  return workspacePath.trim()
    ? `${CONNECTION_LIBRARY_STORAGE_KEY}:${workspacePath.trim()}`
    : CONNECTION_LIBRARY_STORAGE_KEY;
}

function loadLocalConnections(workspacePath: string): ConnectionDefinition[] {
  if (typeof window === 'undefined') {
    return [];
  }

  try {
    const raw = window.localStorage.getItem(buildLocalStorageKey(workspacePath));
    if (!raw) {
      return [];
    }

    const parsed = JSON.parse(raw) as unknown;
    return Array.isArray(parsed) ? (parsed as ConnectionDefinition[]) : [];
  } catch {
    return [];
  }
}

function persistLocalConnections(workspacePath: string, connections: ConnectionDefinition[]) {
  if (typeof window === 'undefined') {
    return;
  }

  try {
    window.localStorage.setItem(
      buildLocalStorageKey(workspacePath),
      JSON.stringify(connections),
    );
  } catch {
    // Ignore preview persistence failures.
  }
}

function describeConnectionStorageError(error: unknown): string {
  if (typeof error === 'string' && error.trim()) {
    return error;
  }

  if (error instanceof Error && error.message.trim()) {
    return error.message;
  }

  return '连接资源同步失败。';
}

export function useConnectionLibrary(workspacePath = ''): UseConnectionLibraryResult {
  const desktopStorageEnabled = hasTauriRuntime();
  const normalizedWorkspacePath = workspacePath.trim();
  const [connections, setConnections] = useState<ConnectionDefinition[]>(() =>
    desktopStorageEnabled ? [] : loadLocalConnections(normalizedWorkspacePath),
  );
  const [storage, setStorage] = useState<ConnectionLibraryStorageState>(() => ({
    isReady: !desktopStorageEnabled,
    isSyncing: false,
    error: null,
  }));
  const [hydratedWorkspacePath, setHydratedWorkspacePath] = useState<string | null>(
    desktopStorageEnabled ? null : normalizedWorkspacePath,
  );
  const lastSyncedSignatureRef = useRef<string | null>(null);
  const lastFailedSignatureRef = useRef<string | null>(null);
  const saveQueueRef = useRef(Promise.resolve());

  const connectionSignature = useMemo(() => JSON.stringify(connections), [connections]);

  useEffect(() => {
    if (!desktopStorageEnabled) {
      const localConnections = loadLocalConnections(normalizedWorkspacePath);
      setConnections(localConnections);
      setStorage({
        isReady: true,
        isSyncing: false,
        error: null,
      });
      setHydratedWorkspacePath(normalizedWorkspacePath);
      lastSyncedSignatureRef.current = JSON.stringify(localConnections);
      lastFailedSignatureRef.current = null;
      return;
    }

    let cancelled = false;

    setStorage({
      isReady: false,
      isSyncing: true,
      error: null,
    });

    void loadConnectionDefinitions(normalizedWorkspacePath)
      .then((definitions) => {
        if (cancelled) {
          return;
        }

        setConnections(definitions);
        setStorage({
          isReady: true,
          isSyncing: false,
          error: null,
        });
        setHydratedWorkspacePath(normalizedWorkspacePath);
        lastSyncedSignatureRef.current = JSON.stringify(definitions);
        lastFailedSignatureRef.current = null;
      })
      .catch((error) => {
        if (cancelled) {
          return;
        }

        setConnections([]);
        setStorage({
          isReady: true,
          isSyncing: false,
          error: describeConnectionStorageError(error),
        });
      });

    return () => {
      cancelled = true;
    };
  }, [desktopStorageEnabled, normalizedWorkspacePath]);

  useEffect(() => {
    if (!desktopStorageEnabled) {
      persistLocalConnections(normalizedWorkspacePath, connections);
      lastSyncedSignatureRef.current = connectionSignature;
      return;
    }

    if (!storage.isReady || hydratedWorkspacePath !== normalizedWorkspacePath) {
      return;
    }

    if (lastSyncedSignatureRef.current === connectionSignature) {
      return;
    }

    if (lastFailedSignatureRef.current === connectionSignature) {
      return;
    }

    let cancelled = false;
    const pendingConnections = JSON.parse(connectionSignature) as ConnectionDefinition[];

    setStorage((current) => ({
      ...current,
      isSyncing: true,
      error: null,
    }));

    saveQueueRef.current = saveQueueRef.current
      .catch(() => undefined)
      .then(async () => {
        await saveConnectionDefinitions(normalizedWorkspacePath, pendingConnections);
      });

    void saveQueueRef.current
      .then(() => {
        if (cancelled) {
          return;
        }

        lastSyncedSignatureRef.current = connectionSignature;
        lastFailedSignatureRef.current = null;
        setStorage({
          isReady: true,
          isSyncing: false,
          error: null,
        });
      })
      .catch((error) => {
        if (cancelled) {
          return;
        }

        lastFailedSignatureRef.current = connectionSignature;
        setStorage((current) => ({
          ...current,
          isSyncing: false,
          error: describeConnectionStorageError(error),
        }));
      });

    return () => {
      cancelled = true;
    };
  }, [
    connectionSignature,
    connections,
    desktopStorageEnabled,
    hydratedWorkspacePath,
    normalizedWorkspacePath,
    storage.isReady,
  ]);

  async function refreshConnections() {
    if (!desktopStorageEnabled) {
      const localConnections = loadLocalConnections(normalizedWorkspacePath);
      setConnections(localConnections);
      setStorage({
        isReady: true,
        isSyncing: false,
        error: null,
      });
      lastSyncedSignatureRef.current = JSON.stringify(localConnections);
      lastFailedSignatureRef.current = null;
      return;
    }

    setStorage((current) => ({
      ...current,
      isSyncing: true,
      error: null,
    }));

    try {
      const definitions = await loadConnectionDefinitions(normalizedWorkspacePath);
      setConnections(definitions);
      setStorage({
        isReady: true,
        isSyncing: false,
        error: null,
      });
      setHydratedWorkspacePath(normalizedWorkspacePath);
      lastSyncedSignatureRef.current = JSON.stringify(definitions);
      lastFailedSignatureRef.current = null;
    } catch (error) {
      setStorage((current) => ({
        ...current,
        isSyncing: false,
        error: describeConnectionStorageError(error),
      }));
    }
  }

  return {
    connections,
    setConnections,
    storage,
    refreshConnections,
  };
}
