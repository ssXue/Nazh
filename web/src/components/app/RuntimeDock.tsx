import type { RuntimeDockProps } from './types';

export function RuntimeDock({
  deployInfo,
  runtimeState,
  eventFeed,
  results,
  connectionPreview,
}: RuntimeDockProps) {
  return (
    <section className="runtime-dock" aria-live="polite">
      <div className="runtime-dock__header">
        <div>
          <h2>运行观测</h2>
        </div>
        <span className="panel__badge">
          {runtimeState.traceId ? `追踪 ${runtimeState.traceId}` : '已部署待运行'}
        </span>
      </div>

      <div className="runtime-dock__grid">
        <section className="runtime-dock__panel">
          <div className="runtime-dock__panel-header">
            <div>
              <h3>部署快照</h3>
            </div>
          </div>

          <div className="metric-grid metric-grid--ops">
            <article>
              <span>已部署节点</span>
              <strong>{deployInfo?.nodeCount ?? '--'}</strong>
            </article>
            <article>
              <span>已部署边数</span>
              <strong>{deployInfo?.edgeCount ?? '--'}</strong>
            </article>
            <article>
              <span>最近事件</span>
              <strong>{runtimeState.lastEventType ?? '--'}</strong>
            </article>
            <article>
              <span>根节点</span>
              <strong>{deployInfo?.rootNodes.length ?? 0}</strong>
            </article>
          </div>

          <div className="rail-list">
            <h3>连接池</h3>
            {connectionPreview.length === 0 ? (
              <p>暂无连接</p>
            ) : (
              connectionPreview.map((connection) => (
                <article key={connection.id} className="rail-card">
                  <strong>{connection.id}</strong>
                  <span>{connection.kind}</span>
                  <p>{connection.in_use ? '借出中' : '空闲'}</p>
                </article>
              ))
            )}
          </div>
        </section>

        <section className="runtime-dock__panel runtime-dock__panel--feed">
          <div className="runtime-dock__panel-header">
            <div>
              <h3>执行事件流</h3>
            </div>
          </div>

          <div className="feed feed--compact">
            {eventFeed.length === 0 ? <p>暂无事件</p> : null}
            {eventFeed.map((entry) => (
              <pre key={entry}>{entry}</pre>
            ))}
          </div>
        </section>

        <section className="runtime-dock__panel runtime-dock__panel--feed">
          <div className="runtime-dock__panel-header">
            <div>
              <h3>结果载荷</h3>
            </div>
          </div>

          <div className="feed feed--compact">
            {results.length === 0 ? <p>暂无输出</p> : null}
            {results.map((result) => (
              <pre key={`${result.trace_id}-${result.timestamp}`}>
                {JSON.stringify(result, null, 2)}
              </pre>
            ))}
          </div>
        </section>
      </div>
    </section>
  );
}
