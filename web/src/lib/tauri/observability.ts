import { invoke } from '@tauri-apps/api/core';

import type { ObservabilityQueryResult } from '../../types';

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

export async function clearObservability(workspacePath: string): Promise<void> {
  return invoke('clear_observability', {
    workspacePath: workspacePath.trim() || null,
  });
}
