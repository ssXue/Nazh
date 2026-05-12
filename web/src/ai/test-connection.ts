/// AI 提供商连接测试。
///
/// 使用 Vercel AI SDK 直接向提供商发送简单请求验证连通性。

import { createOpenAICompatible } from '@ai-sdk/openai-compatible';
import { generateText } from 'ai';

import type { AiProviderDraft } from '../types';

export interface ConnectionTestResult {
  success: boolean;
  message: string;
  latencyMs?: number;
}

/// 测试 AI 提供商连接。
///
/// 向提供商发送一条简短消息，验证 API key、base URL 和模型是否可用。
/// 草稿中未提供 API key 时直接返回失败。
export async function testProviderConnection(
  draft: AiProviderDraft,
): Promise<ConnectionTestResult> {
  const apiKey = draft.apiKey?.trim();
  if (!apiKey) {
    return {
      success: false,
      message: '测试连接需要提供 API Key',
    };
  }

  const baseUrl = draft.baseUrl.trim().replace(/\/+$/, '');
  if (!baseUrl) {
    return {
      success: false,
      message: 'Base URL 为空',
    };
  }

  const model = draft.defaultModel.trim();
  if (!model) {
    return {
      success: false,
      message: '默认模型为空',
    };
  }

  const openai = createOpenAICompatible({ name: 'test', baseURL: baseUrl, apiKey });
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
