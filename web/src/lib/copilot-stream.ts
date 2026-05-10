import { listen } from '@tauri-apps/api/event';
import { invoke } from '@tauri-apps/api/core';

export interface CopilotStreamResult {
  text: string;
  finishReason?: string;
}

/// 发送用户消息并流式获取 AI 回复。
export async function copilotChatStream(
  conversationId: string,
  userMessage: string,
  onDelta: (text: string) => void,
  onThinking?: (text: string) => void,
): Promise<CopilotStreamResult> {
  const streamId: string = await invoke('copilot_chat', {
    conversationId,
    userMessage,
  });

  const eventName = `copilot://stream/${streamId}`;
  let accumulated = '';
  let thinkingAccumulated = '';
  let finishReason: string | undefined;
  let settled = false;
  let stopListening: (() => void) | null = null;
  let resolvePromise!: (value: CopilotStreamResult) => void;
  let rejectPromise!: (reason?: unknown) => void;

  const completion = new Promise<CopilotStreamResult>((resolve, reject) => {
    resolvePromise = resolve;
    rejectPromise = reject;
  });

  const cleanup = () => {
    if (stopListening) {
      const nextStop = stopListening;
      stopListening = null;
      nextStop();
    }
  };

  const resolveStream = (value: string) => {
    if (settled) {
      return;
    }
    settled = true;
    cleanup();
    resolvePromise({ text: value, finishReason });
  };

  const rejectStream = (error: unknown) => {
    if (settled) {
      return;
    }
    settled = true;
    cleanup();
    rejectPromise(error instanceof Error ? error : new Error(String(error)));
  };

  stopListening = await listen<{
    delta?: string;
    thinking?: string;
    done?: boolean;
    error?: string;
    finishReason?: string;
  }>(eventName, (event) => {
    const payload = event.payload;
    if (payload.error) {
      rejectStream(payload.error);
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

  return completion;
}
