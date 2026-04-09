import { useEffect, useMemo, useRef, useState } from 'react';
import { JsonView, collapseAllNested, darkStyles, defaultStyles } from 'react-json-view-lite';
import 'react-json-view-lite/dist/index.css';

import { CopyIcon, DockToggleIcon } from './AppIcons';
import type { RuntimeDockProps } from './types';

type RuntimeConsoleEntry = {
  id: string;
  timestamp: number;
  level: 'info' | 'success' | 'warn' | 'error';
  source: string;
  message: string;
  detail?: string | null;
  tag?: string | null;
};

function formatLogTimestamp(timestamp: number): string {
  return new Intl.DateTimeFormat('zh-CN', {
    hour: '2-digit',
    minute: '2-digit',
    second: '2-digit',
  }).format(timestamp);
}

function normalizeResultPayload(payload: unknown): Record<string, unknown> | unknown[] {
  if (Array.isArray(payload)) {
    return payload;
  }

  if (payload && typeof payload === 'object') {
    return payload as Record<string, unknown>;
  }

  return { value: payload };
}

function getErrorScopeLabel(scope: RuntimeDockProps['appErrors'][number]['scope']): string {
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
  error: RuntimeDockProps['appErrors'][number],
  eventFeed: RuntimeDockProps['eventFeed'],
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
  error: RuntimeDockProps['appErrors'][number],
  eventFeed: RuntimeDockProps['eventFeed'],
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

function buildRuntimeConsoleEntries(
  eventFeed: RuntimeDockProps['eventFeed'],
  appErrors: RuntimeDockProps['appErrors'],
): RuntimeConsoleEntry[] {
  const eventEntries: RuntimeConsoleEntry[] = eventFeed.map((entry) => ({
    id: entry.id,
    timestamp: entry.timestamp,
    level: entry.level,
    source: entry.source,
    message: entry.message,
    detail: entry.detail,
    tag: null,
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
      level: 'error',
      source: getErrorScopeLabel(error.scope),
      message: error.title,
      detail: error.detail,
      tag: '异常',
    }));

  return [...eventEntries, ...capturedErrorEntries].sort((left, right) => {
    if (left.timestamp === right.timestamp) {
      return left.id.localeCompare(right.id);
    }

    return left.timestamp - right.timestamp;
  });
}

function buildEventFeedPlainText(entries: RuntimeConsoleEntry[]): string {
  return entries
    .map((entry) => {
      const prefix = entry.tag ? `[${entry.tag}] ` : '';
      const baseLine = `[${formatLogTimestamp(entry.timestamp)}] ${prefix}[${entry.source}] ${entry.message}`;
      return entry.detail ? `${baseLine}\n${entry.detail}` : baseLine;
    })
    .join('\n\n');
}

export function RuntimeDock({
  eventFeed,
  appErrors,
  results,
  connectionPreview,
  themeMode,
  isCollapsed,
  onToggleCollapsed,
}: RuntimeDockProps) {
  const logViewportRef = useRef<HTMLDivElement | null>(null);
  const [hasCopiedEventFeed, setHasCopiedEventFeed] = useState(false);
  const runtimeConsoleEntries = useMemo(
    () => buildRuntimeConsoleEntries(eventFeed, appErrors),
    [appErrors, eventFeed],
  );
  const eventFeedText = useMemo(
    () => buildEventFeedPlainText(runtimeConsoleEntries),
    [runtimeConsoleEntries],
  );

  useEffect(() => {
    if (isCollapsed || !logViewportRef.current) {
      return;
    }

    logViewportRef.current.scrollTop = logViewportRef.current.scrollHeight;
  }, [isCollapsed, runtimeConsoleEntries]);

  useEffect(() => {
    if (!hasCopiedEventFeed) {
      return undefined;
    }

    const timer = window.setTimeout(() => {
      setHasCopiedEventFeed(false);
    }, 1600);

    return () => window.clearTimeout(timer);
  }, [hasCopiedEventFeed]);

  const handleCopyEventFeed = async () => {
    if (!eventFeedText.trim()) {
      return;
    }

    try {
      if (navigator.clipboard?.writeText) {
        await navigator.clipboard.writeText(eventFeedText);
      } else {
        const textarea = document.createElement('textarea');
        textarea.value = eventFeedText;
        textarea.setAttribute('readonly', 'true');
        textarea.style.position = 'absolute';
        textarea.style.left = '-9999px';
        document.body.appendChild(textarea);
        textarea.select();
        document.execCommand('copy');
        document.body.removeChild(textarea);
      }

      setHasCopiedEventFeed(true);
    } catch {
      setHasCopiedEventFeed(false);
    }
  };

  return (
    <section
      className={`runtime-dock ${isCollapsed ? 'is-collapsed' : ''}`}
      aria-live="polite"
    >
      <div className="runtime-dock__header">
        <button
          type="button"
          className="runtime-dock__toggle"
          aria-expanded={!isCollapsed}
          aria-controls="runtime-dock-grid"
          aria-label={isCollapsed ? '展开运行观测' : '收起运行观测'}
          title={isCollapsed ? '展开运行观测' : '收起运行观测'}
          onClick={onToggleCollapsed}
        >
          <DockToggleIcon width={14} height={14} />
        </button>
      </div>

      <div
        id="runtime-dock-grid"
        className="runtime-dock__grid"
      >
        <section className="runtime-dock__panel">
          <div className="runtime-dock__panel-header">
            <div>
              <h3>连接池</h3>
            </div>
          </div>

          <div className="runtime-dock__panel-body">
            <div className="rail-list">
              {connectionPreview.length === 0 ? (
                <p>暂无连接</p>
              ) : (
                connectionPreview.map((connection) => (
                  <article key={connection.id} className="rail-card">
                    <strong>{connection.id}</strong>
                    <span>{connection.kind}</span>
                    <p>{connection.in_use ? '借出中' : '空闲'}</p>
                  </article>
                ))
              )}
            </div>

          </div>
        </section>

        <section className="runtime-dock__panel runtime-dock__panel--feed">
          <div className="runtime-dock__panel-header">
            <div>
              <h3>执行事件流</h3>
            </div>
            <div className="runtime-dock__panel-actions">
              <button
                type="button"
                className={`ghost runtime-dock__panel-tool ${hasCopiedEventFeed ? 'is-active' : ''}`}
                onClick={() => void handleCopyEventFeed()}
                disabled={!eventFeedText.trim()}
                aria-label={hasCopiedEventFeed ? '执行事件流已复制' : '复制执行事件流'}
                title={hasCopiedEventFeed ? '已复制' : '复制执行事件流'}
              >
                <CopyIcon width={14} height={14} />
              </button>
            </div>
          </div>

          <div className="runtime-dock__panel-body">
            <div ref={logViewportRef} className="runtime-log" role="log" aria-live="polite">
              {runtimeConsoleEntries.length === 0 ? (
                <p className="runtime-log__empty">暂无事件与异常</p>
              ) : (
                runtimeConsoleEntries.map((entry) => (
                  <div key={entry.id} className={`runtime-log__line is-${entry.level}`}>
                    <span className="runtime-log__time">{formatLogTimestamp(entry.timestamp)}</span>
                    <span className="runtime-log__source">
                      {entry.source}
                      {entry.tag ? <em className="runtime-log__badge">{entry.tag}</em> : null}
                    </span>
                    <span className="runtime-log__message">{entry.message}</span>
                    {entry.detail ? (
                      <div className="runtime-log__detail">{entry.detail}</div>
                    ) : null}
                  </div>
                ))
              )}
            </div>
          </div>
        </section>

        <section className="runtime-dock__panel runtime-dock__panel--feed">
          <div className="runtime-dock__panel-header">
            <div>
              <h3>结果载荷</h3>
            </div>
          </div>

          <div className="runtime-dock__panel-body">
            <div className="runtime-results">
              {results.length === 0 ? (
                <p className="runtime-results__empty">暂无输出</p>
              ) : (
                <div className="runtime-results__stream" role="list">
                  {results.map((result) => {
                    const resultKey = `${result.trace_id}-${result.timestamp}`;
                    const topLevelCount =
                      result.payload && typeof result.payload === 'object'
                        ? Object.keys(result.payload).length
                        : 1;

                    return (
                      <article
                        key={resultKey}
                        className="runtime-results__entry"
                        role="listitem"
                      >
                        <div className="runtime-results__entry-meta">
                          <strong>{formatLogTimestamp(new Date(result.timestamp).getTime())}</strong>
                          <span>{result.trace_id}</span>
                          <em>{`${topLevelCount} 个条目`}</em>
                        </div>
                        <div className="runtime-json-view">
                          <JsonView
                            data={normalizeResultPayload(result.payload)}
                            shouldExpandNode={collapseAllNested}
                            clickToExpandNode
                            style={themeMode === 'dark' ? darkStyles : defaultStyles}
                          />
                        </div>
                      </article>
                    );
                  })}
                </div>
              )}
            </div>
          </div>
        </section>
      </div>
    </section>
  );
}
