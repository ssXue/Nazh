/**
 * AI 配置面板——提供商列表预览子组件。
 *
 * 渲染已配置的提供商卡片列表，包含加载态 / 空态 / 错误提示，
 * 以及每张卡片的「设为全局」「测试」「编辑」「删除」操作。
 */

import type { AiConfigPanelProps } from './types';
import { BorderGlow } from '../animations/BorderGlow';
import {
  CheckCircleIcon,
  DeleteActionIcon,
  PencilIcon,
  PlusIcon,
  SignalIcon,
  SlidersIcon,
} from './AppIcons';

interface AiConfigPreviewProps {
  /** 是否运行在 Tauri 桌面端。 */
  isTauriRuntime: boolean;
  /** 当前 AI 配置（null 表示加载失败 / 未加载）。 */
  aiConfig: AiConfigPanelProps['aiConfig'];
  /** 是否正在加载配置。 */
  aiConfigLoading: boolean;
  /** 配置加载错误信息。 */
  aiConfigError: string | null;
  /** 最近一次连接测试结果。 */
  aiTestResult: AiConfigPanelProps['aiTestResult'];
  /** 是否正在执行连接测试。 */
  aiTesting: boolean;
  /** 当前全局生效的提供商。 */
  activeProvider: NonNullable<AiConfigPanelProps['aiConfig']>['providers'][number] | null;
  /** 设为全局提供商。 */
  onSetGlobalProvider: (providerId: string) => void;
  /** 测试已保存提供商的连通性。 */
  onTestSavedProvider: (providerId: string) => void;
  /** 开始编辑提供商。 */
  onStartEditProvider: (providerId: string) => void;
  /** 删除提供商。 */
  onDeleteProvider: (providerId: string) => void;
  /** 打开 Agent 参数对话框。 */
  onOpenAgentDialog: () => void;
  /** 打开添加提供商表单。 */
  onStartAddProvider: () => void;
}

export function AiConfigPreview({
  isTauriRuntime,
  aiConfig,
  aiConfigLoading,
  aiConfigError,
  aiTestResult,
  aiTesting,
  activeProvider,
  onSetGlobalProvider,
  onTestSavedProvider,
  onStartEditProvider,
  onDeleteProvider,
  onOpenAgentDialog,
  onStartAddProvider,
}: AiConfigPreviewProps) {
  const header = (
    <div
      className="panel__header panel__header--desktop window-safe-header"
      data-window-drag-region
    >
      <div className="panel__header__heading">
        <h2>AI 配置</h2>
        {activeProvider && (
          <span>
            {activeProvider.name} · {activeProvider.defaultModel}
          </span>
        )}
      </div>
      <div className="panel__header-actions" data-no-window-drag>
        <button
          type="button"
          className="ai-config-panel__action"
          data-testid="ai-agent-settings"
          onClick={onOpenAgentDialog}
        >
          <SlidersIcon />
          <span>Agent 参数</span>
        </button>
        <button
          type="button"
          className="ai-config-panel__action"
          data-testid="ai-provider-add"
          onClick={onStartAddProvider}
        >
          <PlusIcon />
          <span>添加连接</span>
        </button>
      </div>
    </div>
  );

  function renderBody() {
    if (!isTauriRuntime) {
      return (
        <section className="settings-group">
          <div className="settings-group__header">
            <h3>运行时限制</h3>
          </div>
          <article className="settings-row">
            <span className="settings-row__label" style={{ color: 'var(--text-tertiary)' }}>
              AI 配置仅在桌面端可用。
            </span>
          </article>
        </section>
      );
    }

    if (aiConfigLoading) {
      return (
        <section className="settings-group">
          <div className="settings-group__header">
            <h3>加载中</h3>
          </div>
          <article className="settings-row">
            <span className="settings-row__label">正在加载 AI 配置...</span>
          </article>
        </section>
      );
    }

    if (aiConfig) {
      return (
        <>
          {aiConfigError ? (
            <article className="ai-config-panel__notice ai-config-panel__notice--error">
              <span style={{ color: 'var(--color-error)' }}>
                {aiConfigError}
              </span>
            </article>
          ) : null}

          {aiTestResult ? (
            <article
              className={
                aiTestResult.success
                  ? 'ai-config-panel__notice ai-config-panel__notice--success'
                  : 'ai-config-panel__notice ai-config-panel__notice--error'
              }
            >
              <strong>连接测试</strong>
              <span>
                {aiTestResult.message}
              </span>
            </article>
          ) : null}

          <div className="ai-config-panel__section">
            {aiConfig.providers.length === 0 ? (
              <p className="ai-config-panel__hint" data-testid="ai-provider-empty-state">尚未配置任何提供商。</p>
            ) : (
              <div className="ai-config-panel__card-list">
                {aiConfig.providers.map((provider) => {
                  const isGlobalProvider = provider.id === activeProvider?.id;
                  const card = (
                    <article
                      key={provider.id}
                      className={`ai-provider-card${isGlobalProvider ? ' ai-provider-card--active' : ''}`}
                      data-testid="ai-provider-card"
                    >
                      <div className="ai-provider-card__top">
                        <strong className="ai-provider-card__name">{provider.name}</strong>
                        <span className={`ai-provider-card__status${isGlobalProvider ? ' ai-provider-card__status--active' : ''}`}>
                          <span className="ai-provider-card__status-dot" />
                          {isGlobalProvider ? '生效中' : '待命'}
                        </span>
                      </div>
                      <div className="ai-provider-card__detail">
                        <span>{provider.baseUrl}</span>
                        <span className="ai-provider-card__sep">·</span>
                        <span>{provider.defaultModel}</span>
                        <span className="ai-provider-card__sep">·</span>
                        <span>{provider.hasApiKey ? 'Key 已配置' : 'Key 未配置'}</span>
                      </div>
                      <div className="ai-provider-card__actions">
                        <button
                          type="button"
                          className="settings-inline-button"
                          disabled={isGlobalProvider}
                          onClick={() => onSetGlobalProvider(provider.id)}
                        >
                          <CheckCircleIcon className="ai-btn-icon" />
                          {isGlobalProvider ? '当前全局' : '设为全局'}
                        </button>
                        <button
                          type="button"
                          className="settings-inline-button settings-inline-button--ghost"
                          disabled={aiTesting}
                          onClick={() => onTestSavedProvider(provider.id)}
                        >
                          <SignalIcon className="ai-btn-icon" />
                          {aiTesting ? '测试中...' : '测试'}
                        </button>
                        <button
                          type="button"
                          className="settings-inline-button settings-inline-button--ghost"
                          onClick={() => onStartEditProvider(provider.id)}
                        >
                          <PencilIcon className="ai-btn-icon" />
                          编辑
                        </button>
                        <button
                          type="button"
                          className="settings-inline-button settings-inline-button--ghost settings-inline-button--danger"
                          onClick={() => onDeleteProvider(provider.id)}
                        >
                          <DeleteActionIcon className="ai-btn-icon" />
                        </button>
                      </div>
                    </article>
                  );

                  return isGlobalProvider ? (
                    <BorderGlow
                      key={provider.id}
                      animated
                      loop={false}
                      colors={['#6e89d6', '#7eaa90', '#c29b6b']}
                      borderRadius={12}
                      glowColor="40 80 80"
                      backgroundColor="linear-gradient(135deg, color-mix(in srgb, var(--accent-soft) 60%, var(--surface-muted)), var(--surface-muted))"
                      className="ai-provider-card--glow-wrapper"
                    >
                      {card}
                    </BorderGlow>
                  ) : (
                    <div key={provider.id} className="ai-provider-card--plain-wrapper">
                      {card}
                    </div>
                  );
                })}
              </div>
            )}
          </div>
        </>
      );
    }

    return (
      <section className="settings-group">
        <div className="settings-group__header">
          <h3>加载失败</h3>
        </div>
        <article className="settings-row">
          <span className="settings-row__label">无法加载 AI 配置。</span>
        </article>
      </section>
    );
  }

  return (
    <>
      {header}
      <div className="ai-config-panel__scroll">
        <div className="settings-panel settings-panel--dense">
          {renderBody()}
        </div>
      </div>
    </>
  );
}
