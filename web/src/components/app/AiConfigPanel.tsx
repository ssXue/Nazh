import { useState } from 'react';

import type { AiConfigPanelProps } from './types';

interface ProviderFormState {
  name: string;
  baseUrl: string;
  apiKey: string;
  defaultModel: string;
}

const EMPTY_FORM: ProviderFormState = {
  name: '',
  baseUrl: '',
  apiKey: '',
  defaultModel: '',
};

export function AiConfigPanel({
  isTauriRuntime,
  aiConfig,
  aiConfigLoading,
  aiConfigError,
  onAiConfigSave,
  onAiProviderTest,
  aiTestResult,
  aiTesting,
}: AiConfigPanelProps) {
  const [showForm, setShowForm] = useState(false);
  const [form, setForm] = useState<ProviderFormState>(EMPTY_FORM);

  function handleFormChange(field: keyof ProviderFormState, value: string) {
    setForm((prev) => ({ ...prev, [field]: value }));
  }

  function handleSubmitTest() {
    void onAiProviderTest({
      id: undefined,
      name: form.name,
      baseUrl: form.baseUrl,
      apiKey: form.apiKey,
      defaultModel: form.defaultModel,
      extraHeaders: {},
      enabled: true,
    });
  }

  function handleResetForm() {
    setForm(EMPTY_FORM);
    setShowForm(false);
  }

  const isFormValid =
    form.name.trim().length > 0 &&
    form.baseUrl.trim().length > 0 &&
    form.apiKey.trim().length > 0 &&
    form.defaultModel.trim().length > 0;

  return (
    <>
      <div
        className="panel__header panel__header--desktop window-safe-header"
        data-window-drag-region
      >
        <div>
          <h2>AI 配置</h2>
        </div>
        <span className="panel__badge">{isTauriRuntime ? 'Copilot' : '预览态'}</span>
      </div>

      <div className="settings-panel">
        {!isTauriRuntime ? (
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
        ) : aiConfigLoading ? (
          <section className="settings-group">
            <div className="settings-group__header">
              <h3>加载中</h3>
            </div>
            <article className="settings-row">
              <span className="settings-row__label">正在加载 AI 配置...</span>
            </article>
          </section>
        ) : aiConfig ? (
          <>
            <section className="settings-group">
              <div className="settings-group__header">
                <h3>状态概览</h3>
              </div>

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
            </section>

            {aiTestResult && (
              <section className="settings-group">
                <div className="settings-group__header">
                  <h3>最近测试结果</h3>
                </div>
                <article className="settings-row">
                  <strong className="settings-row__label">连接测试</strong>
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
              </section>
            )}

            <section className="settings-group">
              <div className="settings-group__header">
                <h3>添加提供商</h3>
              </div>

              {!showForm ? (
                <article className="settings-row">
                  <button
                    type="button"
                    className="settings-inline-button"
                    onClick={() => setShowForm(true)}
                  >
                    添加新提供商
                  </button>
                </article>
              ) : (
                <>
                  <article className="settings-row settings-row--stacked">
                    <label className="settings-row__label" htmlFor="ai-provider-name">
                      提供商名称
                    </label>
                    <input
                      id="ai-provider-name"
                      className="settings-path-input"
                      type="text"
                      placeholder="例如：DeepSeek"
                      value={form.name}
                      onChange={(e) => handleFormChange('name', e.target.value)}
                    />
                  </article>

                  <article className="settings-row settings-row--stacked">
                    <label className="settings-row__label" htmlFor="ai-provider-url">
                      API Base URL
                    </label>
                    <input
                      id="ai-provider-url"
                      className="settings-path-input"
                      type="text"
                      placeholder="例如：https://api.deepseek.com/v1"
                      value={form.baseUrl}
                      onChange={(e) => handleFormChange('baseUrl', e.target.value)}
                    />
                  </article>

                  <article className="settings-row settings-row--stacked">
                    <label className="settings-row__label" htmlFor="ai-provider-key">
                      API Key
                    </label>
                    <input
                      id="ai-provider-key"
                      className="settings-path-input"
                      type="password"
                      placeholder="sk-..."
                      value={form.apiKey}
                      onChange={(e) => handleFormChange('apiKey', e.target.value)}
                    />
                  </article>

                  <article className="settings-row settings-row--stacked">
                    <label className="settings-row__label" htmlFor="ai-provider-model">
                      默认模型
                    </label>
                    <input
                      id="ai-provider-model"
                      className="settings-path-input"
                      type="text"
                      placeholder="例如：deepseek-chat"
                      value={form.defaultModel}
                      onChange={(e) => handleFormChange('defaultModel', e.target.value)}
                    />
                  </article>

                  <article className="settings-row">
                    <div className="settings-path-actions">
                      <button
                        type="button"
                        className="settings-inline-button"
                        disabled={!isFormValid || aiTesting}
                        onClick={handleSubmitTest}
                      >
                        {aiTesting ? '测试中...' : '测试连接'}
                      </button>
                      <button
                        type="button"
                        className="settings-inline-button settings-inline-button--ghost"
                        onClick={handleResetForm}
                      >
                        取消
                      </button>
                    </div>
                  </article>
                </>
              )}
            </section>
          </>
        ) : (
          <section className="settings-group">
            <div className="settings-group__header">
              <h3>加载失败</h3>
            </div>
            <article className="settings-row">
              <span className="settings-row__label">无法加载 AI 配置。</span>
            </article>
          </section>
        )}
      </div>
    </>
  );
}
