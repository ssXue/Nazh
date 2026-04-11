import type { CSSProperties } from 'react';
import { useEffect, useMemo, useState } from 'react';

import type { SettingsPanelProps } from './types';

export function SettingsPanel({
  isTauriRuntime,
  runtimeModeLabel,
  workflowStatusLabel,
  statusMessage,
  themeMode,
  onThemeModeChange,
  accentPreset,
  accentOptions,
  customAccentHex,
  onAccentPresetChange,
  onCustomAccentChange,
  motionMode,
  onMotionModeChange,
  startupPage,
  onStartupPageChange,
  projectWorkspacePath,
  projectWorkspaceResolvedPath,
  projectWorkspaceLibraryFilePath,
  projectWorkspaceUsingDefault,
  projectWorkspaceIsSyncing,
  projectWorkspaceError,
  onProjectWorkspacePathChange,
}: SettingsPanelProps) {
  const [workspaceDraft, setWorkspaceDraft] = useState(projectWorkspacePath);

  useEffect(() => {
    setWorkspaceDraft(projectWorkspacePath);
  }, [projectWorkspacePath]);

  const isWorkspaceDirty = workspaceDraft.trim() !== projectWorkspacePath.trim();
  const workspaceStatus = useMemo(() => {
    if (!isTauriRuntime) {
      return '当前为浏览器预览，仅保留本地镜像存储。';
    }

    if (projectWorkspaceError) {
      return projectWorkspaceError;
    }

    if (projectWorkspaceIsSyncing) {
      return '正在同步工程库到当前工作路径。';
    }

    return projectWorkspaceUsingDefault
      ? '当前使用应用默认目录。'
      : '当前使用自定义工作目录。';
  }, [
    isTauriRuntime,
    projectWorkspaceError,
    projectWorkspaceIsSyncing,
    projectWorkspaceUsingDefault,
  ]);

  return (
    <>
      <div
        className="panel__header panel__header--desktop window-safe-header"
        data-window-drag-region
      >
        <div>
          <h2>设置</h2>
        </div>
        <span className="panel__badge">{runtimeModeLabel}</span>
      </div>

      <div className="settings-panel">
        <section className="settings-group">
          <div className="settings-group__header">
            <h3>外观</h3>
          </div>

          <article className="settings-row">
            <div className="settings-row__copy">
              <strong>主题模式</strong>
              <span>立即切换亮色或暗色外观。</span>
            </div>
            <div className="settings-segment" role="group" aria-label="主题模式">
              <button
                type="button"
                className={themeMode === 'light' ? 'settings-segment__button is-active' : 'settings-segment__button'}
                aria-pressed={themeMode === 'light'}
                onClick={() => onThemeModeChange('light')}
              >
                亮色
              </button>
              <button
                type="button"
                className={themeMode === 'dark' ? 'settings-segment__button is-active' : 'settings-segment__button'}
                aria-pressed={themeMode === 'dark'}
                onClick={() => onThemeModeChange('dark')}
              >
                暗色
              </button>
            </div>
          </article>

          <article className="settings-row settings-row--stacked">
            <div className="settings-row__copy">
              <strong>主题色</strong>
              <span>统一控制导航高亮、主要按钮、看板入口与流程强调；辅助状态色会自动降饱和。</span>
            </div>
            <div className="settings-accent-grid" role="group" aria-label="主题色">
              {accentOptions.map((option) => (
                <button
                  key={option.key}
                  type="button"
                  className={accentPreset === option.key ? 'settings-accent-swatch is-active' : 'settings-accent-swatch'}
                  aria-pressed={accentPreset === option.key}
                  onClick={() => onAccentPresetChange(option.key)}
                >
                  <span
                    className="settings-accent-swatch__chip"
                    style={{ '--settings-accent-chip': option.hex } as CSSProperties}
                  />
                  <strong>{option.label}</strong>
                </button>
              ))}
              <label
                className={accentPreset === 'custom' ? 'settings-accent-custom is-active' : 'settings-accent-custom'}
              >
                <input
                  type="color"
                  value={customAccentHex}
                  aria-label="自定义主题色"
                  onChange={(event) => onCustomAccentChange(event.target.value)}
                />
                <span>自定义</span>
                <code>{customAccentHex}</code>
              </label>
            </div>
          </article>
        </section>

        <section className="settings-group">
          <div className="settings-group__header">
            <h3>交互</h3>
          </div>

          <article className="settings-row">
            <div className="settings-row__copy">
              <strong>动效强度</strong>
              <span>减少过渡与动画，适合更稳的桌面操作节奏。</span>
            </div>
            <div className="settings-segment" role="group" aria-label="动效强度">
              <button
                type="button"
                className={motionMode === 'full' ? 'settings-segment__button is-active' : 'settings-segment__button'}
                aria-pressed={motionMode === 'full'}
                onClick={() => onMotionModeChange('full')}
              >
                标准
              </button>
              <button
                type="button"
                className={motionMode === 'reduced' ? 'settings-segment__button is-active' : 'settings-segment__button'}
                aria-pressed={motionMode === 'reduced'}
                onClick={() => onMotionModeChange('reduced')}
              >
                精简
              </button>
            </div>
          </article>

          <article className="settings-row">
            <div className="settings-row__copy">
              <strong>启动页</strong>
              <span>下次打开应用时默认进入的页面。</span>
            </div>
            <div className="settings-segment" role="group" aria-label="启动页">
              <button
                type="button"
                className={startupPage === 'dashboard' ? 'settings-segment__button is-active' : 'settings-segment__button'}
                aria-pressed={startupPage === 'dashboard'}
                onClick={() => onStartupPageChange('dashboard')}
              >
                Dashboard
              </button>
              <button
                type="button"
                className={startupPage === 'boards' ? 'settings-segment__button is-active' : 'settings-segment__button'}
                aria-pressed={startupPage === 'boards'}
                onClick={() => onStartupPageChange('boards')}
              >
                所有看板
              </button>
            </div>
          </article>
        </section>

        <section className="settings-group">
          <div className="settings-group__header">
            <h3>工程</h3>
          </div>

          <article className="settings-row settings-row--stacked">
            <div className="settings-row__copy">
              <strong>工作路径</strong>
              <span>设置工程库的实际存储目录。留空时使用应用默认目录。</span>
            </div>

            <div className="settings-path-editor">
              <div className="settings-path-input-row">
                <input
                  className="settings-path-input"
                  type="text"
                  value={workspaceDraft}
                  placeholder={isTauriRuntime ? '例如：~/Documents/Nazh Workspace' : '仅桌面端可设置'}
                  disabled={!isTauriRuntime || projectWorkspaceIsSyncing}
                  onChange={(event) => setWorkspaceDraft(event.target.value)}
                />

                <div className="settings-path-actions">
                  <button
                    type="button"
                    className="settings-inline-button"
                    disabled={!isTauriRuntime || !isWorkspaceDirty || projectWorkspaceIsSyncing}
                    onClick={() => onProjectWorkspacePathChange(workspaceDraft.trim())}
                  >
                    应用
                  </button>
                  <button
                    type="button"
                    className="settings-inline-button settings-inline-button--ghost"
                    disabled={
                      !isTauriRuntime ||
                      (projectWorkspacePath.trim().length === 0 && !projectWorkspaceIsSyncing)
                    }
                    onClick={() => {
                      setWorkspaceDraft('');
                      onProjectWorkspacePathChange('');
                    }}
                  >
                    默认
                  </button>
                </div>
              </div>

              <div
                className={
                  projectWorkspaceError
                    ? 'settings-path-status settings-path-status--error'
                    : 'settings-path-status'
                }
              >
                <article>
                  <span>状态</span>
                  <strong>{workspaceStatus}</strong>
                </article>
                <article>
                  <span>目录</span>
                  <code>{projectWorkspaceResolvedPath ?? '等待桌面端解析'}</code>
                </article>
                <article>
                  <span>工程库文件</span>
                  <code>{projectWorkspaceLibraryFilePath ?? '等待桌面端解析'}</code>
                </article>
              </div>
            </div>
          </article>
        </section>

        <section className="settings-group settings-group--status">
          <div className="settings-group__header">
            <h3>当前会话</h3>
          </div>

          <div className="settings-summary">
            <article>
              <span>运行环境</span>
              <strong>{runtimeModeLabel}</strong>
            </article>
            <article>
              <span>工作流状态</span>
              <strong>{workflowStatusLabel}</strong>
            </article>
            <article>
              <span>窗口能力</span>
              <strong>{isTauriRuntime ? '桌面增强已启用' : '当前为浏览器预览'}</strong>
            </article>
            <article>
              <span>最近状态</span>
              <strong>{statusMessage}</strong>
            </article>
          </div>
        </section>
      </div>
    </>
  );
}
