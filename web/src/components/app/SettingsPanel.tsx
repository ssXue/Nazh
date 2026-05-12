import type { CSSProperties } from 'react';
import { useCallback, useState } from 'react';

import { open as openDialog } from '@tauri-apps/plugin-dialog';

import {
  type CanvasZoomSpeed,
  getCanvasZoomSpeed,
  setCanvasZoomSpeed,
} from '../flowgram/flowgram-canvas-utils';
import { FolderOpenIcon, ResetIcon } from './AppIcons';
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
  projectWorkspaceBoardsDirectoryPath,
  projectWorkspaceUsingDefault,
  projectWorkspaceIsSyncing,
  projectWorkspaceError,
  onProjectWorkspacePathChange,
  gridVisible,
  onGridVisibleChange,
}: SettingsPanelProps) {
  const [zoomSpeed, setZoomSpeed] = useState<CanvasZoomSpeed>(getCanvasZoomSpeed);

  const handlePickFolder = useCallback(async () => {
    try {
      const selected = await openDialog({
        directory: true,
        multiple: false,
        title: '选择工程路径',
      });
      if (typeof selected === 'string' && selected.length > 0) {
        onProjectWorkspacePathChange(selected);
      }
    } catch {
      // 用户取消或对话框不可用，静默忽略
    }
  }, [onProjectWorkspacePathChange]);

  return (
    <>
      <div
        className="panel__header panel__header--desktop window-safe-header"
        data-window-drag-region
      >
        <div className="panel__header__heading">
          <h2>设置</h2>
        </div>
        <div className="panel__header-actions">
          <span className="panel__badge">{runtimeModeLabel}</span>
        </div>
      </div>

      <div className="settings-panel">
        <section className="settings-group">
          <div className="settings-group__header">
            <h3>偏好设置</h3>
          </div>

          <article className="settings-row">
            <strong className="settings-row__label">主题模式</strong>
            <div className="settings-segment" role="group" aria-label="主题模式" data-testid="settings-theme-toggle">
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
              <button
                type="button"
                className={themeMode === 'system' ? 'settings-segment__button is-active' : 'settings-segment__button'}
                aria-pressed={themeMode === 'system'}
                onClick={() => onThemeModeChange('system')}
              >
                跟随系统
              </button>
            </div>
          </article>

          <article className="settings-row">
            <strong className="settings-row__label">主题色</strong>
            <div className="settings-accent-inline" role="group" aria-label="主题色" data-testid="settings-accent-preset">
              {accentOptions.map((option) => (
                <button
                  key={option.key}
                  type="button"
                  className={accentPreset === option.key ? 'settings-accent-chip is-active' : 'settings-accent-chip'}
                  aria-pressed={accentPreset === option.key}
                  aria-label={option.label}
                  onClick={() => onAccentPresetChange(option.key)}
                >
                  <span
                    className="settings-accent-chip__dot"
                    style={{ '--settings-accent-chip': option.hex } as CSSProperties}
                  />
                  <span>{option.label}</span>
                </button>
              ))}
              <label className="settings-accent-chip settings-accent-chip--custom">
                <input
                  type="color"
                  value={customAccentHex}
                  aria-label="自定义主题色"
                  onChange={(event) => onCustomAccentChange(event.target.value)}
                />
                <span>自定义</span>
              </label>
            </div>
          </article>

          <article className="settings-row">
            <strong className="settings-row__label">动效强度</strong>
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
            <strong className="settings-row__label">启动页</strong>
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

          <article className="settings-row">
            <strong className="settings-row__label">画布网格</strong>
            <div className="settings-segment" role="group" aria-label="画布网格">
              <button
                type="button"
                className={gridVisible ? 'settings-segment__button is-active' : 'settings-segment__button'}
                aria-pressed={gridVisible}
                onClick={() => onGridVisibleChange(true)}
              >
                显示
              </button>
              <button
                type="button"
                className={!gridVisible ? 'settings-segment__button is-active' : 'settings-segment__button'}
                aria-pressed={!gridVisible}
                onClick={() => onGridVisibleChange(false)}
              >
                隐藏
              </button>
            </div>
          </article>

          <article className="settings-row">
            <strong className="settings-row__label">滚轮缩放</strong>
            <div className="settings-segment" role="group" aria-label="滚轮缩放">
              <button
                type="button"
                className={zoomSpeed === 'slow' ? 'settings-segment__button is-active' : 'settings-segment__button'}
                aria-pressed={zoomSpeed === 'slow'}
                onClick={() => { setZoomSpeed('slow'); setCanvasZoomSpeed('slow'); }}
              >
                慢
              </button>
              <button
                type="button"
                className={zoomSpeed === 'normal' ? 'settings-segment__button is-active' : 'settings-segment__button'}
                aria-pressed={zoomSpeed === 'normal'}
                onClick={() => { setZoomSpeed('normal'); setCanvasZoomSpeed('normal'); }}
              >
                标准
              </button>
              <button
                type="button"
                className={zoomSpeed === 'fast' ? 'settings-segment__button is-active' : 'settings-segment__button'}
                aria-pressed={zoomSpeed === 'fast'}
                onClick={() => { setZoomSpeed('fast'); setCanvasZoomSpeed('fast'); }}
              >
                快
              </button>
            </div>
          </article>
        </section>

        <section className="settings-group">
          <div className="settings-group__header">
            <h3>工程路径</h3>
          </div>

          <div className="settings-path-editor">
            <div className="settings-path-card">
              <div className="settings-path-card__row">
                <code className="settings-path-card__path">
                  {projectWorkspaceResolvedPath ?? '等待桌面端解析'}
                </code>
                <div className="settings-path-card__actions">
                  <button
                    type="button"
                    className="settings-icon-button"
                    title="选择文件夹"
                    disabled={!isTauriRuntime || projectWorkspaceIsSyncing}
                    onClick={handlePickFolder}
                  >
                    <FolderOpenIcon width={16} height={16} />
                  </button>
                  <button
                    type="button"
                    className="settings-icon-button settings-icon-button--ghost"
                    title="恢复默认"
                    disabled={
                      !isTauriRuntime ||
                      (projectWorkspacePath.trim().length === 0 && !projectWorkspaceIsSyncing)
                    }
                    onClick={() => {
                      onProjectWorkspacePathChange('');
                    }}
                  >
                    <ResetIcon width={16} height={16} />
                  </button>
                </div>
              </div>
              <div className="settings-path-card__meta">
                {projectWorkspaceError ? (
                  <span className="settings-path-badge settings-path-badge--error">
                    {projectWorkspaceError}
                  </span>
                ) : projectWorkspaceIsSyncing ? (
                  <span className="settings-path-badge settings-path-badge--syncing">
                    同步中…
                  </span>
                ) : (
                  <span
                    className={
                      projectWorkspaceUsingDefault
                        ? 'settings-path-badge'
                        : 'settings-path-badge settings-path-badge--custom'
                    }
                  >
                    {projectWorkspaceUsingDefault ? '默认' : '自定义'}
                  </span>
                )}
                {projectWorkspaceBoardsDirectoryPath && (
                  <span className="settings-path-card__boards">
                    看板 {projectWorkspaceBoardsDirectoryPath}
                  </span>
                )}
              </div>
            </div>
          </div>
        </section>
      </div>
    </>
  );
}
