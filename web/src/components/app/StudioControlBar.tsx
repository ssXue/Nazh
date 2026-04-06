import type { StudioControlBarProps } from './types';

export function StudioControlBar({
  workflowStatusLabel,
  workflowStatusPillClass,
  isTauriRuntime,
  runtimeModeLabel,
  runtimeSnapshot,
  runtimeUpdatedLabel,
  statusMessage,
  graphNodeCount,
  graphEdgeCount,
  graphConnectionCount,
  activeNodeCount,
  canDispatchPayload,
  onDeploy,
  onDispatchPayload,
  onRefreshConnections,
}: StudioControlBarProps) {
  return (
    <section className="studio-controlbar">
      <div className="studio-controlbar__status">
        <span className={`runtime-pill ${workflowStatusPillClass}`}>{workflowStatusLabel}</span>
        <span className={`hero__runtime ${isTauriRuntime ? 'is-live' : 'is-preview'}`}>
          {runtimeModeLabel}
        </span>
        <strong>{runtimeSnapshot}</strong>
        <span className="studio-controlbar__freshness">更新时间: {runtimeUpdatedLabel}</span>
      </div>

      <p className="studio-controlbar__message">{statusMessage}</p>

      <div className="studio-controlbar__metrics">
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

      <div className="studio-controlbar__actions">
        <button type="button" onClick={onDeploy}>
          部署工作流
        </button>
        <button type="button" onClick={onDispatchPayload} disabled={!canDispatchPayload}>
          发送测试消息
        </button>
        <button type="button" className="ghost" onClick={onRefreshConnections}>
          刷新连接
        </button>
      </div>
    </section>
  );
}
