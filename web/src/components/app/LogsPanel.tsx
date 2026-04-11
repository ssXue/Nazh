import { useEffect, useMemo, useState } from 'react';
import { JsonView, collapseAllNested, darkStyles, defaultStyles } from 'react-json-view-lite';
import 'react-json-view-lite/dist/index.css';

import { CopyIcon } from './AppIcons';
import {
  buildEventFeedPlainText,
  buildRuntimeConsoleEntries,
  formatLogDate,
  formatLogTimestamp,
  type RuntimeConsoleEntry,
} from './runtime-console';
import type { LogsPanelProps } from './types';

type LogLevelFilter = 'all' | RuntimeConsoleEntry['level'];
type LogTypeFilter = 'all' | RuntimeConsoleEntry['channel'];

const LEVEL_FILTERS: Array<{ value: LogLevelFilter; label: string }> = [
  { value: 'all', label: '全部' },
  { value: 'error', label: '异常' },
  { value: 'warn', label: '警告' },
  { value: 'success', label: '成功' },
  { value: 'info', label: '信息' },
];

const TYPE_FILTERS: Array<{ value: LogTypeFilter; label: string }> = [
  { value: 'all', label: '全部' },
  { value: 'event', label: '事件' },
  { value: 'exception', label: '异常' },
];

function getLevelLabel(level: RuntimeConsoleEntry['level']): string {
  switch (level) {
    case 'info':
      return '信息';
    case 'success':
      return '成功';
    case 'warn':
      return '警告';
    case 'error':
      return '异常';
  }
}

function getChannelLabel(channel: RuntimeConsoleEntry['channel']): string {
  return channel === 'exception' ? '异常捕获' : '运行事件';
}

function normalizeSearchText(value: string): string {
  return value.trim().toLowerCase();
}

function buildEntrySearchText(entry: RuntimeConsoleEntry): string {
  return normalizeSearchText(
    [entry.source, entry.message, entry.detail ?? '', entry.tag ?? '', entry.scope ?? ''].join(' '),
  );
}

async function copyText(value: string): Promise<boolean> {
  try {
    if (navigator.clipboard?.writeText) {
      await navigator.clipboard.writeText(value);
      return true;
    }

    const textarea = document.createElement('textarea');
    textarea.value = value;
    textarea.setAttribute('readonly', 'true');
    textarea.style.position = 'absolute';
    textarea.style.left = '-9999px';
    document.body.appendChild(textarea);
    textarea.select();
    document.execCommand('copy');
    document.body.removeChild(textarea);
    return true;
  } catch {
    return false;
  }
}

export function LogsPanel({
  eventFeed,
  appErrors,
  resultCount,
  themeMode,
  activeBoardName,
  workflowStatusLabel,
}: LogsPanelProps) {
  const [levelFilter, setLevelFilter] = useState<LogLevelFilter>('all');
  const [typeFilter, setTypeFilter] = useState<LogTypeFilter>('all');
  const [sourceFilter, setSourceFilter] = useState<string>('all');
  const [searchTerm, setSearchTerm] = useState('');
  const [selectedEntryId, setSelectedEntryId] = useState<string | null>(null);
  const [hasCopiedVisibleLogs, setHasCopiedVisibleLogs] = useState(false);

  const consoleEntries = useMemo(
    () => buildRuntimeConsoleEntries(eventFeed, appErrors).slice().reverse(),
    [appErrors, eventFeed],
  );
  const normalizedSearchTerm = normalizeSearchText(searchTerm);

  const sourceBreakdown = useMemo(() => {
    const sourceCounts = new Map<string, number>();

    consoleEntries.forEach((entry) => {
      sourceCounts.set(entry.source, (sourceCounts.get(entry.source) ?? 0) + 1);
    });

    return Array.from(sourceCounts.entries())
      .sort((left, right) => {
        if (left[1] === right[1]) {
          return left[0].localeCompare(right[0]);
        }

        return right[1] - left[1];
      })
      .slice(0, 8);
  }, [consoleEntries]);

  const filteredEntries = useMemo(
    () =>
      consoleEntries.filter((entry) => {
        if (levelFilter !== 'all' && entry.level !== levelFilter) {
          return false;
        }

        if (typeFilter !== 'all' && entry.channel !== typeFilter) {
          return false;
        }

        if (sourceFilter !== 'all' && entry.source !== sourceFilter) {
          return false;
        }

        if (normalizedSearchTerm && !buildEntrySearchText(entry).includes(normalizedSearchTerm)) {
          return false;
        }

        return true;
      }),
    [consoleEntries, levelFilter, normalizedSearchTerm, sourceFilter, typeFilter],
  );

  useEffect(() => {
    if (filteredEntries.length === 0) {
      setSelectedEntryId(null);
      return;
    }

    setSelectedEntryId((current) =>
      current && filteredEntries.some((entry) => entry.id === current)
        ? current
        : filteredEntries[0].id,
    );
  }, [filteredEntries]);

  useEffect(() => {
    if (!hasCopiedVisibleLogs) {
      return undefined;
    }

    const timer = window.setTimeout(() => {
      setHasCopiedVisibleLogs(false);
    }, 1600);

    return () => window.clearTimeout(timer);
  }, [hasCopiedVisibleLogs]);

  const selectedEntry =
    filteredEntries.find((entry) => entry.id === selectedEntryId) ?? filteredEntries[0] ?? null;
  const visibleLogText = useMemo(
    () => buildEventFeedPlainText(filteredEntries.slice().reverse()),
    [filteredEntries],
  );

  const errorCount = consoleEntries.filter((entry) => entry.level === 'error').length;
  const latestEntry = consoleEntries[0] ?? null;
  const levelCounts = useMemo(() => {
    const counts: Record<RuntimeConsoleEntry['level'], number> = {
      info: 0,
      success: 0,
      warn: 0,
      error: 0,
    };

    consoleEntries.forEach((entry) => {
      counts[entry.level] += 1;
    });

    return counts;
  }, [consoleEntries]);

  const selectedEntryPayload = selectedEntry
    ? {
        id: selectedEntry.id,
        channel: getChannelLabel(selectedEntry.channel),
        scope: selectedEntry.scope ?? null,
        level: getLevelLabel(selectedEntry.level),
        source: selectedEntry.source,
        message: selectedEntry.message,
        detail: selectedEntry.detail ?? null,
        tag: selectedEntry.tag ?? null,
        timestamp: new Date(selectedEntry.timestamp).toISOString(),
        local_time: `${formatLogDate(selectedEntry.timestamp)} ${formatLogTimestamp(selectedEntry.timestamp)}`,
      }
    : null;

  const handleCopyVisibleLogs = async () => {
    if (!visibleLogText.trim()) {
      return;
    }

    const hasCopied = await copyText(visibleLogText);
    setHasCopiedVisibleLogs(hasCopied);
  };

  return (
    <>
      <div
        className="panel__header panel__header--desktop window-safe-header"
        data-window-drag-region
      >
        <div className="logs-panel__heading">
          <h2>结构化日志</h2>
          <span>{activeBoardName ?? '全局会话'}</span>
        </div>
        <span className="panel__badge">{workflowStatusLabel}</span>
      </div>

      <div className="logs-panel">
        <section className="logs-panel__summary" aria-label="日志概览">
          <article className="logs-panel__summary-item">
            <span>日志总数</span>
            <strong>{consoleEntries.length}</strong>
            <em>当前可追踪事件</em>
          </article>
          <article className="logs-panel__summary-item">
            <span>异常捕获</span>
            <strong>{errorCount}</strong>
            <em>{errorCount > 0 ? '需要关注' : '当前平稳'}</em>
          </article>
          <article className="logs-panel__summary-item">
            <span>活跃来源</span>
            <strong>{sourceBreakdown.length}</strong>
            <em>{resultCount} 条结果输出</em>
          </article>
          <article className="logs-panel__summary-item">
            <span>最新更新</span>
            <strong>{latestEntry ? formatLogTimestamp(latestEntry.timestamp) : '--:--:--'}</strong>
            <em>{latestEntry ? formatLogDate(latestEntry.timestamp) : '等待事件'}</em>
          </article>
        </section>

        <div className="logs-panel__workspace">
          <aside className="logs-panel__rail" aria-label="日志筛选">
            <div className="logs-panel__section-head">
              <div>
                <h3>筛选器</h3>
                <span>{filteredEntries.length} 条命中</span>
              </div>
            </div>

            <div className="logs-panel__rail-body">
              <label className="logs-panel__search-field">
                <span>检索</span>
                <input
                  type="search"
                  className="logs-panel__search"
                  placeholder="来源 / 内容 / 明细"
                  value={searchTerm}
                  onChange={(event) => setSearchTerm(event.target.value)}
                />
              </label>

              <section className="logs-panel__group" aria-label="按类型筛选">
                <div className="logs-panel__group-title">类型</div>
                <div className="logs-panel__chip-grid">
                  {TYPE_FILTERS.map((filter) => (
                    <button
                      key={filter.value}
                      type="button"
                      className={
                        typeFilter === filter.value
                          ? 'logs-panel__chip is-active'
                          : 'logs-panel__chip'
                      }
                      aria-pressed={typeFilter === filter.value}
                      onClick={() => setTypeFilter(filter.value)}
                    >
                      <span>{filter.label}</span>
                    </button>
                  ))}
                </div>
              </section>

              <section className="logs-panel__group" aria-label="按级别筛选">
                <div className="logs-panel__group-title">级别</div>
                <div className="logs-panel__chip-grid">
                  {LEVEL_FILTERS.map((filter) => {
                    const count =
                      filter.value === 'all' ? consoleEntries.length : levelCounts[filter.value];

                    return (
                      <button
                        key={filter.value}
                        type="button"
                        className={
                          levelFilter === filter.value
                            ? `logs-panel__chip logs-panel__chip--${filter.value} is-active`
                            : `logs-panel__chip logs-panel__chip--${filter.value}`
                        }
                        aria-pressed={levelFilter === filter.value}
                        onClick={() => setLevelFilter(filter.value)}
                      >
                        <span>{filter.label}</span>
                        <strong>{count}</strong>
                      </button>
                    );
                  })}
                </div>
              </section>

              <section className="logs-panel__group" aria-label="按来源筛选">
                <div className="logs-panel__group-title">来源</div>
                <div className="logs-panel__source-list">
                  <button
                    type="button"
                    className={sourceFilter === 'all' ? 'logs-panel__source is-active' : 'logs-panel__source'}
                    aria-pressed={sourceFilter === 'all'}
                    onClick={() => setSourceFilter('all')}
                  >
                    <span className="logs-panel__source-dot logs-panel__source-dot--all" />
                    <span className="logs-panel__source-name">全部来源</span>
                    <strong>{consoleEntries.length}</strong>
                  </button>

                  {sourceBreakdown.map(([source, count]) => (
                    <button
                      key={source}
                      type="button"
                      className={sourceFilter === source ? 'logs-panel__source is-active' : 'logs-panel__source'}
                      aria-pressed={sourceFilter === source}
                      onClick={() => setSourceFilter(source)}
                    >
                      <span className="logs-panel__source-dot" />
                      <span className="logs-panel__source-name">{source}</span>
                      <strong>{count}</strong>
                    </button>
                  ))}
                </div>
              </section>
            </div>
          </aside>

          <section className="logs-panel__stream" aria-label="日志流">
            <div className="logs-panel__section-head">
              <div>
                <h3>事件流</h3>
                <span>{selectedEntry ? `已选中 ${selectedEntry.source}` : '等待事件进入'}</span>
              </div>

              <button
                type="button"
                className={`ghost logs-panel__copy-button ${hasCopiedVisibleLogs ? 'is-active' : ''}`}
                onClick={() => void handleCopyVisibleLogs()}
                disabled={!visibleLogText.trim()}
                aria-label={hasCopiedVisibleLogs ? '已复制可见日志' : '复制可见日志'}
                title={hasCopiedVisibleLogs ? '已复制' : '复制可见日志'}
              >
                <CopyIcon width={14} height={14} />
              </button>
            </div>

            <div className="logs-panel__stream-toolbar">
              <span>按时间倒序显示</span>
              <span>{filteredEntries.length} 条可见记录</span>
            </div>

            <div className="logs-panel__stream-list" role="list">
              {filteredEntries.length === 0 ? (
                <div className="logs-panel__empty">
                  <strong>暂无匹配日志</strong>
                  <span>调整筛选条件后再查看。</span>
                </div>
              ) : (
                filteredEntries.map((entry) => (
                  <button
                    key={entry.id}
                    type="button"
                    className={
                      selectedEntry?.id === entry.id
                        ? `logs-panel__entry logs-panel__entry--${entry.level} is-active`
                        : `logs-panel__entry logs-panel__entry--${entry.level}`
                    }
                    role="listitem"
                    onClick={() => setSelectedEntryId(entry.id)}
                  >
                    <div className="logs-panel__entry-top">
                      <span className={`logs-panel__entry-level logs-panel__entry-level--${entry.level}`} />
                      <span className="logs-panel__entry-time">{formatLogTimestamp(entry.timestamp)}</span>
                      <span className="logs-panel__entry-source">{entry.source}</span>
                      <span className="logs-panel__entry-type">{getChannelLabel(entry.channel)}</span>
                    </div>
                    <strong className="logs-panel__entry-message">{entry.message}</strong>
                    {entry.detail ? (
                      <span className="logs-panel__entry-detail">{entry.detail}</span>
                    ) : null}
                  </button>
                ))
              )}
            </div>
          </section>

          <aside className="logs-panel__inspector" aria-label="日志详情">
            <div className="logs-panel__section-head">
              <div>
                <h3>详情</h3>
                <span>{selectedEntry ? getLevelLabel(selectedEntry.level) : '未选择日志'}</span>
              </div>
            </div>

            <div className="logs-panel__inspector-body">
              {selectedEntry && selectedEntryPayload ? (
                <>
                  <section
                    className={`logs-panel__detail-hero logs-panel__detail-hero--${selectedEntry.level}`}
                  >
                    <div className="logs-panel__detail-headline">
                      <span className="logs-panel__detail-tone">
                        {selectedEntry.tag ?? getChannelLabel(selectedEntry.channel)}
                      </span>
                      <strong>{selectedEntry.message}</strong>
                    </div>
                    <div className="logs-panel__detail-meta">
                      <span>{selectedEntry.source}</span>
                      <span>{`${formatLogDate(selectedEntry.timestamp)} ${formatLogTimestamp(selectedEntry.timestamp)}`}</span>
                    </div>
                  </section>

                  {selectedEntry.detail ? (
                    <section className="logs-panel__detail-block">
                      <div className="logs-panel__detail-block-title">明细</div>
                      <pre>{selectedEntry.detail}</pre>
                    </section>
                  ) : null}

                  <section className="logs-panel__detail-block logs-panel__detail-block--json">
                    <div className="logs-panel__detail-block-title">结构化视图</div>
                    <div className="logs-panel__json-view">
                      <JsonView
                        data={selectedEntryPayload}
                        shouldExpandNode={collapseAllNested}
                        clickToExpandNode
                        style={themeMode === 'dark' ? darkStyles : defaultStyles}
                      />
                    </div>
                  </section>
                </>
              ) : (
                <div className="logs-panel__empty logs-panel__empty--inspector">
                  <strong>选择一条日志</strong>
                  <span>右侧会显示结构化字段与明细内容。</span>
                </div>
              )}
            </div>
          </aside>
        </div>
      </div>
    </>
  );
}
