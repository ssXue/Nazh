import { useState } from 'react';

import type { RuntimeDockProps } from '../types';

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

export function ConnectionTable({ connections }: ConnectionTableProps) {
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
