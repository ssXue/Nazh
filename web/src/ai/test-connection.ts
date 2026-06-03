/// AI 提供商连接测试。
///
/// 使用 Vercel AI SDK 直接向提供商发送简单请求验证连通性。
/// 本地部署 provider（Ollama 等）不需要 API key。

import { createOpenAICompatible } from '@ai-sdk/openai-compatible';
import { generateText } from 'ai';

import { isLocalProvider } from './providers';
import type { AiProviderDraft } from '../types';

export interface ConnectionTestResult {
  success: boolean;
  message: string;
  latencyMs?: number;
}

/// 测试 AI 提供商连接。
///
/// 向提供商发送一条简短消息，验证连通性。
/// 本地 provider（localhost）不需要 API key。
export async function testProviderConnection(
  draft: AiProviderDraft,
): Promise<ConnectionTestResult> {
  const apiKey = draft.apiKey?.trim();
  const baseUrl = draft.baseUrl.trim().replace(/\/+$/, '');
  const model = draft.defaultModel.trim();
  const local = isLocalProvider(baseUrl);

  if (!local && !apiKey) {
    return {
      success: false,
      message: '测试连接需要提供 API Key',
    };
  }

  if (!baseUrl) {
    return {
      success: false,
      message: 'Base URL 为空',
    };
  }

  if (!model) {
    return {
      success: false,
      message: '默认模型为空',
    };
  }

  const openai = createOpenAICompatible({ name: 'test', baseURL: baseUrl, apiKey: local ? 'no-key' : apiKey! });
  const startedAt = performance.now();

  try {
    await generateText({
      model: openai(model),
      prompt: 'Hi',
      maxOutputTokens: 5,
    });

    const latencyMs = Math.round(performance.now() - startedAt);
    return {
      success: true,
      message: `连接成功（模型 ${model}，延迟 ${latencyMs} ms）`,
      latencyMs,
    };
  } catch (error) {
    const latencyMs = Math.round(performance.now() - startedAt);
    const message =
      error instanceof Error ? error.message : String(error);
    return {
      success: false,
      message,
      latencyMs,
    };
  }
}
