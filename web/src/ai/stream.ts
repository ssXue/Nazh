/// AI 流式调用核心模块。
///
/// 基于 Vercel AI SDK 的 `streamText`，提供统一的流式 AI 调用能力。
/// 替代原有的 Tauri event listener 中间层，前端直接与 AI provider 通信。

import { streamText } from 'ai';
import type { LanguageModel } from 'ai';

import type { AiGenerationParams } from '../types';

const DEBUG_STREAM = true;

function streamLog(...args: unknown[]) {
  if (DEBUG_STREAM) console.log('[ai/stream]', ...args);
}

function streamWarn(...args: unknown[]) {
  if (DEBUG_STREAM) console.warn('[ai/stream]', ...args);
}

export interface StreamCallbacks {
  onDelta?: (accumulated: string) => void;
  onThinking?: (accumulated: string) => void;
}

export interface StreamResult {
  text: string;
  finishReason?: string;
  aborted?: boolean;
}

export interface AiStreamOptions {
  model: LanguageModel;
  messages: Array<{ role: 'system' | 'user' | 'assistant'; content: string }>;
  params?: AiGenerationParams;
  signal?: AbortSignal;
  callbacks?: StreamCallbacks;
}

/// 执行流式 AI 调用。
///
/// 使用 ai-sdk 的 `streamText` 直接与 provider 通信，
/// 通过回调逐 token 更新 UI。
export async function aiStreamText(options: AiStreamOptions): Promise<StreamResult> {
  const { model, messages, params, signal, callbacks } = options;

  streamLog('流开始', { msgCount: messages.length });

  const result = streamText({
    model,
    messages,
    maxOutputTokens: params?.maxTokens,
    temperature: params?.temperature,
    topP: params?.topP,
    abortSignal: signal,
  });

  let accumulated = '';
  let thinkingAccumulated = '';

  try {
    for await (const part of result.fullStream) {
      if (signal?.aborted) {
        streamLog('abort 检测到，中断流');
        break;
      }

      switch (part.type) {
        case 'text-delta': {
          accumulated += part.text;
          callbacks?.onDelta?.(accumulated);
          break;
        }
        case 'reasoning-delta': {
          thinkingAccumulated += part.text;
          callbacks?.onThinking?.(thinkingAccumulated);
          break;
        }
        case 'error': {
          streamWarn('stream error', { error: part.error });
          throw new Error(String(part.error));
        }
        default:
          break;
      }
    }
  } catch (error) {
    if (signal?.aborted) {
      streamLog('流被 abort', { accLen: accumulated.length });
      return { text: accumulated, aborted: true };
    }
    throw error;
  }

  const finishReason = await result.finishReason;
  streamLog('流结束', { accLen: accumulated.length, finishReason });

  return {
    text: accumulated,
    finishReason,
    aborted: signal?.aborted ?? false,
  };
}
