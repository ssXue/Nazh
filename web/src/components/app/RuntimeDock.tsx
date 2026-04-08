import { useEffect, useRef } from 'react';
import { JsonView, collapseAllNested, darkStyles, defaultStyles } from 'react-json-view-lite';
import 'react-json-view-lite/dist/index.css';

import { DockToggleIcon } from './AppIcons';
import type { RuntimeDockProps } from './types';

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

export function RuntimeDock({
  deployInfo,
  runtimeState,
  eventFeed,
  appErrors,
  results,
  connectionPreview,
  themeMode,
  isCollapsed,
  onToggleCollapsed,
}: RuntimeDockProps) {
  const logViewportRef = useRef<HTMLDivElement | null>(null);

  useEffect(() => {
    if (isCollapsed || !logViewportRef.current) {
      return;
    }

    logViewportRef.current.scrollTop = logViewportRef.current.scrollHeight;
  }, [eventFeed, isCollapsed]);

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
              <h3>部署快照</h3>
            </div>
          </div>

          <div className="runtime-dock__panel-body">
            <div className="metric-grid metric-grid--ops">
              <article>
                <span>已部署节点</span>
                <strong>{deployInfo?.nodeCount ?? '--'}</strong>
              </article>
              <article>
                <span>已部署边数</span>
                <strong>{deployInfo?.edgeCount ?? '--'}</strong>
              </article>
              <article>
                <span>最近事件</span>
                <strong>{runtimeState.lastEventType ?? '--'}</strong>
              </article>
              <article>
                <span>根节点</span>
                <strong>{deployInfo?.rootNodes.length ?? 0}</strong>
              </article>
            </div>

            <div className="rail-list">
              <h3>连接池</h3>
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

            <div className="runtime-errors">
              <div className="runtime-errors__header">
                <h3>异常捕获</h3>
                <span>{appErrors.length}</span>
              </div>
              {appErrors.length === 0 ? (
                <p>暂无异常</p>
              ) : (
                appErrors.slice(-4).reverse().map((error) => (
                  <article key={error.id} className="runtime-errors__item">
                    <div className="runtime-errors__meta">
                      <span>{formatLogTimestamp(error.timestamp)}</span>
                      <strong>{error.scope}</strong>
                    </div>
                    <strong>{error.title}</strong>
                    {error.detail ? <p>{error.detail}</p> : null}
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
          </div>

          <div className="runtime-dock__panel-body">
            <div ref={logViewportRef} className="runtime-log" role="log" aria-live="polite">
              {eventFeed.length === 0 ? (
                <p className="runtime-log__empty">暂无事件</p>
              ) : (
                eventFeed.map((entry) => (
                  <div key={entry.id} className={`runtime-log__line is-${entry.level}`}>
                    <span className="runtime-log__time">{formatLogTimestamp(entry.timestamp)}</span>
                    <span className="runtime-log__source">{entry.source}</span>
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
