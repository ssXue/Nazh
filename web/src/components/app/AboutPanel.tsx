import nazhLogo from '../../assets/nazh-logo.svg';
import type { AboutPanelProps } from './types';

export function AboutPanel({
  isTauriRuntime,
  runtimeModeLabel,
  graphNodeCount,
  graphConnectionCount,
  deployInfo,
}: AboutPanelProps) {
  return (
    <>
      <div className="panel__header panel__header--desktop">
        <div>
          <h2>帮助与关于</h2>
        </div>
        <span className="panel__badge">Nazh</span>
      </div>

      <div className="panel__section panel__section--stacked">
        <article className="about-brand-card">
          <img className="about-brand-card__logo" src={nazhLogo} alt="Nazh logo" />
          <div>
            <strong>Nazh</strong>
            <span>工业边缘流程编排台</span>
            <p>{runtimeModeLabel}</p>
          </div>
        </article>
      </div>

      <div className="panel__section panel__section--stacked">
        <div className="metric-grid metric-grid--ops">
          <article>
            <span>画布节点</span>
            <strong>{graphNodeCount}</strong>
          </article>
          <article>
            <span>连接资源</span>
            <strong>{graphConnectionCount}</strong>
          </article>
          <article>
            <span>桌面会话</span>
            <strong>{isTauriRuntime ? '已连接' : '未连接'}</strong>
          </article>
          <article>
            <span>部署状态</span>
            <strong>{deployInfo ? '已部署' : '未部署'}</strong>
          </article>
        </div>
      </div>

      <div className="panel__section panel__section--stacked">
        <div className="rail-list">
          <article className="rail-card">
            <strong>技术栈</strong>
            <span>Tauri / Rust / React / FlowGram</span>
          </article>
          <article className="rail-card">
            <strong>当前能力</strong>
            <span>工作流编辑 / 连接资源 / 运行观测 / FlowGram 预检</span>
          </article>
          <article className="rail-card">
            <strong>当前版本</strong>
            <span>V 1.0.0</span>
          </article>
        </div>
      </div>
    </>
  );
}
