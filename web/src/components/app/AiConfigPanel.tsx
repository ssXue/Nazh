import { useEffect, useMemo, useState } from 'react';

import type { AiConfigPanelProps } from './types';
import type {
  AiAgentSettings,
  AiConfigUpdate,
  AiProviderDraft,
  AiProviderUpsert,
  AiSecretInput,
} from '../../types';

interface ProviderFormState {
  name: string;
  baseUrl: string;
  apiKey: string;
  defaultModel: string;
}

interface AgentSettingsFormState {
  systemPrompt: string;
  temperature: string;
  maxTokens: string;
  topP: string;
  timeoutMs: string;
}

const EMPTY_FORM: ProviderFormState = {
  name: '',
  baseUrl: '',
  apiKey: '',
  defaultModel: '',
};

const EMPTY_AGENT_SETTINGS_FORM: AgentSettingsFormState = {
  systemPrompt: '',
  temperature: '',
  maxTokens: '',
  topP: '',
  timeoutMs: '',
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

function readNumberInput(value: number | bigint | undefined | null): string {
  if (typeof value === 'bigint') {
    return value.toString();
  }

  return typeof value === 'number' && Number.isFinite(value) ? String(value) : '';
}

function parseOptionalFiniteNumber(value: string): number | undefined {
  const normalized = value.trim();
  if (!normalized) {
    return undefined;
  }

  const parsed = Number(normalized);
  return Number.isFinite(parsed) ? parsed : undefined;
}

function parseOptionalPositiveInteger(value: string): number | undefined {
  const normalized = value.trim();
  if (!normalized) {
    return undefined;
  }

  const parsed = Number(normalized);
  if (!Number.isFinite(parsed) || parsed <= 0) {
    return undefined;
  }

  return Math.round(parsed);
}

function parseOptionalPositiveBigInt(value: string): bigint | undefined {
  const normalized = value.trim();
  if (!normalized) {
    return undefined;
  }

  if (!/^\d+$/.test(normalized)) {
    return undefined;
  }

  const parsed = BigInt(normalized);
  return parsed > 0n ? parsed : undefined;
}

function buildProviderUpserts(
  aiConfig: NonNullable<AiConfigPanelProps['aiConfig']>,
  activeProviderId: string | null,
): AiProviderUpsert[] {
  const resolvedActiveProviderId =
    activeProviderId ?? aiConfig.activeProviderId ?? aiConfig.providers[0]?.id ?? null;

  return aiConfig.providers.map((provider) => ({
    id: provider.id,
    name: provider.name,
    baseUrl: provider.baseUrl,
    defaultModel: provider.defaultModel,
    extraHeaders: provider.extraHeaders,
    enabled: provider.id === resolvedActiveProviderId,
    apiKey: { kind: 'keep' } as AiSecretInput,
  }));
}

function buildConfigUpdate(
  aiConfig: NonNullable<AiConfigPanelProps['aiConfig']>,
  overrides?: {
    activeProviderId?: string | null;
    providers?: AiProviderUpsert[];
    copilotParams?: NonNullable<AiConfigPanelProps['aiConfig']>['copilotParams'];
    agentSettings?: AiAgentSettings;
  },
): AiConfigUpdate {
  const resolvedActiveProviderId =
    overrides?.activeProviderId ??
    aiConfig.activeProviderId ??
    aiConfig.providers[0]?.id ??
    null;

  return {
    version: aiConfig.version,
    providers:
      overrides?.providers ??
      buildProviderUpserts(aiConfig, resolvedActiveProviderId),
    activeProviderId: resolvedActiveProviderId ?? undefined,
    copilotParams: overrides?.copilotParams ?? aiConfig.copilotParams,
    agentSettings: overrides?.agentSettings ?? aiConfig.agentSettings,
  };
}

function toAgentSettingsForm(
  aiConfig: NonNullable<AiConfigPanelProps['aiConfig']> | null,
): AgentSettingsFormState {
  if (!aiConfig) {
    return EMPTY_AGENT_SETTINGS_FORM;
  }

  return {
    systemPrompt: aiConfig.agentSettings.systemPrompt ?? '',
    temperature: readNumberInput(aiConfig.copilotParams.temperature),
    maxTokens: readNumberInput(aiConfig.copilotParams.maxTokens),
    topP: readNumberInput(aiConfig.copilotParams.topP),
    timeoutMs: readNumberInput(aiConfig.agentSettings.timeoutMs),
  };
}

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
  const [agentSettingsForm, setAgentSettingsForm] = useState<AgentSettingsFormState>(
    EMPTY_AGENT_SETTINGS_FORM,
  );

  useEffect(() => {
    setAgentSettingsForm(toAgentSettingsForm(aiConfig));
  }, [aiConfig]);

  const activeProvider = useMemo(() => {
    if (!aiConfig?.activeProviderId) {
      return null;
    }

    return aiConfig.providers.find((provider) => provider.id === aiConfig.activeProviderId) ?? null;
  }, [aiConfig]);

  const isFormValid =
    form.name.trim().length > 0 &&
    form.baseUrl.trim().length > 0 &&
    form.apiKey.trim().length > 0 &&
    form.defaultModel.trim().length > 0;

  const isTemperatureValid =
    !agentSettingsForm.temperature.trim() ||
    parseOptionalFiniteNumber(agentSettingsForm.temperature) !== undefined;
  const isMaxTokensValid =
    !agentSettingsForm.maxTokens.trim() ||
    parseOptionalPositiveInteger(agentSettingsForm.maxTokens) !== undefined;
  const isTopPValid =
    !agentSettingsForm.topP.trim() ||
    parseOptionalFiniteNumber(agentSettingsForm.topP) !== undefined;
  const isTimeoutValid =
    !agentSettingsForm.timeoutMs.trim() ||
    parseOptionalPositiveBigInt(agentSettingsForm.timeoutMs) !== undefined;
  const isAgentSettingsValid =
    isTemperatureValid && isMaxTokensValid && isTopPValid && isTimeoutValid;

  const hasPendingAgentSettings =
    !!aiConfig &&
    JSON.stringify(toAgentSettingsForm(aiConfig)) !== JSON.stringify(agentSettingsForm);

  function handleFormChange(field: keyof ProviderFormState, value: string) {
    setForm((prev) => ({ ...prev, [field]: value }));
  }

  function handleAgentSettingsChange(
    field: keyof AgentSettingsFormState,
    value: string,
  ) {
    setAgentSettingsForm((prev) => ({ ...prev, [field]: value }));
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
    const draft: AiProviderDraft = {
      id: undefined,
      name: form.name,
      baseUrl: form.baseUrl,
      apiKey: form.apiKey,
      defaultModel: form.defaultModel,
      extraHeaders: {},
      enabled: true,
    };

    void onAiProviderTest(draft);
  }

  function handleTestSavedProvider(providerId: string) {
    if (!aiConfig) {
      return;
    }

    const provider = aiConfig.providers.find((item) => item.id === providerId);
    if (!provider) {
      return;
    }

    void onAiProviderTest({
      id: provider.id,
      name: provider.name,
      baseUrl: provider.baseUrl,
      apiKey: undefined,
      defaultModel: provider.defaultModel,
      extraHeaders: provider.extraHeaders,
      enabled: provider.enabled,
    });
  }

  function handleConfirmAdd() {
    if (!aiConfig) return;

    const newId = crypto.randomUUID();
    const nextActiveProviderId =
      aiConfig.activeProviderId ?? aiConfig.providers[0]?.id ?? newId;

    const existingUpserts = buildProviderUpserts(aiConfig, nextActiveProviderId);
    const newProvider: AiProviderUpsert = {
      id: newId,
      name: form.name.trim(),
      baseUrl: form.baseUrl.trim(),
      defaultModel: form.defaultModel.trim(),
      extraHeaders: {},
      enabled: newId === nextActiveProviderId,
      apiKey: { kind: 'set', value: form.apiKey.trim() } as AiSecretInput,
    };

    void onAiConfigSave(
      buildConfigUpdate(aiConfig, {
        activeProviderId: nextActiveProviderId,
        providers: [...existingUpserts, newProvider],
      }),
    );
    setForm(EMPTY_FORM);
    setShowForm(false);
  }

  function handleSetGlobalProvider(providerId: string) {
    if (!aiConfig) {
      return;
    }

    void onAiConfigSave(
      buildConfigUpdate(aiConfig, {
        activeProviderId: providerId,
      }),
    );
  }

  function handleSaveAgentSettings() {
    if (!aiConfig || !isAgentSettingsValid) {
      return;
    }

    void onAiConfigSave(
      buildConfigUpdate(aiConfig, {
        copilotParams: {
          temperature: parseOptionalFiniteNumber(agentSettingsForm.temperature),
          maxTokens: parseOptionalPositiveInteger(agentSettingsForm.maxTokens),
          topP: parseOptionalFiniteNumber(agentSettingsForm.topP),
        },
        agentSettings: {
          systemPrompt: agentSettingsForm.systemPrompt.trim() || undefined,
          timeoutMs: parseOptionalPositiveBigInt(agentSettingsForm.timeoutMs),
        },
      }),
    );
  }

  function handleResetForm() {
    setForm(EMPTY_FORM);
    setShowForm(false);
  }

  function handleResetAgentSettings() {
    setAgentSettingsForm(toAgentSettingsForm(aiConfig));
  }

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

              {aiConfigError ? (
                <article className="settings-row">
                  <span className="settings-row__label" style={{ color: 'var(--color-error)' }}>
                    {aiConfigError}
                  </span>
                </article>
              ) : null}

              <article className="settings-row">
                <strong className="settings-row__label">全局生效 AI</strong>
                <span className="settings-row__value">
                  {activeProvider ? `${activeProvider.name} · ${activeProvider.defaultModel}` : '未配置'}
                </span>
              </article>
              <article className="settings-row">
                <strong className="settings-row__label">Provider 数量</strong>
                <span className="settings-row__value">{aiConfig.providers.length}</span>
              </article>
              <article className="settings-row">
                <span className="settings-row__label" style={{ color: 'var(--text-tertiary)' }}>
                  AI 配置页仅允许一个 provider 作为全局配置。Code Node 会默认使用这里选中的 AI。
                </span>
              </article>
            </section>

            {aiTestResult ? (
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
            ) : null}

            <section className="settings-group">
              <div className="settings-group__header">
                <h3>全局 AI</h3>
              </div>

              {aiConfig.providers.length === 0 ? (
                <article className="settings-row">
                  <span className="settings-row__label">尚未配置任何提供商。</span>
                </article>
              ) : (
                aiConfig.providers.map((provider) => {
                  const isGlobalProvider = provider.id === activeProvider?.id;
                  return (
                    <article key={provider.id} className="settings-row settings-row--stacked">
                      <strong className="settings-row__label">
                        {provider.name}
                        {isGlobalProvider ? ' · 全局生效' : ' · 待命'}
                      </strong>
                      <span className="settings-row__value">{provider.baseUrl}</span>
                      <span className="settings-row__value">
                        默认模型：{provider.defaultModel}
                      </span>
                      <span className="settings-row__value">
                        API Key：{provider.hasApiKey ? '已配置' : '未配置'}
                      </span>
                      <div className="settings-path-actions">
                        <button
                          type="button"
                          className="settings-inline-button"
                          disabled={isGlobalProvider}
                          onClick={() => handleSetGlobalProvider(provider.id)}
                        >
                          {isGlobalProvider ? '当前全局 AI' : '设为全局 AI'}
                        </button>
                        <button
                          type="button"
                          className="settings-inline-button settings-inline-button--ghost"
                          disabled={aiTesting}
                          onClick={() => handleTestSavedProvider(provider.id)}
                        >
                          {aiTesting ? '测试中...' : '测试连接'}
                        </button>
                      </div>
                    </article>
                  );
                })
              )}
            </section>

            <section className="settings-group">
              <div className="settings-group__header">
                <h3>全局 Agent 参数</h3>
              </div>
              <article className="settings-row">
                <span className="settings-row__label" style={{ color: 'var(--text-tertiary)' }}>
                  `code/rhai` 节点调用 `ai_complete(prompt)` 时默认使用这里的系统提示词、采样参数和超时设置。
                </span>
              </article>

              <article className="settings-row settings-row--stacked">
                <label className="settings-row__label" htmlFor="ai-agent-system-prompt">
                  System Prompt
                </label>
                <textarea
                  id="ai-agent-system-prompt"
                  className="settings-path-input"
                  placeholder="可选：全局约束 code node 的 AI 输出风格"
                  value={agentSettingsForm.systemPrompt}
                  onChange={(event) =>
                    handleAgentSettingsChange('systemPrompt', event.target.value)
                  }
                />
              </article>

              <article className="settings-row settings-row--stacked">
                <label className="settings-row__label" htmlFor="ai-agent-temperature">
                  Temperature
                </label>
                <input
                  id="ai-agent-temperature"
                  className="settings-path-input"
                  type="text"
                  placeholder="留空使用默认值"
                  value={agentSettingsForm.temperature}
                  onChange={(event) =>
                    handleAgentSettingsChange('temperature', event.target.value)
                  }
                />
              </article>

              <article className="settings-row settings-row--stacked">
                <label className="settings-row__label" htmlFor="ai-agent-max-tokens">
                  Max Tokens
                </label>
                <input
                  id="ai-agent-max-tokens"
                  className="settings-path-input"
                  type="text"
                  placeholder="留空使用默认值"
                  value={agentSettingsForm.maxTokens}
                  onChange={(event) =>
                    handleAgentSettingsChange('maxTokens', event.target.value)
                  }
                />
              </article>

              <article className="settings-row settings-row--stacked">
                <label className="settings-row__label" htmlFor="ai-agent-top-p">
                  Top P
                </label>
                <input
                  id="ai-agent-top-p"
                  className="settings-path-input"
                  type="text"
                  placeholder="留空使用默认值"
                  value={agentSettingsForm.topP}
                  onChange={(event) => handleAgentSettingsChange('topP', event.target.value)}
                />
              </article>

              <article className="settings-row settings-row--stacked">
                <label className="settings-row__label" htmlFor="ai-agent-timeout">
                  Agent 超时 ms
                </label>
                <input
                  id="ai-agent-timeout"
                  className="settings-path-input"
                  type="text"
                  placeholder="留空使用运行时默认值"
                  value={agentSettingsForm.timeoutMs}
                  onChange={(event) =>
                    handleAgentSettingsChange('timeoutMs', event.target.value)
                  }
                />
              </article>

              {!isAgentSettingsValid ? (
                <article className="settings-row">
                  <span className="settings-row__label" style={{ color: 'var(--color-error)' }}>
                    参数格式无效：Temperature / Top P 需要是数字，Max Tokens / 超时需要是大于 0 的整数。
                  </span>
                </article>
              ) : null}

              <article className="settings-row">
                <div className="settings-path-actions">
                  <button
                    type="button"
                    className="settings-inline-button"
                    disabled={!hasPendingAgentSettings || !isAgentSettingsValid}
                    onClick={handleSaveAgentSettings}
                  >
                    保存全局参数
                  </button>
                  <button
                    type="button"
                    className="settings-inline-button settings-inline-button--ghost"
                    disabled={!hasPendingAgentSettings}
                    onClick={handleResetAgentSettings}
                  >
                    还原
                  </button>
                </div>
              </article>
            </section>

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
                    <span className="settings-row__label" style={{ color: 'var(--text-tertiary)' }}>
                      如果当前已经有全局 AI，新 provider 会先作为待命配置保存；如需切换默认 AI，请在上方“全局 AI”里选择。
                    </span>
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
