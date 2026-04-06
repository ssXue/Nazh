import type { OverviewPanelProps } from './types';

export function OverviewPanel({
  graphNodeCount,
  graphEdgeCount,
  graphConnectionCount,
  activeNodeCount,
  workflowStatusLabel,
  workflowStatusPillClass,
  statusMessage,
  runtimeSnapshot,
  runtimeUpdatedLabel,
  deployInfo,
}: OverviewPanelProps) {
  return (
    <>
      <div className="panel__header panel__header--desktop">
        <div>
          <h2>管理总览</h2>
        </div>
        <span className={`runtime-pill ${workflowStatusPillClass}`}>{workflowStatusLabel}</span>
      </div>

      <div className="panel__section panel__section--stacked">
        <div className="metric-grid metric-grid--ops">
          <article>
            <span>节点</span>
            <strong>{graphNodeCount}</strong>
          </article>
          <article>
            <span>边数</span>
            <strong>{graphEdgeCount}</strong>
          </article>
          <article>
            <span>连接</span>
            <strong>{graphConnectionCount}</strong>
          </article>
          <article>
            <span>运行中</span>
            <strong>{activeNodeCount}</strong>
          </article>
        </div>
      </div>

      <div className="panel__section panel__section--stacked">
        <div className="rail-list">
          <article className="rail-card">
            <strong>窗体工作流状态</strong>
            <span>{workflowStatusLabel}</span>
            <p>{statusMessage}</p>
          </article>
          <article className="rail-card">
            <strong>最近活动</strong>
            <span>{runtimeSnapshot}</span>
            <p>最近更新时间: {runtimeUpdatedLabel}</p>
          </article>
          <article className="rail-card">
            <strong>部署快照</strong>
            <span>{deployInfo ? `${deployInfo.nodeCount} 节点 / ${deployInfo.edgeCount} 边` : '尚未部署'}</span>
            {deployInfo ? <p>{`根节点: ${deployInfo.rootNodes.join(', ')}`}</p> : null}
          </article>
        </div>
      </div>
    </>
  );
}
