/**
 * AI 配置面板——提供商添加/编辑表单子组件。
 *
 * 渲染侧边抽屉形式的表单，支持从预设快速选择厂商、填写
 * Provider 字段（名称 / URL / 模型 / Key），以及保存/测试/取消操作。
 */

import type { AiConfigPanelProps } from './types';
import type { AiProviderDraft, AiProviderUpsert } from '../../types';
import {
  EMPTY_PROVIDER_FORM,
  hasPendingProviderChanges,
  resolveProviderApiKeyInput,
  resolveProviderApiKeyMode,
  type ProviderFormState,
} from '../../lib/ai-config';
import {
  type ProviderPreset,
  PROVIDER_PRESETS,
  buildConfigUpdate,
  buildProviderUpserts,
} from './ai-config-utils';
import {
  SaveIcon,
  SignalIcon,
  XCloseIcon,
} from './AppIcons';

interface AiProviderFormProps {
  /** 当前完整 AI 配置。 */
  aiConfig: NonNullable<AiConfigPanelProps['aiConfig']>;
  /** 正在编辑的提供商（null 表示新增）。 */
  editingProvider: NonNullable<AiConfigPanelProps['aiConfig']>['providers'][number] | null;
  /** 正在编辑的 provider ID。 */
  editingProviderId: string | null;
  /** 表单字段值。 */
  form: ProviderFormState;
  /** 是否正在执行连接测试。 */
  aiTesting: boolean;
  /** 最近一次连接测试结果。 */
  aiTestResult: AiConfigPanelProps['aiTestResult'];
  /** 是否勾选了「清空已保存 Key」。 */
  clearSavedApiKey: boolean;
  /** 保存配置回调。 */
  onAiConfigSave: AiConfigPanelProps['onAiConfigSave'];
  /** 连接测试回调。 */
  onAiProviderTest: AiConfigPanelProps['onAiProviderTest'];
  /** 更新表单字段。 */
  onFormChange: (field: keyof ProviderFormState, value: string) => void;
  /** 选择预设厂商。 */
  onSelectPreset: (preset: ProviderPreset) => void;
  /** 重置表单 + 关闭抽屉。 */
  onResetForm: () => void;
  /** 设置 clearSavedApiKey 标记。 */
  onSetClearSavedApiKey: (value: boolean) => void;
  /** 直接设置 form（用于 API Key 模式切换时清空 key 字段）。 */
  onSetForm: (updater: (prev: ProviderFormState) => ProviderFormState) => void;
}

export function AiProviderForm({
  aiConfig,
  editingProvider,
  editingProviderId,
  form,
  aiTesting,
  aiTestResult,
  clearSavedApiKey,
  onAiConfigSave,
  onAiProviderTest,
  onFormChange,
  onSelectPreset,
  onResetForm,
  onSetClearSavedApiKey,
  onSetForm,
}: AiProviderFormProps) {
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

  function handleConfirmAdd() {
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
    onResetForm();
  }

  return (
    <div className="ai-drawer" onClick={(e) => e.stopPropagation()}>
      <div className="ai-drawer__header">
        <h3>{isEditingProvider ? '编辑提供商' : '添加提供商'}</h3>
        <button
          type="button"
          className="ai-drawer__close"
          onClick={onResetForm}
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
                  data-testid="ai-provider-preset"
                  onClick={() => onSelectPreset(preset)}
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
              onChange={(e) => onFormChange('name', e.target.value)}
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
              onChange={(e) => onFormChange('baseUrl', e.target.value)}
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
              onChange={(e) => onFormChange('defaultModel', e.target.value)}
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
                      onSetClearSavedApiKey(false);
                      onSetForm((prev) => ({ ...prev, apiKey: '' }));
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
                      onSetClearSavedApiKey(true);
                      onSetForm((prev) => ({ ...prev, apiKey: '' }));
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
              onChange={(e) => onFormChange('apiKey', e.target.value)}
            />
          </article>
        </div>

        <p className="ai-config-panel__hint">
          {isEditingProvider
            ? '编辑现有 provider 时会保留其全局启用状态；如需切换默认 AI，请在上方"全局 AI"里选择。'
            : '如果当前已经有全局 AI，新 provider 会先作为待命配置保存；如需切换默认 AI，请在上方"全局 AI"里选择。'}
        </p>
      </div>

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

      <div className="ai-drawer__footer">
        <div className="settings-path-actions">
          <button
            type="button"
            className="settings-inline-button"
            disabled={!isFormValid || (isEditingProvider && !hasPendingProviderEdits)}
            data-testid="ai-provider-save"
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
            onClick={onResetForm}
          >
            <XCloseIcon className="ai-btn-icon" />
            取消
          </button>
        </div>
      </div>
    </div>
  );
}
