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
  aiConfig,
  aiConfigLoading,
  aiConfigError,
  onAiConfigSave,
  onAiProviderTest,
  aiTestResult,
  aiTesting,
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
            <h3>偏好设置</h3>
          </div>

          <article className="settings-row">
            <strong className="settings-row__label">主题模式</strong>
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

          <article className="settings-row">
            <strong className="settings-row__label">主题色</strong>
            <div className="settings-accent-inline" role="group" aria-label="主题色">
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
        </section>

        <section className="settings-group">
          <div className="settings-group__header">
            <h3>工程路径</h3>
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
        </section>
        <section className="settings-group">
          <div className="settings-group__header">
            <h3>AI 配置</h3>
          </div>

          {!isTauriRuntime ? (
            <article className="settings-row">
              <span className="settings-row__label" style={{ color: 'var(--text-tertiary)' }}>
                AI 配置仅在桌面端可用。
              </span>
            </article>
          ) : aiConfigLoading ? (
            <article className="settings-row">
              <span className="settings-row__label">正在加载 AI 配置...</span>
            </article>
          ) : aiConfig ? (
            <>
              {aiConfigError && (
                <article className="settings-row">
                  <span className="settings-row__label" style={{ color: 'var(--color-error)' }}>
                    {aiConfigError}
                  </span>
                </article>
              )}

              <article className="settings-row">
                <strong className="settings-row__label">已配置提供商</strong>
                <span className="settings-row__value">
                  {aiConfig.providers.length === 0
                    ? '尚未配置'
                    : aiConfig.providers
                        .map((p) => `${p.name}${p.enabled ? '' : '（已禁用）'}`)
                        .join('、')}
                </span>
              </article>

              {aiConfig.providers.length > 0 && (
                <article className="settings-row">
                  <strong className="settings-row__label">激活提供商</strong>
                  <span className="settings-row__value">
                    {aiConfig.activeProviderId
                      ? aiConfig.providers.find((p) => p.id === aiConfig.activeProviderId)
                          ?.name ?? '未选择'
                      : '未选择'}
                  </span>
                </article>
              )}

              {aiTestResult && (
                <article className="settings-row">
                  <strong className="settings-row__label">测试结果</strong>
                  <span
                    className="settings-row__value"
                    style={{
                      color: aiTestResult.success
                        ? 'var(--color-success)'
                        : 'var(--color-error)',
                    }}
                  >
                    {aiTestResult.message}
                  </span>
                </article>
              )}

              <article className="settings-row">
                <button
                  type="button"
                  className="settings-inline-button"
                  disabled={aiTesting}
                  onClick={() => {
                    const name = window.prompt('提供商名称', 'DeepSeek');
                    if (!name) return;
                    const baseUrl = window.prompt(
                      'API Base URL',
                      'https://api.deepseek.com/v1',
                    );
                    if (!baseUrl) return;
                    const apiKey = window.prompt('API Key');
                    if (!apiKey) return;
                    const model = window.prompt('默认模型', 'deepseek-chat');
                    if (!model) return;

                    void onAiProviderTest({
                      id: undefined,
                      name,
                      baseUrl,
                      apiKey,
                      defaultModel: model,
                      extraHeaders: {},
                      enabled: true,
                    });
                  }}
                >
                  {aiTesting ? '测试中...' : '测试新提供商'}
                </button>
              </article>
            </>
          ) : (
            <article className="settings-row">
              <span className="settings-row__label">无法加载 AI 配置。</span>
            </article>
          )}
        </section>
      </div>
    </>
  );
}
