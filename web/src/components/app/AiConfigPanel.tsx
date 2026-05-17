/**
 * AI 配置面板——主组件。
 *
 * 管理全局状态（表单展开、编辑中提供商、Agent 参数），
 * 通过 AiConfigPreview / AiProviderForm 子组件渲染 UI，
 * 通过 ExpandTransition 统一切换覆盖层。
 */

import { useEffect, useMemo, useState } from 'react';

import { SwitchBar } from '../flowgram/nodes/settings-shared';

import type { AiConfigPanelProps } from './types';
import type { AiProviderDraft } from '../../types';
import {
  EMPTY_PROVIDER_FORM,
  hasPendingProviderChanges,
  resolveProviderApiKeyMode,
  toProviderFormState,
  type ProviderFormState,
} from '../../lib/ai-config';
import {
  type AgentSettingsFormState,
  type ProviderPreset,
  EMPTY_AGENT_SETTINGS_FORM,
  buildConfigUpdate,
  buildProviderUpserts,
  parseOptionalFiniteNumber,
  parseOptionalPositiveInteger,
  toAgentSettingsForm,
} from './ai-config-utils';
import {
  ResetIcon,
  SaveIcon,
  XCloseIcon,
} from './AppIcons';
import { AiConfigPreview } from './AiConfigPreview';
import { AiProviderForm } from './AiProviderForm';
import { ExpandTransition } from './ExpandTransition';

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
  // ── Provider 表单状态 ──────────────────────────────────────────
  const [showForm, setShowForm] = useState(false);
  const [form, setForm] = useState<ProviderFormState>(EMPTY_PROVIDER_FORM);
  const [editingProviderId, setEditingProviderId] = useState<string | null>(null);
  const [clearSavedApiKey, setClearSavedApiKey] = useState(false);

  // ── Agent 参数对话框状态 ───────────────────────────────────────
  const [agentSettingsForm, setAgentSettingsForm] = useState<AgentSettingsFormState>(
    EMPTY_AGENT_SETTINGS_FORM,
  );
  const [showAgentDialog, setShowAgentDialog] = useState(false);

  // aiConfig 变化时同步 Agent 表单
  useEffect(() => {
    setAgentSettingsForm(toAgentSettingsForm(aiConfig));
  }, [aiConfig]);

  // ── 派生状态 ──────────────────────────────────────────────────
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

  // ── Agent 参数校验 ─────────────────────────────────────────────
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
    parseOptionalPositiveInteger(agentSettingsForm.timeoutMs) !== undefined;
  const isAgentSettingsValid =
    isTemperatureValid &&
    isMaxTokensValid &&
    isTopPValid &&
    isTimeoutValid;

  const hasPendingAgentSettings =
    !!aiConfig &&
    JSON.stringify(toAgentSettingsForm(aiConfig)) !== JSON.stringify(agentSettingsForm);

  // ── Provider 表单回调 ──────────────────────────────────────────

  function handleFormChange(field: keyof ProviderFormState, value: string) {
    if (field === 'apiKey') {
      setClearSavedApiKey(false);
    }
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

  function handleResetForm() {
    setForm(EMPTY_PROVIDER_FORM);
    setEditingProviderId(null);
    setClearSavedApiKey(false);
    setShowForm(false);
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

  // ── Agent 参数回调 ─────────────────────────────────────────────

  function handleAgentSettingsChange(
    field: keyof AgentSettingsFormState,
    value: string,
  ) {
    setAgentSettingsForm((prev) => ({ ...prev, [field]: value }));
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
          timeoutMs: parseOptionalPositiveInteger(agentSettingsForm.timeoutMs),
          thinkingEnabled: agentSettingsForm.thinkingEnabled,
          toolCallingEnabled: agentSettingsForm.toolCallingEnabled,
        },
      }),
    );
  }

  function handleResetAgentSettings() {
    setAgentSettingsForm(toAgentSettingsForm(aiConfig));
  }

  // ── Agent 参数对话框覆盖层 ─────────────────────────────────────

  const agentDialogOverlay = (
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

          <article className="settings-row">
            <label className="settings-row__label" htmlFor="ai-agent-thinking">
              启用 Thinking（深度思考）
            </label>
            <SwitchBar
              checked={agentSettingsForm.thinkingEnabled}
              onChange={(value) =>
                setAgentSettingsForm((prev) => ({
                  ...prev,
                  thinkingEnabled: value,
                }))
              }
            />
          </article>

          <article className="settings-row">
            <label className="settings-row__label" htmlFor="ai-agent-tool-calling">
              启用工具调用（Copilot）
            </label>
            <SwitchBar
              checked={agentSettingsForm.toolCallingEnabled}
              onChange={(value) =>
                setAgentSettingsForm((prev) => ({
                  ...prev,
                  toolCallingEnabled: value,
                }))
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
  );

  // ── 渲染 ──────────────────────────────────────────────────────

  // Provider 表单需要 aiConfig 非空才渲染
  const providerFormOverlay = aiConfig ? (
    <AiProviderForm
      aiConfig={aiConfig}
      editingProvider={editingProvider}
      editingProviderId={editingProviderId}
      form={form}
      aiTesting={aiTesting}
      aiTestResult={aiTestResult}
      clearSavedApiKey={clearSavedApiKey}
      onAiConfigSave={onAiConfigSave}
      onAiProviderTest={onAiProviderTest}
      onFormChange={handleFormChange}
      onSelectPreset={handleSelectPreset}
      onResetForm={handleResetForm}
      onSetClearSavedApiKey={setClearSavedApiKey}
      onSetForm={setForm}
    />
  ) : null;

  return (
    <div className="ai-config-panel">
      <ExpandTransition
        active={showForm || showAgentDialog}
        mode="centered"
        base={
          <AiConfigPreview
            isTauriRuntime={isTauriRuntime}
            aiConfig={aiConfig}
            aiConfigLoading={aiConfigLoading}
            aiConfigError={aiConfigError}
            aiTestResult={aiTestResult}
            aiTesting={aiTesting}
            activeProvider={activeProvider}
            onSetGlobalProvider={handleSetGlobalProvider}
            onTestSavedProvider={handleTestSavedProvider}
            onStartEditProvider={handleStartEditProvider}
            onDeleteProvider={handleDeleteProvider}
            onOpenAgentDialog={() => setShowAgentDialog(true)}
            onStartAddProvider={handleStartAddProvider}
          />
        }
        overlay={showForm ? providerFormOverlay : showAgentDialog ? agentDialogOverlay : null}
      />
    </div>
  );
}
