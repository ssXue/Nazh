import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { JsonView, collapseAllNested, darkStyles, defaultStyles } from 'react-json-view-lite';
import 'react-json-view-lite/dist/index.css';

import {
  ConnectionsIcon,
  CopyIcon,
  LogsIcon,
  PayloadIcon,
  PlusIcon,
  XCloseIcon,
} from './AppIcons';
import { JsonCodeEditor } from '@flowgram.ai/form-materials';
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

type RuntimeDockPanel = 'events' | 'results' | 'payload' | 'connections' | 'variables';

const runtimeDockTabs: Array<{ id: RuntimeDockPanel; label: string; title: string }> = [
  { id: 'events', label: '事件', title: '执行事件流' },
  { id: 'results', label: '结果', title: '结果载荷' },
  { id: 'payload', label: '载荷', title: '测试载荷' },
  { id: 'connections', label: '连接', title: '连接资源' },
  { id: 'variables', label: '变量', title: '运行时变量' },
];

interface DockColumn {
  id: string;
  panel: RuntimeDockPanel;
}

let nextColumnId = 1;
function createColumnId(): string {
  return `c${nextColumnId++}`;
}

function RuntimeDockTabIcon({ panel }: { panel: RuntimeDockPanel }) {
  if (panel === 'results' || panel === 'payload') {
    return <PayloadIcon width={14} height={14} />;
  }

  if (panel === 'connections') {
    return <ConnectionsIcon width={14} height={14} />;
  }

  if (panel === 'variables') {
    return <PayloadIcon width={14} height={14} />;
  }

  return <LogsIcon width={14} height={14} />;
}

function getPanelCount(panel: RuntimeDockPanel, counts: Record<RuntimeDockPanel, number>): number {
  return counts[panel];
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
  payloadText,
  deployInfo,
  onPayloadTextChange,
}: RuntimeDockProps) {
  const logViewportRef = useRef<HTMLDivElement | null>(null);
  const [hasCopiedEventFeed, setHasCopiedEventFeed] = useState(false);
  const [variableCount, setVariableCount] = useState(0);
  const [columns, setColumns] = useState<DockColumn[]>([
    { id: createColumnId(), panel: 'events' },
  ]);
  const [focusedColumnId, setFocusedColumnId] = useState<string>(columns[0].id);
  const [dragState, setDragState] = useState<{
    index: number;
    startX: number;
    startWidths: number[];
  } | null>(null);
  const containerRef = useRef<HTMLDivElement | null>(null);
  const columnWidthsRef = useRef<number[]>([]);

  const runtimeConsoleEntries = useMemo(
    () => buildRuntimeConsoleEntries(eventFeed, appErrors),
    [appErrors, eventFeed],
  );
  const eventFeedText = useMemo(
    () => buildEventFeedPlainText(runtimeConsoleEntries),
    [runtimeConsoleEntries],
  );

  const panelCounts = useMemo<Record<RuntimeDockPanel, number>>(
    () => ({
      events: runtimeConsoleEntries.length,
      results: results.length,
      payload: deployInfo ? 1 : 0,
      connections: connectionPreview.length,
      variables: variableCount,
    }),
    [runtimeConsoleEntries.length, results.length, deployInfo, connectionPreview.length, variableCount],
  );

  const isMultiColumn = columns.length > 1;

  const activePanels = useMemo(
    () => new Set(columns.map((col) => col.panel)),
    [columns],
  );

  useEffect(() => {
    if (isCollapsed) {
      return;
    }

    const eventsColumn = columns.find((col) => col.panel === 'events');
    if (!eventsColumn || focusedColumnId !== eventsColumn.id) {
      return;
    }

    if (!logViewportRef.current) {
      return;
    }

    logViewportRef.current.scrollTop = logViewportRef.current.scrollHeight;
  }, [columns, focusedColumnId, isCollapsed, runtimeConsoleEntries]);

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

  const handleTabClick = useCallback(
    (panel: RuntimeDockPanel) => {
      if (isMultiColumn) {
        // 多栏模式：切换焦点栏的面板类型
        setColumns((prev) =>
          prev.map((col) =>
            col.id === focusedColumnId ? { ...col, panel } : col,
          ),
        );
      } else {
        // 单栏模式：直接切换
        setColumns((prev) =>
          prev.map((col) =>
            col.id === focusedColumnId ? { ...col, panel } : col,
          ),
        );
      }

      if (isCollapsed) {
        onToggleCollapsed();
      }
    },
    [focusedColumnId, isCollapsed, isMultiColumn, onToggleCollapsed],
  );

  const handleSplitPanel = useCallback(
    (panel: RuntimeDockPanel) => {
      if (columns.length >= 4) {
        return;
      }

      const newColumn: DockColumn = { id: createColumnId(), panel };
      setColumns((prev) => [...prev, newColumn]);
      setFocusedColumnId(newColumn.id);

      if (isCollapsed) {
        onToggleCollapsed();
      }
    },
    [columns.length, isCollapsed, onToggleCollapsed],
  );

  const handleCloseColumn = useCallback(
    (columnId: string) => {
      if (columns.length <= 1) {
        return;
      }

      setColumns((prev) => {
        const next = prev.filter((col) => col.id !== columnId);
        if (focusedColumnId === columnId && next.length > 0) {
          setFocusedColumnId(next[next.length - 1].id);
        }
        return next;
      });
    },
    [columns.length, focusedColumnId],
  );

  // ── 拖拽分割线 ──

  const handleDragStart = useCallback(
    (index: number, event: React.PointerEvent) => {
      event.preventDefault();
      const container = containerRef.current;
      if (!container) {
        return;
      }

      const children = container.querySelectorAll<HTMLElement>('.runtime-dock__column');
      const widths = Array.from(children).map((child) => child.getBoundingClientRect().width);
      columnWidthsRef.current = widths;

      setDragState({
        index,
        startX: event.clientX,
        startWidths: widths,
      });

      (event.target as HTMLElement).setPointerCapture(event.pointerId);
    },
    [],
  );

  const handleDragMove = useCallback(
    (event: React.PointerEvent) => {
      if (!dragState) {
        return;
      }

      const container = containerRef.current;
      if (!container) {
        return;
      }

      const { index, startX, startWidths } = dragState;
      const delta = event.clientX - startX;
      const leftWidth = Math.max(120, startWidths[index] + delta);
      const rightWidth = Math.max(120, startWidths[index + 1] - delta);
      const children = container.querySelectorAll<HTMLElement>('.runtime-dock__column');

      if (children[index]) {
        (children[index] as HTMLElement).style.flex = `${leftWidth} 0 0`;
      }
      if (children[index + 1]) {
        (children[index + 1] as HTMLElement).style.flex = `${rightWidth} 0 0`;
      }
    },
    [dragState],
  );

  const handleDragEnd = useCallback(() => {
    setDragState(null);
  }, []);

  return (
    <section
      className={`runtime-dock ${isCollapsed ? 'is-collapsed' : ''}`}
      aria-live="polite"
    >
      <div className="runtime-dock__tabs" role="tablist" aria-label="运行观测窗体">
        {runtimeDockTabs.map((tab) => {
          const count = getPanelCount(tab.id, panelCounts);
          const isActive = activePanels.has(tab.id);

          return (
            <div key={tab.id} className="runtime-dock__tab-group">
              <button
                id={`runtime-dock-tab-${tab.id}`}
                type="button"
                role="tab"
                className={`runtime-dock__tab ${isActive ? 'is-active' : ''}`}
                aria-selected={isActive}
                aria-controls="runtime-dock-grid"
                title={tab.title}
                onClick={() => handleTabClick(tab.id)}
              >
                <RuntimeDockTabIcon panel={tab.id} />
                <span>{tab.label}</span>
                {count > 0 ? <em>{count}</em> : null}
              </button>
              {(!isActive || isMultiColumn) && columns.length < 4 ? (
                <button
                  type="button"
                  className="runtime-dock__tab-split"
                  onClick={() => handleSplitPanel(tab.id)}
                  title={`分栏显示${tab.label}`}
                  aria-label={`分栏显示${tab.label}`}
                >
                  <PlusIcon width={12} height={12} />
                </button>
              ) : null}
            </div>
          );
        })}
      </div>

      <div
        id="runtime-dock-grid"
        className="runtime-dock__grid"
      >
        <div
          ref={containerRef}
          className={`runtime-dock__columns ${isMultiColumn ? 'is-multi' : 'is-single'}`}
          onPointerMove={dragState ? handleDragMove : undefined}
          onPointerUp={dragState ? handleDragEnd : undefined}
          onPointerCancel={dragState ? handleDragEnd : undefined}
        >
          {columns.map((column, index) => (
            <div
              key={column.id}
              className={`runtime-dock__column ${focusedColumnId === column.id ? 'is-focused' : ''}`}
              onClick={() => setFocusedColumnId(column.id)}
            >
              {isMultiColumn && (
                <div className="runtime-dock__column-header">
                  <RuntimeDockTabIcon panel={column.panel} />
                  <span>{runtimeDockTabs.find((t) => t.id === column.panel)?.label}</span>
                  <button
                    type="button"
                    className="runtime-dock__column-close"
                    onClick={(e) => {
                      e.stopPropagation();
                      handleCloseColumn(column.id);
                    }}
                    title="关闭栏"
                    aria-label="关闭此栏"
                  >
                    <XCloseIcon width={12} height={12} />
                  </button>
                </div>
              )}
              <div className="runtime-dock__column-body">
                <DockPanelContent
                  panel={column.panel}
                  logViewportRef={column.panel === 'events' ? logViewportRef : undefined}
                  runtimeConsoleEntries={runtimeConsoleEntries}
                  eventFeedText={eventFeedText}
                  hasCopiedEventFeed={hasCopiedEventFeed}
                  onCopyEventFeed={() => void handleCopyEventFeed()}
                  results={results}
                  connectionPreview={connectionPreview}
                  themeMode={themeMode}
                  activeWorkflowId={activeWorkflowId}
                  onVariableCountChange={setVariableCount}
                  isCollapsed={isCollapsed}
                  payloadText={payloadText}
                  deployInfo={deployInfo}
                  onPayloadTextChange={onPayloadTextChange}
                />
              </div>

              {index < columns.length - 1 && (
                <div
                  className="runtime-dock__divider"
                  onPointerDown={(e) => handleDragStart(index, e)}
                />
              )}
            </div>
          ))}
        </div>
      </div>
    </section>
  );
}

// ── 面板内容渲染 ──

interface DockPanelContentProps {
  panel: RuntimeDockPanel;
  logViewportRef: React.RefObject<HTMLDivElement | null> | undefined;
  runtimeConsoleEntries: ReturnType<typeof buildRuntimeConsoleEntries>;
  eventFeedText: string;
  hasCopiedEventFeed: boolean;
  onCopyEventFeed: () => void;
  results: RuntimeDockProps['results'];
  connectionPreview: RuntimeDockProps['connectionPreview'];
  themeMode: RuntimeDockProps['themeMode'];
  activeWorkflowId: RuntimeDockProps['activeWorkflowId'];
  onVariableCountChange: (count: number) => void;
  isCollapsed: boolean;
  payloadText: string;
  deployInfo: RuntimeDockProps['deployInfo'];
  onPayloadTextChange: RuntimeDockProps['onPayloadTextChange'];
}

function DockPanelContent({
  panel,
  logViewportRef,
  runtimeConsoleEntries,
  eventFeedText,
  hasCopiedEventFeed,
  onCopyEventFeed,
  results,
  connectionPreview,
  themeMode,
  activeWorkflowId,
  onVariableCountChange,
  isCollapsed,
  payloadText,
  deployInfo,
  onPayloadTextChange,
}: DockPanelContentProps) {
  if (panel === 'events') {
    return (
      <section className="runtime-dock__panel is-active" role="tabpanel">
        <div className="runtime-dock__panel-header">
          <h3>执行事件流</h3>
          <div className="runtime-dock__panel-actions">
            <button
              type="button"
              className={`runtime-dock__panel-tool ${hasCopiedEventFeed ? 'is-active' : ''}`}
              onClick={onCopyEventFeed}
              disabled={!eventFeedText.trim()}
              aria-label={hasCopiedEventFeed ? '执行事件流已复制' : '复制执行事件流'}
              title={hasCopiedEventFeed ? '已复制' : '复制执行事件流'}
            >
              <CopyIcon width={14} height={14} />
            </button>
          </div>
        </div>
        <div className="runtime-dock__panel-body">
          <div
            ref={(el) => { if (logViewportRef) { (logViewportRef as React.MutableRefObject<HTMLDivElement | null>).current = el; } }}
            className="runtime-log"
            role="log"
            aria-live="polite"
            data-testid="event-feed"
          >
            {runtimeConsoleEntries.length === 0 ? (
              <p className="runtime-log__empty">暂无事件与异常</p>
            ) : (
              runtimeConsoleEntries.map((entry) => (
                <div key={entry.id} className={`runtime-log__line is-${entry.level}`}>
                  <span className="runtime-log__time">
                    {formatLogTimestamp(entry.timestamp)}
                  </span>
                  <span className="runtime-log__source">
                    {entry.source}
                    {entry.tag ? (
                      <em className="runtime-log__badge">{entry.tag}</em>
                    ) : null}
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
    );
  }

  if (panel === 'results') {
    return (
      <section className="runtime-dock__panel is-active" role="tabpanel">
        <div className="runtime-dock__panel-header">
          <h3>结果载荷</h3>
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
                        <strong>
                          {formatLogTimestamp(new Date(result.timestamp).getTime())}
                        </strong>
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
    );
  }

  if (panel === 'payload') {
    return <PayloadEditorPanel payloadText={payloadText} deployInfo={deployInfo} onPayloadTextChange={onPayloadTextChange} themeMode={themeMode} />;
  }

  if (panel === 'connections') {
    return <ConnectionTable connections={connectionPreview} />;
  }

  // variables
  return (
    <section className="runtime-dock__panel is-active" role="tabpanel">
      <div className="runtime-dock__panel-header">
        <h3>运行时变量</h3>
      </div>
      <div className="runtime-dock__panel-body">
        <RuntimeVariablesPanel
          workflowId={activeWorkflowId}
          onVariableCountChange={onVariableCountChange}
        />
      </div>
    </section>
  );
}

// ── 载荷编辑面板 ──

function PayloadEditorPanel({
  payloadText,
  deployInfo,
  onPayloadTextChange,
  themeMode,
}: {
  payloadText: string;
  deployInfo: RuntimeDockProps['deployInfo'];
  onPayloadTextChange: RuntimeDockProps['onPayloadTextChange'];
  themeMode: RuntimeDockProps['themeMode'];
}) {
  return (
    <section className="runtime-dock__panel is-active" role="tabpanel">
      <div className="runtime-dock__panel-header">
        <h3>测试载荷</h3>
        <span className="runtime-dock__payload-badge">{deployInfo ? '已可发送' : '等待部署'}</span>
      </div>
      <div className="runtime-dock__panel-body runtime-dock__panel-body--payload">
        <div className="runtime-dock__payload-editor">
          <JsonCodeEditor value={payloadText} onChange={onPayloadTextChange} theme={themeMode} />
        </div>
      </div>
    </section>
  );
}

// ── 连接面板（高密度表格）─────────────────────

type HealthPhase = 'idle' | 'connecting' | 'healthy' | 'degraded' | 'invalid' | 'reconnecting' | 'rateLimited' | 'circuitOpen' | 'timeout' | 'disconnected';

type HealthGroup = 'ok' | 'warn' | 'error' | 'off';

const healthGroupMap: Record<HealthPhase, HealthGroup> = {
  idle: 'ok',
  connecting: 'ok',
  healthy: 'ok',
  degraded: 'warn',
  rateLimited: 'warn',
  invalid: 'error',
  reconnecting: 'error',
  circuitOpen: 'error',
  timeout: 'error',
  disconnected: 'off',
};

const healthGroupOrder: HealthGroup[] = ['error', 'warn', 'off', 'ok'];

const healthPhaseLabels: Record<HealthPhase, string> = {
  idle: '空闲',
  connecting: '连接中',
  healthy: '健康',
  degraded: '降级',
  invalid: '无效',
  reconnecting: '重连中',
  rateLimited: '限流',
  circuitOpen: '熔断',
  timeout: '超时',
  disconnected: '断开',
};

function formatRelativeTime(value: string | undefined | null): string {
  if (!value) return '-';
  const ts = Date.parse(value);
  if (Number.isNaN(ts)) return '-';
  const deltaMs = Date.now() - ts;
  if (deltaMs < 60_000) return '刚刚';
  const minutes = Math.floor(deltaMs / 60_000);
  if (minutes < 60) return `${minutes}m`;
  const hours = Math.floor(minutes / 60);
  if (hours < 24) return `${hours}h`;
  return `${Math.floor(hours / 24)}d`;
}

function formatLatency(ms: number | null | undefined): string {
  if (ms == null) return '-';
  if (ms < 1) return '<1ms';
  if (ms < 1000) return `${Math.round(ms)}ms`;
  return `${(ms / 1000).toFixed(1)}s`;
}

interface ConnectionTableProps {
  connections: RuntimeDockProps['connectionPreview'];
}

function ConnectionTable({ connections }: ConnectionTableProps) {
  const [expandedId, setExpandedId] = useState<string | null>(null);

  if (connections.length === 0) {
    return (
      <section className="runtime-dock__panel is-active" role="tabpanel">
        <div className="runtime-dock__panel-header">
          <h3>连接资源</h3>
        </div>
        <div className="runtime-dock__panel-body">
          <p className="runtime-dock__empty">暂无连接占用</p>
        </div>
      </section>
    );
  }

  const sorted = [...connections].sort((a, b) => {
    const aGroup = healthGroupMap[(a.health?.phase ?? 'disconnected') as HealthPhase];
    const bGroup = healthGroupMap[(b.health?.phase ?? 'disconnected') as HealthPhase];
    const groupCmp = healthGroupOrder.indexOf(aGroup) - healthGroupOrder.indexOf(bGroup);
    if (groupCmp !== 0) return groupCmp;
    if (a.in_use !== b.in_use) return a.in_use ? -1 : 1;
    return a.id.localeCompare(b.id);
  });

  const counts = {
    total: connections.length,
    active: connections.filter((c) => c.in_use).length,
    warn: connections.filter((c) => healthGroupMap[(c.health?.phase ?? 'disconnected') as HealthPhase] === 'warn').length,
    error: connections.filter((c) => healthGroupMap[(c.health?.phase ?? 'disconnected') as HealthPhase] === 'error').length,
    off: connections.filter((c) => healthGroupMap[(c.health?.phase ?? 'disconnected') as HealthPhase] === 'off').length,
  };

  return (
    <section className="runtime-dock__panel is-active" role="tabpanel">
      <div className="runtime-dock__panel-header">
        <h3>连接资源</h3>
        <div className="runtime-dock__conn-summary">
          <span>{counts.total} 连接</span>
          {counts.active > 0 && <em className="conn-summary-active">{counts.active} 活跃</em>}
          {counts.warn > 0 && <em className="conn-summary-degraded">{counts.warn} 降级</em>}
          {counts.error > 0 && <em className="conn-summary-unhealthy">{counts.error} 异常</em>}
          {counts.off > 0 && <em className="conn-summary-disconnected">{counts.off} 断开</em>}
        </div>
      </div>
      <div className="runtime-dock__panel-body">
        <div className="conn-table" role="table">
          <div className="conn-table__head" role="row">
            <span role="columnheader">状态</span>
            <span role="columnheader">连接</span>
            <span role="columnheader">类型</span>
            <span role="columnheader">延迟</span>
            <span role="columnheader">失败</span>
            <span role="columnheader">最近活动</span>
          </div>
          {sorted.map((conn) => {
            const health = conn.health;
            const phase = health?.phase ?? 'disconnected';
            const isExpanded = expandedId === conn.id;

            return (
              <div key={conn.id} role="rowgroup">
                <button
                  type="button"
                  className={`conn-table__row ${isExpanded ? 'is-expanded' : ''} ${conn.in_use ? 'is-active' : ''}`}
                  role="row"
                  onClick={() => setExpandedId(isExpanded ? null : conn.id)}
                >
                  <span
                    className={`conn-table__phase conn-table__phase--${healthGroupMap[phase as HealthPhase]}`}
                    title={healthPhaseLabels[phase as HealthPhase] ?? phase}
                  >
                    <i />
                  </span>
                  <span className="conn-table__id" title={conn.id}>{conn.id}</span>
                  <span className="conn-table__kind">{conn.kind}</span>
                  <span className="conn-table__latency">{formatLatency(health?.lastLatencyMs)}</span>
                  <span className="conn-table__failures">
                    {(health?.consecutiveFailures ?? 0) > 0 ? (
                      <em>{health?.consecutiveFailures ?? 0}</em>
                    ) : (
                      <span className="conn-table__zero">0</span>
                    )}
                  </span>
                  <span className="conn-table__time">
                    {formatRelativeTime(conn.in_use ? conn.last_borrowed_at : health?.lastReleasedAt)}
                  </span>
                </button>
                {isExpanded && health && (
                  <div className="conn-table__detail">
                    <div className="conn-table__detail-grid">
                      <div className="conn-table__detail-field">
                        <label>状态</label>
                        <span className={`conn-table__phase-label conn-table__phase-label--${healthGroupMap[phase as HealthPhase]}`}>
                          {healthPhaseLabels[phase as HealthPhase] ?? phase}
                        </span>
                      </div>
                      {health.diagnosis && (
                        <div className="conn-table__detail-field">
                          <label>诊断</label>
                          <span>{health.diagnosis}</span>
                        </div>
                      )}
                      {health.lastLatencyMs != null && (
                        <div className="conn-table__detail-field">
                          <label>延迟</label>
                          <span>{formatLatency(health.lastLatencyMs)}</span>
                        </div>
                      )}
                      <div className="conn-table__detail-field">
                        <label>连续失败</label>
                        <span>{health.consecutiveFailures}</span>
                      </div>
                      <div className="conn-table__detail-field">
                        <label>累计失败</label>
                        <span>{health.totalFailures}</span>
                      </div>
                      <div className="conn-table__detail-field">
                        <label>超时次数</label>
                        <span>{health.timeoutCount}</span>
                      </div>
                      <div className="conn-table__detail-field">
                        <label>重连次数</label>
                        <span>{health.reconnectAttempts}</span>
                      </div>
                      {health.lastFailureReason && (
                        <div className="conn-table__detail-field">
                          <label>最近错误</label>
                          <span>{health.lastFailureReason}</span>
                        </div>
                      )}
                      {health.lastConnectedAt && (
                        <div className="conn-table__detail-field">
                          <label>上次连接</label>
                          <span>{formatRelativeTime(health.lastConnectedAt)}</span>
                        </div>
                      )}
                      {health.circuitOpenUntil && (
                        <div className="conn-table__detail-field">
                          <label>熔断至</label>
                          <span>{formatRelativeTime(health.circuitOpenUntil)}</span>
                        </div>
                      )}
                    </div>
                  </div>
                )}
              </div>
            );
          })}
        </div>
      </div>
    </section>
  );
}
