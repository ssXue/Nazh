import { useEffect, useMemo, useRef, useState } from 'react';
import { JsonView, collapseAllNested, darkStyles, defaultStyles } from 'react-json-view-lite';
import 'react-json-view-lite/dist/index.css';

import { ConnectionsIcon, CopyIcon, LogsIcon, PayloadIcon } from './AppIcons';
import {
  buildEventFeedPlainText,
  buildRuntimeConsoleEntries,
  formatLogTimestamp,
} from './runtime-console';
import { RuntimeVariablesPanel } from './RuntimeVariablesPanel';
import type { RuntimeDockProps } from './types';

function normalizeResultPayload(payload: unknown): Record<string, unknown> | unknown[] {
  if (Array.isArray(payload)) {
    return payload;
  }

  if (payload && typeof payload === 'object') {
    return payload as Record<string, unknown>;
  }

  return { value: payload };
}

type RuntimeDockPanel = 'events' | 'results' | 'connections' | 'variables';

const runtimeDockTabs: Array<{ id: RuntimeDockPanel; label: string; title: string }> = [
  { id: 'events', label: '事件', title: '执行事件流' },
  { id: 'results', label: '结果', title: '结果载荷' },
  { id: 'connections', label: '连接', title: '连接资源' },
  { id: 'variables', label: '变量', title: '运行时变量' },
];

function RuntimeDockTabIcon({ panel }: { panel: RuntimeDockPanel }) {
  if (panel === 'results') {
    return <PayloadIcon width={14} height={14} />;
  }

  if (panel === 'connections') {
    return <ConnectionsIcon width={14} height={14} />;
  }

  if (panel === 'variables') {
    // 暂复用 PayloadIcon；后续可替换为专属变量图标
    return <PayloadIcon width={14} height={14} />;
  }

  return <LogsIcon width={14} height={14} />;
}

export function RuntimeDock({
  eventFeed,
  appErrors,
  results,
  connectionPreview,
  themeMode,
  isCollapsed,
  onToggleCollapsed,
  activeWorkflowId,
}: RuntimeDockProps) {
  const logViewportRef = useRef<HTMLDivElement | null>(null);
  const [hasCopiedEventFeed, setHasCopiedEventFeed] = useState(false);
  const [activePanel, setActivePanel] = useState<RuntimeDockPanel>('events');
  const runtimeConsoleEntries = useMemo(
    () => buildRuntimeConsoleEntries(eventFeed, appErrors),
    [appErrors, eventFeed],
  );
  const eventFeedText = useMemo(
    () => buildEventFeedPlainText(runtimeConsoleEntries),
    [runtimeConsoleEntries],
  );

  useEffect(() => {
    if (isCollapsed || activePanel !== 'events' || !logViewportRef.current) {
      return;
    }

    logViewportRef.current.scrollTop = logViewportRef.current.scrollHeight;
  }, [activePanel, isCollapsed, runtimeConsoleEntries]);

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

  const handlePanelSelect = (panel: RuntimeDockPanel) => {
    setActivePanel(panel);

    if (isCollapsed) {
      onToggleCollapsed();
    }
  };

  return (
    <section
      className={`runtime-dock ${isCollapsed ? 'is-collapsed' : ''}`}
      aria-live="polite"
    >
      <div className="runtime-dock__tabs" role="tablist" aria-label="运行观测窗体">
        {runtimeDockTabs.map((tab) => {
          const count =
            tab.id === 'events'
              ? runtimeConsoleEntries.length
              : tab.id === 'results'
                ? results.length
                : tab.id === 'connections'
                  ? connectionPreview.length
                  : 0;
          const isActive = activePanel === tab.id;

          return (
            <button
              key={tab.id}
              id={`runtime-dock-tab-${tab.id}`}
              type="button"
              role="tab"
              className={`runtime-dock__tab ${isActive ? 'is-active' : ''}`}
              aria-selected={isActive}
              aria-controls={`runtime-dock-panel-${tab.id}`}
              title={tab.title}
              onClick={() => handlePanelSelect(tab.id)}
            >
              <RuntimeDockTabIcon panel={tab.id} />
              <span>{tab.label}</span>
              <em>{count}</em>
            </button>
          );
        })}
      </div>

      <div
        id="runtime-dock-grid"
        className="runtime-dock__grid"
      >
        <div className="runtime-dock__main">
          <section
            id="runtime-dock-panel-events"
            className={`runtime-dock__panel runtime-dock__panel--feed ${activePanel === 'events' ? 'is-active' : ''}`}
            role="tabpanel"
            aria-labelledby="runtime-dock-tab-events"
            hidden={activePanel !== 'events' || isCollapsed}
          >
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
              <div ref={logViewportRef} className="runtime-log" role="log" aria-live="polite" data-testid="event-feed">
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

          <section
            id="runtime-dock-panel-results"
            className={`runtime-dock__panel runtime-dock__panel--feed ${activePanel === 'results' ? 'is-active' : ''}`}
            role="tabpanel"
            aria-labelledby="runtime-dock-tab-results"
            hidden={activePanel !== 'results' || isCollapsed}
          >
            <div className="runtime-dock__panel-header">
              <div>
                <h3>结果载荷</h3>
              </div>
            </div>

            <div className="runtime-dock__panel-body">
              <div className="runtime-results" data-testid="result-list">
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

          <section
            id="runtime-dock-panel-connections"
            className={`runtime-dock__panel runtime-dock__panel--connections ${activePanel === 'connections' ? 'is-active' : ''}`}
            role="tabpanel"
            aria-labelledby="runtime-dock-tab-connections"
            hidden={activePanel !== 'connections' || isCollapsed}
          >
            <div className="runtime-dock__panel-header">
              <div>
                <h3>连接资源</h3>
              </div>
            </div>

            <div className="runtime-dock__panel-body">
              <div className="runtime-dock__connection-panel">
                {connectionPreview.length === 0 ? (
                  <p className="runtime-dock__empty">暂无连接占用</p>
                ) : (
                  <div className="runtime-dock__connections">
                    {connectionPreview.map((connection) => (
                      <span
                        key={connection.id}
                        className={`runtime-dock__conn-chip ${connection.in_use ? 'is-busy' : 'is-idle'}`}
                      >
                        <i className="runtime-dock__conn-dot" />
                        {connection.id}
                        <small>{connection.kind}</small>
                      </span>
                    ))}
                  </div>
                )}
              </div>
            </div>
          </section>

          <section
            id="runtime-dock-panel-variables"
            className={`runtime-dock__panel runtime-dock__panel--variables ${activePanel === 'variables' ? 'is-active' : ''}`}
            role="tabpanel"
            aria-labelledby="runtime-dock-tab-variables"
            hidden={activePanel !== 'variables' || isCollapsed}
          >
            <div className="runtime-dock__panel-header">
              <div>
                <h3>运行时变量</h3>
              </div>
            </div>

            <div className="runtime-dock__panel-body">
              <RuntimeVariablesPanel workflowId={activeWorkflowId} />
            </div>
          </section>
        </div>
      </div>
    </section>
  );
}
