import { useEffect, useMemo, useState } from 'react';

import type { ConnectionRecord, RuntimeLogEntry, WorkflowWindowStatus } from '../../types';

interface DashboardPanelProps {
  userId: string;
  activeBoardName: string | null;
  boardCount: number;
  graphNodeCount: number;
  graphEdgeCount: number;
  graphConnectionCount: number;
  activeNodeCount: number;
  completedNodeCount: number;
  failedNodeCount: number;
  outputNodeCount: number;
  eventCount: number;
  resultCount: number;
  statusMessage: string;
  workflowStatus: WorkflowWindowStatus;
  traceId: string | null;
  lastEventType: string | null;
  lastNodeId: string | null;
  lastUpdatedAt: number | null;
  connections: ConnectionRecord[];
  eventFeed: RuntimeLogEntry[];
  onNavigateToBoards: () => void;
}

function getStatusLabel(status: WorkflowWindowStatus): string {
  switch (status) {
    case 'running':
      return '运行中';
    case 'failed':
      return '执行失败';
    case 'completed':
      return '执行完成';
    case 'deployed':
      return '已部署';
    case 'idle':
      return '待机';
    case 'preview':
      return '预览';
    default:
      return '未知';
  }
}

function getStatusTone(status: WorkflowWindowStatus): string {
  switch (status) {
    case 'running':
      return 'active';
    case 'failed':
      return 'danger';
    case 'completed':
      return 'success';
    case 'deployed':
      return 'ready';
    default:
      return 'muted';
  }
}

function formatTimeAgo(timestamp: number | null): string {
  if (!timestamp) return '--';
  const diff = Date.now() - timestamp;
  if (diff < 1000) return '刚刚';
  if (diff < 60000) return `${Math.floor(diff / 1000)} 秒前`;
  if (diff < 3600000) return `${Math.floor(diff / 60000)} 分钟前`;
  return `${Math.floor(diff / 3600000)} 小时前`;
}

function formatEventTime(timestamp: number): string {
  const d = new Date(timestamp);
  return `${d.getHours().toString().padStart(2, '0')}:${d.getMinutes().toString().padStart(2, '0')}:${d.getSeconds().toString().padStart(2, '0')}`;
}

function getEventLevelClass(level: RuntimeLogEntry['level']): string {
  switch (level) {
    case 'success':
      return 'is-success';
    case 'warn':
      return 'is-warning';
    case 'error':
      return 'is-danger';
    default:
      return 'is-info';
  }
}

function getConnectionHealthSummary(connections: ConnectionRecord[]) {
  let healthy = 0;
  let degraded = 0;
  let failed = 0;
  for (const c of connections) {
    const phase = c.health?.phase ?? 'unknown';
    if (phase === 'healthy') healthy++;
    else if (phase === 'degraded') degraded++;
    else if (phase === 'timeout' || phase === 'disconnected' || phase === 'invalid' || phase === 'circuitOpen') failed++;
    else healthy++;
  }
  return { healthy, degraded, failed, total: connections.length };
}

export function DashboardPanel({
  userId,
  activeBoardName,
  activeNodeCount,
  completedNodeCount,
  failedNodeCount,
  outputNodeCount,
  eventCount,
  resultCount,
  workflowStatus,
  traceId,
  lastEventType,
  lastUpdatedAt,
  connections,
  eventFeed,
  onNavigateToBoards,
}: DashboardPanelProps) {
  const [, setTick] = useState(0);

  useEffect(() => {
    const id = setInterval(() => setTick((t) => t + 1), 1000);
    return () => clearInterval(id);
  }, []);

  const totalProcessed = completedNodeCount + failedNodeCount + outputNodeCount;
  const successRate = totalProcessed > 0 ? Math.round((completedNodeCount / totalProcessed) * 100) : null;
  const healthScore = successRate ?? (workflowStatus === 'running' || workflowStatus === 'completed' ? 100 : 0);
  const hasFailure = failedNodeCount > 0;

  const connectionSummary = useMemo(() => getConnectionHealthSummary(connections), [connections]);

  const recentEvents = useMemo(() => {
    return eventFeed.slice(-5).reverse();
  }, [eventFeed]);

  const recentAlerts = useMemo(() => {
    return eventFeed
      .filter((e) => e.level === 'warn' || e.level === 'error')
      .slice(-5)
      .reverse();
  }, [eventFeed]);

  const statusLabel = getStatusLabel(workflowStatus);
  const statusTone = getStatusTone(workflowStatus);

  // 波形条高度由 eventCount + resultCount 映射
  const waveformBase = Math.min(eventCount + resultCount, 50);
  const waveformBars = useMemo(() => {
    return Array.from({ length: 24 }, (_, i) => {
      const seed = ((i * 37 + waveformBase * 13) % 17) / 17;
      const height = 20 + Math.round(seed * 80);
      return { height, delay: i * 0.08 };
    });
  }, [waveformBase]);

  return (
    <div className="dashboard-panel">
      <div
        className="panel__header panel__header--desktop window-safe-header"
        data-window-drag-region
      >
        <div className="panel__header__heading">
          <h2>总览</h2>
          <span>
            {activeBoardName ?? 'Nazh 控制台'} · {userId}
          </span>
        </div>
      </div>

      {/* Hero */}
      <section className="dashboard-hero">
        <div className="dashboard-hero__context">
          <strong className="dashboard-hero__title">
            {activeBoardName ? activeBoardName : 'Nazh 控制台'}
          </strong>
          <div className="dashboard-hero__status-row">
            <span className={`dashboard-hero__status-badge is-${statusTone}`}>{statusLabel}</span>
            {traceId && <span className="dashboard-hero__trace">Trace: {traceId.slice(0, 8)}…</span>}
          </div>
        </div>

        <div className="dashboard-hero__health">
          <div
            className="dashboard-hero__health-ring"
            style={{ '--dashboard-gauge-progress': `${healthScore}%` } as React.CSSProperties}
          >
            <div className="dashboard-hero__health-center">
              <strong>{healthScore}%</strong>
              <span>{hasFailure ? '异常' : healthScore > 0 ? '健康' : '待机'}</span>
            </div>
          </div>
          <span className="dashboard-hero__health-label">系统健康度</span>
        </div>

        <div className="dashboard-hero__meta">
          {lastEventType && (
            <div className="dashboard-hero__meta-item">
              <span>最近事件</span>
              <strong>{lastEventType}</strong>
            </div>
          )}
          <div className="dashboard-hero__meta-item">
            <span>最后更新</span>
            <strong>{formatTimeAgo(lastUpdatedAt)}</strong>
          </div>
          <div className="dashboard-hero__meta-item">
            <span>吞吐量</span>
            <strong>{eventCount + resultCount}</strong>
          </div>
        </div>

        <div className="dashboard-hero__slogan" aria-hidden="true">
          <div className="dashboard-hero__slogan-track">
            {Array.from({ length: 20 }, (_, i) => (
              <span key={i}>Make Automation Great Again&nbsp;&nbsp;·&nbsp;&nbsp;</span>
            ))}
          </div>
        </div>
      </section>

      {/* KPI Row */}
      <div className="dashboard-kpi-grid">
        <div className={`dashboard-kpi-card${activeNodeCount > 0 ? ' is-pulsing' : ''}`}>
          <span className="dashboard-kpi-card__value is-active">{activeNodeCount}</span>
          <span className="dashboard-kpi-card__label">运行中</span>
        </div>
        <div className="dashboard-kpi-card">
          <span className="dashboard-kpi-card__value is-success">{completedNodeCount}</span>
          <span className="dashboard-kpi-card__label">已完成</span>
        </div>
        <div className={`dashboard-kpi-card${failedNodeCount > 0 ? ' has-alert' : ''}`}>
          <span className="dashboard-kpi-card__value is-danger">{failedNodeCount}</span>
          <span className="dashboard-kpi-card__label">异常</span>
        </div>
        <div className="dashboard-kpi-card">
          <span className="dashboard-kpi-card__value">{eventCount}</span>
          <span className="dashboard-kpi-card__label">事件</span>
        </div>
        <div className="dashboard-kpi-card">
          <span className="dashboard-kpi-card__value">{resultCount}</span>
          <span className="dashboard-kpi-card__label">输出</span>
        </div>
        <div className="dashboard-kpi-card">
          <span className="dashboard-kpi-card__value">{connectionSummary.total}</span>
          <span className="dashboard-kpi-card__label">连接</span>
          {connectionSummary.failed > 0 && (
            <span className="dashboard-kpi-card__badge is-danger">{connectionSummary.failed} 故障</span>
          )}
        </div>
      </div>

      {/* Middle row: Alerts + Event Feed */}
      <div className="dashboard-mid-grid">
        <article className="dashboard-chart-card">
          <div className="dashboard-chart-card__header">
            <strong>异常告警</strong>
            <span>{recentAlerts.length} 条待关注</span>
          </div>
          <div className="dashboard-alert-feed">
            {recentAlerts.length === 0 && (
              <div className="dashboard-alert-feed__empty">
                <span className="dashboard-alert-feed__empty-dot is-success" />
                系统正常，暂无异常
              </div>
            )}
            {recentAlerts.map((event) => (
              <div key={event.id} className="dashboard-alert-feed__item">
                <span className={`dashboard-alert-feed__stripe ${getEventLevelClass(event.level)}`} />
                <div className="dashboard-alert-feed__body">
                  <div className="dashboard-alert-feed__head">
                    <span className="dashboard-alert-feed__time">{formatEventTime(event.timestamp)}</span>
                    <span className={`dashboard-alert-feed__level ${getEventLevelClass(event.level)}`}>
                      {event.level === 'error' ? '错误' : '警告'}
                    </span>
                  </div>
                  <span className="dashboard-alert-feed__source">{event.source}</span>
                  <span className="dashboard-alert-feed__message">{event.message}</span>
                </div>
              </div>
            ))}
          </div>
        </article>

        <article className="dashboard-chart-card">
          <div className="dashboard-chart-card__header">
            <strong>事件流</strong>
            <span>{eventFeed.length} 条记录</span>
          </div>
          <div className="dashboard-event-feed">
            {recentEvents.length === 0 && (
              <div className="dashboard-event-feed__empty">暂无事件</div>
            )}
            {recentEvents.map((event) => (
              <div key={event.id} className="dashboard-event-feed__item">
                <span className={`dashboard-event-feed__dot ${getEventLevelClass(event.level)}`} />
                <span className="dashboard-event-feed__time">{formatEventTime(event.timestamp)}</span>
                <span className="dashboard-event-feed__source">{event.source}</span>
                <span className="dashboard-event-feed__message">{event.message}</span>
              </div>
            ))}
          </div>
        </article>
      </div>

      {/* Waveform */}
      <article className="dashboard-waveform-card">
        <div className="dashboard-waveform-card__header">
          <strong>实时流量</strong>
          <span>事件 {eventCount} · 输出 {resultCount}</span>
        </div>
        <div className="dashboard-waveform" aria-hidden="true">
          {waveformBars.map((bar, i) => (
            <span
              key={i}
              className="dashboard-waveform__bar"
              style={
                {
                  height: `${bar.height}%`,
                  animationDelay: `${bar.delay}s`,
                } as React.CSSProperties
              }
            />
          ))}
        </div>
      </article>
    </div>
  );
}
