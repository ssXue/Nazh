/**
 * AI 配置面板——共享类型、常量与纯函数工具。
 *
 * 从 AiConfigPanel 中抽取出的无 React 依赖的工具代码，
 * 供主面板和子组件共同使用。
 */

import type {
  AiAgentSettings,
  AiConfigUpdate,
  AiProviderUpsert,
  AiSecretInput,
} from '../../types';
import { resolveProviderApiKeyInput } from '../../lib/ai-config';
import type { AiConfigPanelProps } from './types';

// ── Agent 参数表单状态 ──────────────────────────────────────────────

export interface AgentSettingsFormState {
  systemPrompt: string;
  temperature: string;
  maxTokens: string;
  topP: string;
  timeoutMs: string;
  thinkingEnabled: boolean;
  toolCallingEnabled: boolean;
}

export const EMPTY_AGENT_SETTINGS_FORM: AgentSettingsFormState = {
  systemPrompt: '',
  temperature: '',
  maxTokens: '',
  topP: '',
  timeoutMs: '',
  thinkingEnabled: false,
  toolCallingEnabled: false,
};

// ── 提供商预设 ────────────────────────────────────────────────────

export interface ProviderPreset {
  label: string;
  name: string;
  baseUrl: string;
  defaultModel: string;
}

export const PROVIDER_PRESETS: ProviderPreset[] = [
  { label: 'DeepSeek Flash', name: 'DeepSeek', baseUrl: 'https://api.deepseek.com', defaultModel: 'deepseek-v4-flash' },
  { label: 'DeepSeek Pro', name: 'DeepSeek', baseUrl: 'https://api.deepseek.com', defaultModel: 'deepseek-v4-pro' },
  { label: 'OpenAI', name: 'OpenAI', baseUrl: 'https://api.openai.com/v1', defaultModel: 'gpt-4o-mini' },
  { label: 'Kimi Code', name: 'Kimi Code', baseUrl: 'https://api.kimi.com/coding/v1', defaultModel: 'kimi-for-coding' },
  { label: '月之暗面', name: 'Moonshot', baseUrl: 'https://api.moonshot.cn/v1', defaultModel: 'moonshot-v1-8k' },
  { label: '智谱', name: 'Zhipu', baseUrl: 'https://open.bigmodel.cn/api/paas/v4', defaultModel: 'glm-4-flash' },
  { label: '通义千问', name: 'Qwen', baseUrl: 'https://dashscope.aliyuncs.com/compatible-mode/v1', defaultModel: 'qwen-turbo' },
  { label: '硅基流动', name: 'SiliconFlow', baseUrl: 'https://api.siliconflow.cn/v1', defaultModel: 'Qwen/Qwen2.5-7B-Instruct' },
  { label: '阶跃星辰', name: 'StepFun', baseUrl: 'https://api.stepfun.com/v1', defaultModel: 'step-2-16k' },
  { label: 'Ollama 本地', name: 'Ollama', baseUrl: 'http://localhost:11434/v1', defaultModel: 'qwen2.5:7b' },
];

// ── 纯工具函数 ─────────────────────────────────────────────────────

/** 把后端返回的数值字段安全转为表单字符串。 */
export function readNumberInput(value: number | bigint | undefined | null): string {
  if (typeof value === 'bigint') {
    return value.toString();
  }

  return typeof value === 'number' && Number.isFinite(value) ? String(value) : '';
}

/** 解析可选有限小数。 */
export function parseOptionalFiniteNumber(value: string): number | undefined {
  const normalized = value.trim();
  if (!normalized) {
    return undefined;
  }

  const parsed = Number(normalized);
  return Number.isFinite(parsed) ? parsed : undefined;
}

/** 解析可选正整数。 */
export function parseOptionalPositiveInteger(value: string): number | undefined {
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

/** 把当前 aiConfig.providers 转为 AiProviderUpsert 列表（keep key 模式）。 */
export function buildProviderUpserts(
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

/** 构造完整的 AiConfigUpdate，支持局部覆盖。 */
export function buildConfigUpdate(
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

/** 从当前 aiConfig 映射 Agent 参数表单初始值。 */
export function toAgentSettingsForm(
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
    thinkingEnabled: aiConfig.agentSettings.thinkingEnabled,
    toolCallingEnabled: aiConfig.agentSettings.toolCallingEnabled,
  };
}
