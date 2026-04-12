import type { AppErrorRecord, ObservabilityEntry, RuntimeLogEntry } from '../../types';

export interface RuntimeConsoleEntry {
  id: string;
  timestamp: number;
  level: RuntimeLogEntry['level'];
  source: string;
  message: string;
  detail?: string | null;
  tag?: string | null;
  channel: 'event' | 'alert' | 'audit' | 'exception';
  scope?: AppErrorRecord['scope'] | null;
  traceId?: string | null;
  nodeId?: string | null;
  durationMs?: number | null;
  payload?: unknown;
}

export function formatLogTimestamp(timestamp: number): string {
  return new Intl.DateTimeFormat('zh-CN', {
    hour: '2-digit',
    minute: '2-digit',
    second: '2-digit',
  }).format(timestamp);
}

export function formatLogDate(timestamp: number): string {
  return new Intl.DateTimeFormat('zh-CN', {
    month: '2-digit',
    day: '2-digit',
  }).format(timestamp);
}

function getErrorScopeLabel(scope: AppErrorRecord['scope']): string {
  switch (scope) {
    case 'workflow':
      return '工作流';
    case 'command':
      return '命令';
    case 'frontend':
      return '前端';
    case 'runtime':
      return '运行时';
  }
}

function normalizeConsoleSignatureText(value: string): string {
  return value.replace(/\s+/g, ' ').trim();
}

function isMirroredWorkflowFailure(
  error: AppErrorRecord,
  eventFeed: RuntimeLogEntry[],
): boolean {
  const matchedNode = error.title.match(/^节点\s+(.+?)\s+执行失败$/);
  if (!matchedNode) {
    return false;
  }

  const nodeId = matchedNode[1];
  const normalizedDetail = normalizeConsoleSignatureText(error.detail ?? '');

  return eventFeed.some(
    (entry) =>
      entry.level === 'error' &&
      entry.source === nodeId &&
      entry.message === '节点执行失败' &&
      normalizeConsoleSignatureText(entry.detail ?? '') === normalizedDetail,
  );
}

function isMirroredFlowgramError(
  error: AppErrorRecord,
  eventFeed: RuntimeLogEntry[],
): boolean {
  return eventFeed.some(
    (entry) =>
      entry.level === 'error' &&
      entry.source === 'flowgram' &&
      normalizeConsoleSignatureText(entry.message) === normalizeConsoleSignatureText(error.title) &&
      normalizeConsoleSignatureText(entry.detail ?? '') ===
        normalizeConsoleSignatureText(error.detail ?? ''),
  );
}

export function buildRuntimeConsoleEntries(
  eventFeed: RuntimeLogEntry[],
  appErrors: AppErrorRecord[],
  observabilityEntries?: ObservabilityEntry[] | null,
): RuntimeConsoleEntry[] {
  const eventEntries: RuntimeConsoleEntry[] =
    observabilityEntries && observabilityEntries.length > 0
      ? observabilityEntries.map((entry) => ({
          id: entry.id,
          timestamp: Date.parse(entry.timestamp) || Date.now(),
          level: entry.level,
          source: entry.source,
          message: entry.message,
          detail: entry.detail,
          tag:
            entry.category === 'alert'
              ? '告警'
              : entry.category === 'audit'
                ? '审计'
                : entry.category === 'result'
                  ? '结果'
                  : null,
          channel:
            entry.category === 'alert'
              ? 'alert'
              : entry.category === 'audit'
                ? 'audit'
                : 'event',
          scope: null,
          traceId: entry.traceId ?? null,
          nodeId: entry.nodeId ?? null,
          durationMs: entry.durationMs ?? null,
          payload: entry.data,
        }))
      : eventFeed.map((entry) => ({
          id: entry.id,
          timestamp: entry.timestamp,
          level: entry.level,
          source: entry.source,
          message: entry.message,
          detail: entry.detail,
          tag: null,
          channel: 'event',
          scope: null,
        }));

  const capturedErrorEntries: RuntimeConsoleEntry[] = appErrors
    .filter((error) => {
      if (error.scope === 'workflow') {
        return !isMirroredWorkflowFailure(error, eventFeed);
      }

      if (error.scope === 'frontend') {
        return !isMirroredFlowgramError(error, eventFeed);
      }

      return true;
    })
    .map((error) => ({
      id: error.id,
      timestamp: error.timestamp,
      level: 'error' as const,
      source: getErrorScopeLabel(error.scope),
      message: error.title,
      detail: error.detail,
      tag: '异常',
      channel: 'exception' as const,
      scope: error.scope,
    }));

  return [...eventEntries, ...capturedErrorEntries].sort((left, right) => {
    if (left.timestamp === right.timestamp) {
      return left.id.localeCompare(right.id);
    }

    return left.timestamp - right.timestamp;
  });
}

export function buildEventFeedPlainText(entries: RuntimeConsoleEntry[]): string {
  return entries
    .map((entry) => {
      const prefix = entry.tag ? `[${entry.tag}] ` : '';
      const baseLine = `[${formatLogTimestamp(entry.timestamp)}] ${prefix}[${entry.source}] ${entry.message}`;
      return entry.detail ? `${baseLine}\n${entry.detail}` : baseLine;
    })
    .join('\n\n');
}
