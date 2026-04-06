import type { SettingsPanelProps } from './types';

export function SettingsPanel({
  isTauriRuntime,
  runtimeModeLabel,
  workflowStatusLabel,
  statusMessage,
  themeMode,
}: SettingsPanelProps) {
  return (
    <>
      <div className="panel__header panel__header--desktop">
        <div>
          <h2>设置</h2>
        </div>
        <span className="panel__badge">{runtimeModeLabel}</span>
      </div>

      <div className="panel__section panel__section--stacked">
        <div className="metric-grid metric-grid--ops">
          <article>
            <span>运行模式</span>
            <strong>{runtimeModeLabel}</strong>
          </article>
          <article>
            <span>工作流状态</span>
            <strong>{workflowStatusLabel}</strong>
          </article>
          <article>
            <span>窗口适配</span>
            <strong>{isTauriRuntime ? '已启用' : '仅预览'}</strong>
          </article>
          <article>
            <span>主题模式</span>
            <strong>{themeMode === 'dark' ? '暗色' : '亮色'}</strong>
          </article>
        </div>
      </div>

      <div className="panel__section panel__section--stacked">
        <div className="rail-list">
          <article className="rail-card">
            <strong>窗口行为</strong>
            <span>屏幕自适应</span>
            <p>窗体尺寸跟随当前显示器工作区。</p>
          </article>
          <article className="rail-card">
            <strong>交互保护</strong>
            <span>桌面优先</span>
            <p>已禁用网页右键菜单、缩放手势与无关文本选中。</p>
          </article>
          <article className="rail-card">
            <strong>当前状态</strong>
            <span>{workflowStatusLabel}</span>
            <p>{statusMessage}</p>
          </article>
        </div>
      </div>
    </>
  );
}
