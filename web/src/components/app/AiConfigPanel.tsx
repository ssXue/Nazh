import { useState } from 'react';

import type { AiConfigPanelProps } from './types';
import type {
  AiConfigUpdate,
  AiProviderUpsert,
  AiSecretInput,
} from '../../types';

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

interface ProviderPreset {
  label: string;
  name: string;
  baseUrl: string;
  defaultModel: string;
}

const PROVIDER_PRESETS: ProviderPreset[] = [
  { label: 'DeepSeek', name: 'DeepSeek', baseUrl: 'https://api.deepseek.com/v1', defaultModel: 'deepseek-chat' },
  { label: 'OpenAI', name: 'OpenAI', baseUrl: 'https://api.openai.com/v1', defaultModel: 'gpt-4o-mini' },
  { label: '月之暗面', name: 'Moonshot', baseUrl: 'https://api.moonshot.cn/v1', defaultModel: 'moonshot-v1-8k' },
  { label: '智谱', name: 'Zhipu', baseUrl: 'https://open.bigmodel.cn/api/paas/v4', defaultModel: 'glm-4-flash' },
  { label: '通义千问', name: 'Qwen', baseUrl: 'https://dashscope.aliyuncs.com/compatible-mode/v1', defaultModel: 'qwen-turbo' },
  { label: '硅基流动', name: 'SiliconFlow', baseUrl: 'https://api.siliconflow.cn/v1', defaultModel: 'Qwen/Qwen2.5-7B-Instruct' },
  { label: 'Ollama 本地', name: 'Ollama', baseUrl: 'http://localhost:11434/v1', defaultModel: 'qwen2.5:7b' },
];

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

  function handleSelectPreset(preset: ProviderPreset) {
    setForm((prev) => ({
      ...prev,
      name: preset.name,
      baseUrl: preset.baseUrl,
      defaultModel: preset.defaultModel,
    }));
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

  function handleConfirmAdd() {
    if (!aiConfig) return;

    const newId = crypto.randomUUID();

    const existingUpserts: AiProviderUpsert[] = aiConfig.providers.map((p) => ({
      id: p.id,
      name: p.name,
      baseUrl: p.baseUrl,
      defaultModel: p.defaultModel,
      extraHeaders: p.extraHeaders,
      enabled: p.enabled,
      apiKey: { kind: 'keep' } as AiSecretInput,
    }));

    const newProvider: AiProviderUpsert = {
      id: newId,
      name: form.name.trim(),
      baseUrl: form.baseUrl.trim(),
      defaultModel: form.defaultModel.trim(),
      extraHeaders: {},
      enabled: true,
      apiKey: { kind: 'set', value: form.apiKey.trim() } as AiSecretInput,
    };

    const update: AiConfigUpdate = {
      version: aiConfig.version,
      providers: [...existingUpserts, newProvider],
      activeProviderId: aiConfig.activeProviderId ?? newId,
      copilotParams: aiConfig.copilotParams,
    };

    void onAiConfigSave(update);
    setForm(EMPTY_FORM);
    setShowForm(false);
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

              {aiConfig.providers.length === 0 ? (
                <article className="settings-row">
                  <span className="settings-row__label">尚未配置任何提供商。</span>
                </article>
              ) : (
                <>
                  <article className="settings-row">
                    <strong className="settings-row__label">已配置提供商</strong>
                    <span className="settings-row__value">
                      {aiConfig.providers
                        .map((p) => `${p.name}${p.enabled ? '' : '（已禁用）'}`)
                        .join('、')}
                    </span>
                  </article>

                  <article className="settings-row">
                    <strong className="settings-row__label">激活提供商</strong>
                    <span className="settings-row__value">
                      {aiConfig.activeProviderId
                        ? aiConfig.providers.find((p) => p.id === aiConfig.activeProviderId)
                            ?.name ?? '未选择'
                        : '未选择'}
                    </span>
                  </article>
                </>
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
                    <strong className="settings-row__label">快速选择厂商</strong>
                    <div className="settings-accent-inline" role="group" aria-label="预置厂商">
                      {PROVIDER_PRESETS.map((preset) => {
                        const isActive =
                          form.name === preset.name &&
                          form.baseUrl === preset.baseUrl;
                        return (
                          <button
                            key={preset.label}
                            type="button"
                            className={
                              isActive
                                ? 'settings-accent-chip is-active'
                                : 'settings-accent-chip'
                            }
                            onClick={() => handleSelectPreset(preset)}
                          >
                            <span>{preset.label}</span>
                          </button>
                        );
                      })}
                    </div>
                  </article>

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
                        disabled={!isFormValid}
                        onClick={handleConfirmAdd}
                      >
                        确认添加
                      </button>
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
