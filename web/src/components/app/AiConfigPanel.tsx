import { useEffect, useMemo, useState } from 'react';

import type { AiConfigPanelProps } from './types';
import type {
  AiAgentSettings,
  AiConfigUpdate,
  AiProviderDraft,
  AiProviderUpsert,
  AiSecretInput,
} from '../../types';
import {
  EMPTY_PROVIDER_FORM,
  hasPendingProviderChanges,
  resolveProviderApiKeyInput,
  resolveProviderApiKeyMode,
  toProviderFormState,
  type ProviderFormState,
} from '../../lib/ai-config';
import {
  CheckCircleIcon,
  DeleteActionIcon,
  PencilIcon,
  PlusIcon,
  ResetIcon,
  SaveIcon,
  SignalIcon,
  SlidersIcon,
  XCloseIcon,
} from './AppIcons';

type ThinkingModeValue = 'enabled' | 'disabled';
type ReasoningEffortValue = 'high' | 'max';

interface AgentSettingsFormState {
  systemPrompt: string;
  temperature: string;
  maxTokens: string;
  topP: string;
  thinkingMode: string;
  reasoningEffort: string;
  timeoutMs: string;
}

const EMPTY_AGENT_SETTINGS_FORM: AgentSettingsFormState = {
  systemPrompt: '',
  temperature: '',
  maxTokens: '',
  topP: '',
  thinkingMode: '',
  reasoningEffort: '',
  timeoutMs: '',
};

interface ProviderPreset {
  label: string;
  name: string;
  baseUrl: string;
  defaultModel: string;
}

const PROVIDER_PRESETS: ProviderPreset[] = [
  { label: 'DeepSeek Flash', name: 'DeepSeek', baseUrl: 'https://api.deepseek.com', defaultModel: 'deepseek-v4-flash' },
  { label: 'DeepSeek Pro', name: 'DeepSeek', baseUrl: 'https://api.deepseek.com', defaultModel: 'deepseek-v4-pro' },
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

function isThinkingModeValue(value: string): value is ThinkingModeValue {
  return value === 'enabled' || value === 'disabled';
}

function isReasoningEffortValue(value: string): value is ReasoningEffortValue {
  return value === 'high' || value === 'max';
}

function parseOptionalThinkingConfig(value: string) {
  const normalized = value.trim();
  return isThinkingModeValue(normalized) ? { type: normalized } : undefined;
}

function parseOptionalReasoningEffort(value: string): ReasoningEffortValue | undefined {
  const normalized = value.trim();
  return isReasoningEffortValue(normalized) ? normalized : undefined;
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
    thinkingMode: aiConfig.copilotParams.thinking?.type ?? '',
    reasoningEffort: aiConfig.copilotParams.reasoningEffort ?? '',
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
  const [form, setForm] = useState<ProviderFormState>(EMPTY_PROVIDER_FORM);
  const [editingProviderId, setEditingProviderId] = useState<string | null>(null);
  const [clearSavedApiKey, setClearSavedApiKey] = useState(false);
  const [agentSettingsForm, setAgentSettingsForm] = useState<AgentSettingsFormState>(
    EMPTY_AGENT_SETTINGS_FORM,
  );
  const [showAgentDialog, setShowAgentDialog] = useState(false);

  useEffect(() => {
    setAgentSettingsForm(toAgentSettingsForm(aiConfig));
  }, [aiConfig]);

  const activeProvider = useMemo(() => {
    if (!aiConfig?.activeProviderId) {
      return null;
    }

    return aiConfig.providers.find((provider) => provider.id === aiConfig.activeProviderId) ?? null;
  }, [aiConfig]);

  const editingProvider = useMemo(() => {
    if (!aiConfig || !editingProviderId) {
      return null;
    }

    return aiConfig.providers.find((provider) => provider.id === editingProviderId) ?? null;
  }, [aiConfig, editingProviderId]);

  const isEditingProvider = editingProvider !== null;
  const providerApiKeyMode = resolveProviderApiKeyMode(
    form.apiKey,
    editingProviderId,
    clearSavedApiKey,
  );

  const isFormValid =
    form.name.trim().length > 0 &&
    form.baseUrl.trim().length > 0 &&
    (isEditingProvider || form.apiKey.trim().length > 0) &&
    form.defaultModel.trim().length > 0;
  const canTestFormProvider =
    isFormValid && (!isEditingProvider || providerApiKeyMode !== 'clear');
  const hasPendingProviderEdits = hasPendingProviderChanges(
    editingProvider,
    form,
    editingProviderId,
    clearSavedApiKey,
  );

  const isTemperatureValid =
    !agentSettingsForm.temperature.trim() ||
    parseOptionalFiniteNumber(agentSettingsForm.temperature) !== undefined;
  const isMaxTokensValid =
    !agentSettingsForm.maxTokens.trim() ||
    parseOptionalPositiveInteger(agentSettingsForm.maxTokens) !== undefined;
  const isTopPValid =
    !agentSettingsForm.topP.trim() ||
    parseOptionalFiniteNumber(agentSettingsForm.topP) !== undefined;
  const isThinkingModeValid =
    agentSettingsForm.thinkingMode === '' ||
    isThinkingModeValue(agentSettingsForm.thinkingMode);
  const isReasoningEffortValid =
    agentSettingsForm.reasoningEffort === '' ||
    isReasoningEffortValue(agentSettingsForm.reasoningEffort);
  const isTimeoutValid =
    !agentSettingsForm.timeoutMs.trim() ||
    parseOptionalPositiveInteger(agentSettingsForm.timeoutMs) !== undefined;
  const isAgentSettingsValid =
    isTemperatureValid &&
    isMaxTokensValid &&
    isTopPValid &&
    isThinkingModeValid &&
    isReasoningEffortValid &&
    isTimeoutValid;

  const hasPendingAgentSettings =
    !!aiConfig &&
    JSON.stringify(toAgentSettingsForm(aiConfig)) !== JSON.stringify(agentSettingsForm);

  function handleFormChange(field: keyof ProviderFormState, value: string) {
    if (field === 'apiKey') {
      setClearSavedApiKey(false);
    }
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
    const trimmedApiKey = form.apiKey.trim();
    const draft: AiProviderDraft = {
      id:
        isEditingProvider && providerApiKeyMode === 'keep'
          ? editingProvider.id
          : undefined,
      name: form.name.trim(),
      baseUrl: form.baseUrl.trim(),
      apiKey: trimmedApiKey || undefined,
      defaultModel: form.defaultModel.trim(),
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

  function handleStartAddProvider() {
    setEditingProviderId(null);
    setClearSavedApiKey(false);
    setForm(EMPTY_PROVIDER_FORM);
    setShowForm(true);
  }

  function handleStartEditProvider(providerId: string) {
    if (!aiConfig) {
      return;
    }

    const provider = aiConfig.providers.find((item) => item.id === providerId);
    if (!provider) {
      return;
    }

    setEditingProviderId(provider.id);
    setClearSavedApiKey(false);
    setForm(toProviderFormState(provider));
    setShowForm(true);
  }

  function handleConfirmAdd() {
    if (!aiConfig) return;

    const nextProviderId = editingProvider?.id ?? crypto.randomUUID();
    const nextActiveProviderId =
      aiConfig.activeProviderId ?? aiConfig.providers[0]?.id ?? nextProviderId;

    const existingUpserts = buildProviderUpserts(aiConfig, nextActiveProviderId);
    const nextProvider: AiProviderUpsert = {
      id: nextProviderId,
      name: form.name.trim(),
      baseUrl: form.baseUrl.trim(),
      defaultModel: form.defaultModel.trim(),
      extraHeaders: editingProvider?.extraHeaders ?? {},
      enabled: nextProviderId === nextActiveProviderId,
      apiKey: resolveProviderApiKeyInput(form.apiKey, editingProviderId, clearSavedApiKey),
    };
    const nextProviders = isEditingProvider
      ? existingUpserts.map((provider) =>
          provider.id === nextProviderId ? nextProvider : provider,
        )
      : [...existingUpserts, nextProvider];

    void onAiConfigSave(
      buildConfigUpdate(aiConfig, {
        activeProviderId: nextActiveProviderId,
        providers: nextProviders,
      }),
    );
    setForm(EMPTY_PROVIDER_FORM);
    setEditingProviderId(null);
    setClearSavedApiKey(false);
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
          thinking: parseOptionalThinkingConfig(agentSettingsForm.thinkingMode),
          reasoningEffort: parseOptionalReasoningEffort(agentSettingsForm.reasoningEffort),
        },
        agentSettings: {
          systemPrompt: agentSettingsForm.systemPrompt.trim() || undefined,
          timeoutMs: parseOptionalPositiveInteger(agentSettingsForm.timeoutMs),
        },
      }),
    );
  }

  function handleResetForm() {
    setForm(EMPTY_PROVIDER_FORM);
    setEditingProviderId(null);
    setClearSavedApiKey(false);
    setShowForm(false);
  }

  function handleDeleteProvider(providerId: string) {
    if (!aiConfig) return;

    const remaining = aiConfig.providers.filter((p) => p.id !== providerId);
    const nextActiveId: string | null =
      aiConfig.activeProviderId === providerId
        ? remaining[0]?.id ?? null
        : aiConfig.activeProviderId ?? null;

    void onAiConfigSave(
      buildConfigUpdate(aiConfig, {
        activeProviderId: nextActiveId,
        providers: buildProviderUpserts(
          { ...aiConfig, providers: remaining },
          nextActiveId,
        ),
      }),
    );
  }

  function handleResetAgentSettings() {
    setAgentSettingsForm(toAgentSettingsForm(aiConfig));
  }

  return (
    <div className="ai-config-panel">
      <div
        className="panel__header panel__header--desktop window-safe-header"
        data-window-drag-region
      >
        <div className="ai-config-panel__header-info">
          <h2>AI 配置</h2>
          {activeProvider && (
            <span className="ai-config-panel__header-active">
              {activeProvider.name} · {activeProvider.defaultModel}
            </span>
          )}
        </div>
        <div className="ai-config-panel__header-actions" data-no-window-drag>
          <button
            type="button"
            className="ai-config-panel__action"
            onClick={() => setShowAgentDialog(true)}
          >
            <SlidersIcon />
            <span>Agent 参数</span>
          </button>
          <button
            type="button"
            className="ai-config-panel__action"
            onClick={handleStartAddProvider}
          >
            <PlusIcon />
            <span>添加连接</span>
          </button>
        </div>
      </div>

      <div className="ai-config-panel__scroll">
        <div className="settings-panel settings-panel--dense">
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
                <p className="ai-config-panel__hint">尚未配置任何提供商。</p>
              ) : (
                <div className="ai-config-panel__card-list">
                  {aiConfig.providers.map((provider) => {
                    const isGlobalProvider = provider.id === activeProvider?.id;
                    return (
                      <article
                        key={provider.id}
                        className={`ai-provider-card${isGlobalProvider ? ' ai-provider-card--active' : ''}`}
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
                            onClick={() => handleSetGlobalProvider(provider.id)}
                          >
                            <CheckCircleIcon className="ai-btn-icon" />
                            {isGlobalProvider ? '当前全局' : '设为全局'}
                          </button>
                          <button
                            type="button"
                            className="settings-inline-button settings-inline-button--ghost"
                            disabled={aiTesting}
                            onClick={() => handleTestSavedProvider(provider.id)}
                          >
                            <SignalIcon className="ai-btn-icon" />
                            {aiTesting ? '测试中...' : '测试'}
                          </button>
                          <button
                            type="button"
                            className="settings-inline-button settings-inline-button--ghost"
                            onClick={() => handleStartEditProvider(provider.id)}
                          >
                            <PencilIcon className="ai-btn-icon" />
                            编辑
                          </button>
                          <button
                            type="button"
                            className="settings-inline-button settings-inline-button--ghost settings-inline-button--danger"
                            onClick={() => handleDeleteProvider(provider.id)}
                          >
                            <DeleteActionIcon className="ai-btn-icon" />
                          </button>
                        </div>
                      </article>
                    );
                  })}
                </div>
              )}
            </div>
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
      </div>

      {showForm && (
        <div className="ai-drawer-overlay" onClick={handleResetForm}>
          <div className="ai-drawer" onClick={(e) => e.stopPropagation()}>
            <div className="ai-drawer__header">
              <h3>{isEditingProvider ? '编辑提供商' : '添加提供商'}</h3>
              <button
                type="button"
                className="ai-drawer__close"
                onClick={handleResetForm}
              >
                <XCloseIcon className="ai-btn-icon" />
              </button>
            </div>

            <div className="ai-drawer__body">
              <article className="ai-config-panel__notice">
                <strong>
                  {isEditingProvider ? `正在编辑：${editingProvider?.name}` : '正在新增提供商'}
                </strong>
                <span>
                  {isEditingProvider
                    ? '保存后会覆盖当前 provider 配置，现有全局启用状态会自动保留。'
                    : '保存后会追加到提供商列表中。'}
                </span>
              </article>

              <article className="settings-row settings-row--stacked">
                <strong className="settings-row__label">快速选择厂商</strong>
                <div className="settings-accent-inline" role="group" aria-label="预置厂商">
                  {PROVIDER_PRESETS.map((preset) => {
                    const isActive =
                      form.name === preset.name &&
                      form.baseUrl === preset.baseUrl &&
                      form.defaultModel === preset.defaultModel;
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

              <div className="ai-config-panel__field-grid">
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
                    placeholder="例如：https://api.deepseek.com"
                    value={form.baseUrl}
                    onChange={(e) => handleFormChange('baseUrl', e.target.value)}
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
                    placeholder="例如：deepseek-v4-flash"
                    value={form.defaultModel}
                    onChange={(e) => handleFormChange('defaultModel', e.target.value)}
                  />
                </article>

                <article className="settings-row settings-row--stacked ai-config-panel__field--wide">
                  <label className="settings-row__label" htmlFor="ai-provider-key">
                    API Key
                  </label>
                  {isEditingProvider ? (
                    <>
                      <div className="settings-accent-inline" role="group" aria-label="API Key 处理方式">
                        <button
                          type="button"
                          className={
                            providerApiKeyMode === 'keep'
                              ? 'settings-accent-chip is-active'
                              : 'settings-accent-chip'
                          }
                          onClick={() => {
                            setClearSavedApiKey(false);
                            setForm((prev) => ({ ...prev, apiKey: '' }));
                          }}
                        >
                          <span>保持现有 Key</span>
                        </button>
                        <button
                          type="button"
                          className={
                            providerApiKeyMode === 'clear'
                              ? 'settings-accent-chip is-active'
                              : 'settings-accent-chip'
                          }
                          onClick={() => {
                            setClearSavedApiKey(true);
                            setForm((prev) => ({ ...prev, apiKey: '' }));
                          }}
                        >
                          <span>清空已保存 Key</span>
                        </button>
                      </div>
                      <span className="settings-row__value" style={{ color: 'var(--text-tertiary)' }}>
                        {providerApiKeyMode === 'set'
                          ? '检测到新的 API Key，保存后会覆盖当前密钥。'
                          : providerApiKeyMode === 'clear'
                            ? '当前将清空已保存 API Key。若要替换成新密钥，直接在下方输入即可。'
                            : editingProvider?.hasApiKey
                              ? '当前已保存 API Key。留空会保持不变，输入新值会覆盖。'
                              : '当前未保存 API Key。可直接输入新的密钥。'}
                      </span>
                    </>
                  ) : null}
                  <input
                    id="ai-provider-key"
                    className="settings-path-input"
                    type="password"
                    placeholder={
                      isEditingProvider ? '留空保持当前值，输入新值则覆盖' : 'sk-...'
                    }
                    value={form.apiKey}
                    onChange={(e) => handleFormChange('apiKey', e.target.value)}
                  />
                </article>
              </div>

              <p className="ai-config-panel__hint">
                {isEditingProvider
                  ? '编辑现有 provider 时会保留其全局启用状态；如需切换默认 AI，请在上方"全局 AI"里选择。'
                  : '如果当前已经有全局 AI，新 provider 会先作为待命配置保存；如需切换默认 AI，请在上方"全局 AI"里选择。'}
              </p>
            </div>

            <div className="ai-drawer__footer">
              <div className="settings-path-actions">
                <button
                  type="button"
                  className="settings-inline-button"
                  disabled={!isFormValid || (isEditingProvider && !hasPendingProviderEdits)}
                  onClick={handleConfirmAdd}
                >
                  <SaveIcon className="ai-btn-icon" />
                  {isEditingProvider ? '保存修改' : '确认添加'}
                </button>
                <button
                  type="button"
                  className="settings-inline-button"
                  disabled={!canTestFormProvider || aiTesting}
                  onClick={handleSubmitTest}
                >
                  <SignalIcon className="ai-btn-icon" />
                  {aiTesting ? '测试中...' : '测试连接'}
                </button>
                <button
                  type="button"
                  className="settings-inline-button settings-inline-button--ghost"
                  onClick={handleResetForm}
                >
                  <XCloseIcon className="ai-btn-icon" />
                  取消
                </button>
              </div>
            </div>
          </div>
        </div>
      )}

      {showAgentDialog && (
        <div className="ai-drawer-overlay" onClick={() => setShowAgentDialog(false)}>
          <div className="ai-agent-dialog" onClick={(e) => e.stopPropagation()}>
            <div className="ai-drawer__header">
              <h3>全局 Agent 参数</h3>
              <button
                type="button"
                className="ai-drawer__close"
                onClick={() => setShowAgentDialog(false)}
              >
                <XCloseIcon className="ai-btn-icon" />
              </button>
            </div>

            <div className="ai-drawer__body">
              <p className="ai-config-panel__hint">
                `code` 节点调用 `ai_complete(prompt)` 时默认使用这里的系统提示词、采样参数和超时设置。
              </p>

              <div className="ai-agent-dialog__fields">
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
                  <strong className="settings-row__label">DeepSeek Thinking</strong>
                  <div className="settings-segment" role="group" aria-label="DeepSeek Thinking">
                    <button
                      type="button"
                      className={
                        agentSettingsForm.thinkingMode === ''
                          ? 'settings-segment__button is-active'
                          : 'settings-segment__button'
                      }
                      onClick={() => handleAgentSettingsChange('thinkingMode', '')}
                    >
                      默认
                    </button>
                    <button
                      type="button"
                      className={
                        agentSettingsForm.thinkingMode === 'enabled'
                          ? 'settings-segment__button is-active'
                          : 'settings-segment__button'
                      }
                      onClick={() => handleAgentSettingsChange('thinkingMode', 'enabled')}
                    >
                      开启
                    </button>
                    <button
                      type="button"
                      className={
                        agentSettingsForm.thinkingMode === 'disabled'
                          ? 'settings-segment__button is-active'
                          : 'settings-segment__button'
                      }
                      onClick={() => handleAgentSettingsChange('thinkingMode', 'disabled')}
                    >
                      关闭
                    </button>
                  </div>
                </article>

                <article className="settings-row settings-row--stacked">
                  <strong className="settings-row__label">Reasoning Effort</strong>
                  <div className="settings-segment" role="group" aria-label="Reasoning Effort">
                    <button
                      type="button"
                      className={
                        agentSettingsForm.reasoningEffort === ''
                          ? 'settings-segment__button is-active'
                          : 'settings-segment__button'
                      }
                      onClick={() => handleAgentSettingsChange('reasoningEffort', '')}
                    >
                      默认
                    </button>
                    <button
                      type="button"
                      className={
                        agentSettingsForm.reasoningEffort === 'high'
                          ? 'settings-segment__button is-active'
                          : 'settings-segment__button'
                      }
                      onClick={() => handleAgentSettingsChange('reasoningEffort', 'high')}
                    >
                      High
                    </button>
                    <button
                      type="button"
                      className={
                        agentSettingsForm.reasoningEffort === 'max'
                          ? 'settings-segment__button is-active'
                          : 'settings-segment__button'
                      }
                      onClick={() => handleAgentSettingsChange('reasoningEffort', 'max')}
                    >
                      Max
                    </button>
                  </div>
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
              </div>

              {!isAgentSettingsValid ? (
                <article className="ai-config-panel__notice ai-config-panel__notice--error">
                  <span style={{ color: 'var(--color-error)' }}>
                    参数格式无效：Temperature / Top P 需要是数字，Max Tokens / 超时需要是大于 0 的整数。
                  </span>
                </article>
              ) : null}
            </div>

            <div className="ai-drawer__footer">
              <div className="settings-path-actions">
                <button
                  type="button"
                  className="settings-inline-button"
                  disabled={!hasPendingAgentSettings || !isAgentSettingsValid}
                  onClick={handleSaveAgentSettings}
                >
                  <SaveIcon className="ai-btn-icon" />
                  保存参数
                </button>
                <button
                  type="button"
                  className="settings-inline-button settings-inline-button--ghost"
                  disabled={!hasPendingAgentSettings}
                  onClick={handleResetAgentSettings}
                >
                  <ResetIcon className="ai-btn-icon" />
                  还原
                </button>
              </div>
            </div>
          </div>
        </div>
      )}
    </div>
  );
}
