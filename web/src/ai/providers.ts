/// AI Provider 注册表。
///
/// 基于 Vercel AI SDK 的 provider 工厂，统一管理 OpenAI 兼容 provider。
/// 所有 provider（DeepSeek / Moonshot / OpenAI / Ollama 等）都通过
/// `@ai-sdk/openai-compatible` 接入。
///
/// 使用 openai-compatible 而非 openai 的原因：
/// DeepSeek 等模型的 `reasoning_content` 字段在多轮工具调用时必须回传，
/// `@ai-sdk/openai` 不处理此字段，而 `@ai-sdk/openai-compatible` 原生支持
/// reasoning_content 的提取和回传。

import { createOpenAICompatible } from '@ai-sdk/openai-compatible';
import type { LanguageModel } from 'ai';

import type { AiProviderView } from '../types';
import { loadApiKey } from './api-key';

export interface CreateModelOptions {
  provider: AiProviderView;
  modelOverride?: string;
}

/// 创建 AI SDK LanguageModel 实例。
///
/// 按需从 Rust 加密存储读取 API key，不缓存到全局。
/// 调用方应在每次 AI 调用时重新创建 model 实例。
export async function createLanguageModel(
  options: CreateModelOptions,
): Promise<LanguageModel> {
  const { provider, modelOverride } = options;

  const apiKey = await loadApiKey(provider.id);
  if (!apiKey.trim()) {
    throw new Error(`AI 提供商「${provider.name}」未配置 API key，请在设置中配置`);
  }

  const modelId = (modelOverride ?? provider.defaultModel).trim();
  if (!modelId) {
    throw new Error(`AI 提供商「${provider.name}」未配置默认模型，请在设置中配置`);
  }

  const compatible = createOpenAICompatible({
    name: provider.id,
    baseURL: normalizeBaseUrl(provider.baseUrl),
    apiKey,
    headers: buildHeaders(provider),
  });

  return compatible(modelId);
}

/// 将用户输入的 base URL 规范化为 SDK 期望的格式。
///
/// - 去除尾部 `/`
/// - 保留用户自定义的 path 前缀（如 `/v1`），不自动追加
function normalizeBaseUrl(url: string): string {
  const trimmed = url.trim();
  if (!trimmed) {
    throw new Error('AI 提供商 base URL 为空');
  }
  return trimmed.replace(/\/+$/, '');
}

/// 构建自定义请求头。
function buildHeaders(provider: AiProviderView): Record<string, string> | undefined {
  const entries = Object.entries(provider.extraHeaders);
  if (entries.length === 0) {
    return undefined;
  }
  return Object.fromEntries(entries);
}
