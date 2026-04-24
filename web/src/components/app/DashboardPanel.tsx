import type { CSSProperties } from 'react';

import type { DeployResponse } from '../../types';

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
  deployInfo: DeployResponse | null;
  onNavigateToBoards: () => void;
}

export function DashboardPanel({
  userId,
  activeBoardName,
  boardCount,
  graphNodeCount,
  graphEdgeCount,
  graphConnectionCount,
  activeNodeCount,
  completedNodeCount,
  failedNodeCount,
  outputNodeCount,
  eventCount,
  resultCount,
  statusMessage,
  deployInfo,
  onNavigateToBoards,
}: DashboardPanelProps) {
  const structureMetrics = [
    { label: '看板', value: boardCount },
    { label: '节点', value: graphNodeCount },
    { label: '边', value: graphEdgeCount },
    { label: '绑定', value: graphConnectionCount },
  ];
  const structurePeak = Math.max(...structureMetrics.map((metric) => metric.value), 1);

  const runtimeMetrics = [
    { label: '运行中', value: activeNodeCount, tone: 'active' },
    { label: '完成', value: completedNodeCount, tone: 'success' },
    { label: '失败', value: failedNodeCount, tone: 'danger' },
    { label: '输出', value: outputNodeCount, tone: 'warning' },
  ] as const;
  const runtimeHealthyCount = activeNodeCount + completedNodeCount + outputNodeCount;

  const pulseSeries = [
    boardCount * 4 + 8,
    graphConnectionCount * 7 + 12,
    graphNodeCount * 5 + 14,
    graphEdgeCount * 4 + 10,
    activeNodeCount * 12 + eventCount * 3 + 8,
    completedNodeCount * 8 + resultCount * 5 + 10,
    failedNodeCount * 16 + 6,
  ];
  const pulsePeak = Math.max(...pulseSeries, 1);

  const readinessScore = Math.min(
    100,
    (activeBoardName ? 24 : 0) +
      (graphNodeCount > 0 ? 22 : 0) +
      (graphConnectionCount > 0 ? 18 : 0) +
      (deployInfo ? 24 : 0) +
      (eventCount > 0 || resultCount > 0 ? 12 : 0),
  );
  const gaugeStyle = {
    '--dashboard-gauge-progress': `${readinessScore}%`,
  } as CSSProperties;

  return (
    <div className="dashboard-panel">
      <div className="dashboard-panel__header window-safe-header" data-window-drag-region>
        <h2>Dashboard</h2>
      </div>

      <section className="dashboard-hero">
        <div className="dashboard-hero__copy">
          <span className="dashboard-hero__eyebrow">USER ID · {userId}</span>
          <strong>欢迎回来，{userId}</strong>
          <p>{activeBoardName ? `当前聚焦 ${activeBoardName}` : '当前聚焦全局态势'}</p>
        </div>
      </section>

      <div className="dashboard-panel__cards">
        <div className="dashboard-stat-card">
          <span className="dashboard-stat-card__value">{boardCount}</span>
          <span className="dashboard-stat-card__label">工程看板</span>
        </div>
        <div className="dashboard-stat-card">
          <span className="dashboard-stat-card__value">{graphNodeCount}</span>
          <span className="dashboard-stat-card__label">节点总数</span>
        </div>
        <div className="dashboard-stat-card">
          <span className="dashboard-stat-card__value">{graphEdgeCount}</span>
          <span className="dashboard-stat-card__label">边数</span>
        </div>
        <div className="dashboard-stat-card">
          <span className="dashboard-stat-card__value">{graphConnectionCount}</span>
          <span className="dashboard-stat-card__label">连接绑定</span>
        </div>
        <div className="dashboard-stat-card">
          <span className="dashboard-stat-card__value">{activeNodeCount}</span>
          <span className="dashboard-stat-card__label">活跃节点</span>
        </div>
      </div>

      <div className="dashboard-telemetry-grid">
        <article className="dashboard-chart-card">
          <div className="dashboard-chart-card__header">
            <strong>结构负载</strong>
            <span>{activeBoardName ?? '全局工程'}</span>
          </div>
          <div className="dashboard-structure-chart" aria-hidden="true">
            {structureMetrics.map((metric) => (
              <div key={metric.label} className="dashboard-structure-chart__item">
                <div className="dashboard-structure-chart__rail">
                  <div
                    className="dashboard-structure-chart__bar"
                    style={{ height: `${Math.max((metric.value / structurePeak) * 100, 10)}%` }}
                  />
                </div>
                <strong>{metric.value}</strong>
                <span>{metric.label}</span>
              </div>
            ))}
          </div>
        </article>

        <article className="dashboard-chart-card">
          <div className="dashboard-chart-card__header">
            <strong>运行态分布</strong>
            <span>{runtimeHealthyCount} 正常通道</span>
          </div>
          <div className="dashboard-runtime-strip" aria-hidden="true">
            {runtimeMetrics.map((metric) => (
              <div
                key={metric.label}
                className={`dashboard-runtime-strip__segment is-${metric.tone}`}
                style={{ flexGrow: metric.value === 0 ? 0.18 : metric.value }}
              />
            ))}
          </div>
          <div className="dashboard-runtime-legend">
            {runtimeMetrics.map((metric) => (
              <div key={metric.label} className="dashboard-runtime-legend__item">
                <span className={`dashboard-runtime-legend__dot is-${metric.tone}`} />
                <strong>{metric.value}</strong>
                <span>{metric.label}</span>
              </div>
            ))}
          </div>
        </article>

        <article className="dashboard-chart-card dashboard-chart-card--pulse">
          <div className="dashboard-chart-card__header">
            <strong>会话热度</strong>
            <span>
              {eventCount} 事件 / {resultCount} 输出
            </span>
          </div>
          <div className="dashboard-pulse-chart" aria-hidden="true">
            {pulseSeries.map((value, index) => (
              <span
                key={`${value}-${index}`}
                className="dashboard-pulse-chart__bar"
                style={{ height: `${Math.max((value / pulsePeak) * 100, 12)}%` }}
              />
            ))}
          </div>
          <div className="dashboard-gauge" style={gaugeStyle}>
            <div className="dashboard-gauge__ring">
              <div className="dashboard-gauge__center">
                <strong>{readinessScore}%</strong>
                <span>{deployInfo ? '已就绪' : '待部署'}</span>
              </div>
            </div>
            <div className="dashboard-gauge__meta">
              <span>部署就绪度</span>
              <strong>
                {deployInfo ? `根节点 ${deployInfo.rootNodes.length}` : '未生成运行快照'}
              </strong>
            </div>
          </div>
        </article>
      </div>

      <div className="dashboard-panel__status">
        <div className="dashboard-status-row">
          <span className="dashboard-status-row__label">会话摘要</span>
          <span className="dashboard-status-row__value">{statusMessage}</span>
        </div>
        <div className="dashboard-status-row">
          <span className="dashboard-status-row__label">部署信息</span>
          <span className="dashboard-status-row__value">
            {deployInfo
              ? `${deployInfo.nodeCount} 节点 · 根: ${deployInfo.rootNodes.join(', ')}`
              : '当前尚未部署工作流'}
          </span>
        </div>
      </div>

      <div className="dashboard-panel__actions">
        <button type="button" className="dashboard-action-card" data-testid="dashboard-navigate-boards" onClick={onNavigateToBoards}>
          <strong>所有看板</strong>
          <span>查看并管理所有工程看板</span>
          <span className="dashboard-action-card__arrow">→</span>
        </button>
      </div>
    </div>
  );
}
