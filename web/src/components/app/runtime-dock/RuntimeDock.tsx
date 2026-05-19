import { useCallback, useEffect, useMemo, useRef, useState } from 'react';

import {
  ConnectionsIcon,
  LogsIcon,
  PayloadIcon,
  PlusIcon,
  XCloseIcon,
} from '../AppIcons';
import {
  buildEventFeedPlainText,
  buildRuntimeConsoleEntries,
} from '../runtime-console';
import type { RuntimeDockProps } from '../types';
import {
  type DockColumn,
  type RuntimeDockPanel,
  runtimeDockTabs,
  createColumnId,
  getPanelCount,
} from './types';
import { DockPanelContent } from './DockPanelContent';

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
      data-testid="runtime-dock"
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
                data-testid={`runtime-dock-tab-${tab.id}`}
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
