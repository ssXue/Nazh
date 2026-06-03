import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';

import type { AiConfigUpdate, AiConfigView } from '../../types';

export async function loadAiConfig(): Promise<AiConfigView> {
  return invoke<AiConfigView>('load_ai_config');
}

export async function saveAiConfig(update: AiConfigUpdate): Promise<AiConfigView> {
  return invoke<AiConfigView>('save_ai_config', { update });
}

export interface AiDeviceAssetContext {
  id: string;
  name: string;
  deviceType: string;
  version: number;
  yaml: string;
  yamlFilePath: string | null;
}

export interface AiCapabilityAssetContext {
  id: string;
  deviceId: string;
  name: string;
  description: string | null;
  version: number;
  yaml: string;
  yamlFilePath: string | null;
}

export interface AiAssetContext {
  devices: AiDeviceAssetContext[];
  capabilities: AiCapabilityAssetContext[];
}

export async function loadAiAssetContext(workspacePath: string): Promise<AiAssetContext> {
  return invoke<AiAssetContext>('load_ai_asset_context', {
    workspacePath: workspacePath.trim() || null,
  });
}

export function createCopilotStreamId(): string {
  const randomId = globalThis.crypto?.randomUUID?.();
  if (typeof randomId === 'string' && randomId.trim()) {
    return randomId;
  }
  return `copilot-${Date.now()}-${Math.random().toString(16).slice(2)}`;
}

export function toError(error: unknown): Error {
  if (error instanceof Error) {
    return error;
  }
  return new Error(String(error));
}

export function isRecoverableCopilotStreamError(error: Error): boolean {
  const message = error.message.trim().toLowerCase();
  return [
    'error decoding response body',
    '未收到结束信号',
    'connection reset',
    'broken pipe',
    'unexpected eof',
    'unexpected end of file',
    'connection closed before message completed',
    'stream interrupted',
    'stream stall',
  ].some((pattern) => message.includes(pattern));
}

async function waitForCopilotRetry(delayMs: number): Promise<void> {
  await new Promise<void>((resolve) => {
    globalThis.setTimeout(resolve, delayMs);
  });
}

export interface TauriEventStreamResult {
  text: string;
  finishReason?: string;
}

export interface TauriEventStreamRetryOptions {
  maxRetries?: number;
  onRetryStart?: (attempt: number, error: Error) => void | Promise<void>;
}

/** 通用 Tauri 事件流：自动重试 + thinking + settled guard。 */
export async function tauriEventStream(
  command: string,
  args: Record<string, unknown>,
  onDelta: (text: string) => void,
  onThinking?: (text: string) => void,
  retryOptions?: TauriEventStreamRetryOptions,
): Promise<TauriEventStreamResult> {
  const maxRetries = Math.max(0, Math.floor(retryOptions?.maxRetries ?? 1));

  for (let attempt = 0; attempt <= maxRetries; attempt += 1) {
    try {
      return await runTauriEventStreamAttempt(command, args, onDelta, onThinking);
    } catch (error) {
      const normalizedError = toError(error);
      const shouldRetry =
        attempt < maxRetries && isRecoverableCopilotStreamError(normalizedError);

      if (!shouldRetry) {
        throw normalizedError;
      }

      await retryOptions?.onRetryStart?.(attempt + 1, normalizedError);
      onDelta('');
      onThinking?.('');
      await waitForCopilotRetry(350);
    }
  }

  throw new Error('AI 流式输出重试失败');
}

/** 单个 chunk 最长静默时间（超时视为连接挂死，可重试）。 */
const STREAM_STALL_TIMEOUT_MS = 60_000;
/** 流式总超时（防止无限挂住）。 */
const STREAM_GLOBAL_TIMEOUT_MS = 180_000;

/** 单次 Tauri 事件流尝试（含 stall + global 双重超时）。 */
async function runTauriEventStreamAttempt(
  command: string,
  args: Record<string, unknown>,
  onDelta: (text: string) => void,
  onThinking?: (text: string) => void,
): Promise<TauriEventStreamResult> {
  const streamId = createCopilotStreamId();
  const eventName = `copilot://stream/${streamId}`;

  let accumulated = '';
  let thinkingAccumulated = '';
  let finishReason: string | undefined;
  let settled = false;
  let resolvePromise!: (value: TauriEventStreamResult) => void;
  let rejectPromise!: (reason?: unknown) => void;
  let stopListening: (() => void) | null = null;
  let stallTimer: ReturnType<typeof setTimeout> | null = null;
  let globalTimer: ReturnType<typeof setTimeout> | null = null;

  const completion = new Promise<TauriEventStreamResult>((resolve, reject) => {
    resolvePromise = resolve;
    rejectPromise = reject;
  });

  const clearTimers = () => {
    if (stallTimer !== null) {
      clearTimeout(stallTimer);
      stallTimer = null;
    }
    if (globalTimer !== null) {
      clearTimeout(globalTimer);
      globalTimer = null;
    }
  };

  const cleanup = () => {
    clearTimers();
    if (stopListening) {
      const fn = stopListening;
      stopListening = null;
      fn();
    }
  };

  const resolveStream = (value: string) => {
    if (settled) return;
    settled = true;
    cleanup();
    resolvePromise({ text: value, finishReason });
  };

  const rejectStream = (error: unknown) => {
    if (settled) return;
    settled = true;
    cleanup();
    rejectPromise(toError(error));
  };

  const resetStallTimer = () => {
    if (stallTimer !== null) clearTimeout(stallTimer);
    stallTimer = setTimeout(() => {
      rejectStream(new Error(
        `AI stream stall: no data received for ${STREAM_STALL_TIMEOUT_MS / 1000}s`,
      ));
    }, STREAM_STALL_TIMEOUT_MS);
  };

  globalTimer = setTimeout(() => {
    rejectStream(new Error(
      `AI stream stall: global timeout exceeded ${STREAM_GLOBAL_TIMEOUT_MS / 1000}s`,
    ));
  }, STREAM_GLOBAL_TIMEOUT_MS);

  resetStallTimer();

  stopListening = await listen<{
    delta?: string;
    thinking?: string;
    done?: boolean;
    error?: string;
    finishReason?: string;
  }>(eventName, (event) => {
    const payload = event.payload;
    // 收到任何事件 = 连接存活，重置 stall 计时
    resetStallTimer();

    if (payload.error) {
      rejectStream(new Error(payload.error));
      return;
    }
    if (payload.finishReason?.trim()) {
      finishReason = payload.finishReason.trim();
    }
    if (payload.thinking && onThinking) {
      thinkingAccumulated += payload.thinking;
      onThinking(thinkingAccumulated);
    }
    if (payload.delta) {
      accumulated += payload.delta;
      onDelta(accumulated);
    }
    if (payload.done) {
      resolveStream(accumulated);
    }
  });

  try {
    await invoke<void>(command, { ...args, streamId });
  } catch (error) {
    rejectStream(error);
  }

  return completion;
}

/// 重启 nazh-desktop 应用（ADR-0023 方案 B）。
/// 用于 EtherCAT TX/RX 任务死亡等进程级资源不可恢复场景。
export async function restartApp(): Promise<void> {
  return invoke<void>('restart_app');
}
