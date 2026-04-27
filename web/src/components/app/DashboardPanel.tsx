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
  const runtimeMetrics = [
    { label: '运行中', value: activeNodeCount, tone: 'active' },
    { label: '完成', value: completedNodeCount, tone: 'success' },
    { label: '失败', value: failedNodeCount, tone: 'danger' },
    { label: '输出', value: outputNodeCount, tone: 'warning' },
  ] as const;
  const runtimeHealthyCount = activeNodeCount + completedNodeCount + outputNodeCount;
  const structureMetrics = [
    { label: '看板', value: boardCount },
    { label: '节点', value: graphNodeCount },
    { label: '边', value: graphEdgeCount },
    { label: '绑定', value: graphConnectionCount },
  ];
  const structurePeak = Math.max(...structureMetrics.map((m) => m.value), 1);

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

  const totalProcessed = completedNodeCount + failedNodeCount + outputNodeCount;
  const successRate = totalProcessed > 0 ? Math.round((completedNodeCount / totalProcessed) * 100) : null;
  const throughput = eventCount + resultCount;
  const hasFailure = failedNodeCount > 0;
  const healthScore = successRate ?? (deployInfo ? 100 : 0);

  const gaugeStyle = {
    '--dashboard-gauge-progress': `${healthScore}%`,
  } as CSSProperties;

  return (
    <div className="dashboard-panel">
      <div className="dashboard-panel__header window-safe-header" data-window-drag-region>
        <h2>总览</h2>
      </div>

      <section className="dashboard-hero">
        <div className="dashboard-hero__context">
          <span className="dashboard-hero__eyebrow">OPERATOR · {userId}</span>
          <strong className="dashboard-hero__title">
            {activeBoardName ? activeBoardName : 'Nazh 控制台'}
          </strong>
          <span className="dashboard-hero__deploy">
            {deployInfo
              ? `已部署 ${deployInfo.nodeCount} 节点 · 根: ${deployInfo.rootNodes.join(', ')}`
              : '未部署工作流'}
          </span>
        </div>

        <div className="dashboard-hero__health" style={gaugeStyle}>
          <div className="dashboard-hero__health-ring">
            <div className="dashboard-hero__health-center">
              <strong>{healthScore}%</strong>
              <span>{hasFailure ? '异常' : deployInfo ? '健康' : '待机'}</span>
            </div>
          </div>
          <span className="dashboard-hero__health-label">系统健康度</span>
        </div>

        <div className="dashboard-hero__metrics">
          <div className="dashboard-hero__metric">
            <span className="dashboard-hero__metric-value is-active">{activeNodeCount}</span>
            <span className="dashboard-hero__metric-label">运行中</span>
          </div>
          <div className="dashboard-hero__metric">
            <span className="dashboard-hero__metric-value">{throughput}</span>
            <span className="dashboard-hero__metric-label">吞吐量</span>
          </div>
          <div className="dashboard-hero__metric">
            <span className="dashboard-hero__metric-value is-success">
              {successRate !== null ? `${successRate}%` : '--'}
            </span>
            <span className="dashboard-hero__metric-label">成功率</span>
          </div>
          <div className={`dashboard-hero__metric${hasFailure ? ' has-alert' : ''}`}>
            <span className="dashboard-hero__metric-value">{failedNodeCount}</span>
            <span className="dashboard-hero__metric-label">异常</span>
          </div>
        </div>
      </section>

      <div className="dashboard-telemetry-grid">
        <article className="dashboard-chart-card dashboard-chart-card--dense">
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
        </article>

        <article className="dashboard-chart-card dashboard-chart-card--status">
          <div className="dashboard-chart-card__header">
            <strong>工作面摘要</strong>
          </div>
          <div className="dashboard-status-list">
            <div className="dashboard-status-list__row">
              <span>当前焦点</span>
              <strong>{activeBoardName ? activeBoardName : '全局工作台'}</strong>
            </div>
            <div className="dashboard-status-list__row">
              <span>最近状态</span>
              <strong>{statusMessage || '暂无'}</strong>
            </div>
          </div>
          <button
            type="button"
            className="dashboard-action-card"
            data-testid="dashboard-navigate-boards"
            onClick={onNavigateToBoards}
          >
            <strong>进入所有看板</strong>
            <span className="dashboard-action-card__arrow">→</span>
          </button>
        </article>
      </div>
    </div>
  );
}
