import { listen } from '@tauri-apps/api/event';
import { invoke } from '@tauri-apps/api/core';

import type { AiToolCall } from '../generated/AiToolCall';

/// 调试日志开关——开发期间保持 true，上线后可关闭。
const DEBUG_STREAM = true;

function streamLog(...args: unknown[]) {
  if (DEBUG_STREAM) console.log('[copilot-stream]', ...args);
}

function streamWarn(...args: unknown[]) {
  if (DEBUG_STREAM) console.warn('[copilot-stream]', ...args);
}

export interface ToolCallInfo {
  calls: AiToolCall[];
  round: number;
}

export interface ToolResultInfo {
  toolCallId: string;
  name: string;
  isError: boolean;
  contentPreview: string;
}

export interface CanvasOpEvent {
  type: 'add_node' | 'add_edge' | 'create_workflow';
  nodeId?: string;
  ref?: string;
  nodeType?: string;
  label?: string;
  config?: Record<string, unknown>;
  connectionId?: string;
  fromRef?: string;
  toRef?: string;
  fromId?: string;
  toId?: string;
  sourcePortId?: string;
  targetPortId?: string;
  name?: string;
}

export interface CopilotStreamResult {
  text: string;
  finishReason?: string;
  aborted?: boolean;
}

/// 发送用户消息并流式获取 AI 回复。
///
/// `signal` 用于取消：触发后通知后端停止生成，前端停止监听事件。
export async function copilotChatStream(
  conversationId: string,
  userMessage: string,
  workspacePath?: string,
  onDelta?: (text: string) => void,
  onThinking?: (text: string) => void,
  onToolCalls?: (info: ToolCallInfo) => void,
  onToolResult?: (info: ToolResultInfo) => void,
  onCanvasOp?: (op: CanvasOpEvent) => void,
  signal?: AbortSignal,
): Promise<CopilotStreamResult> {
  const streamId: string = await invoke('copilot_chat', {
    conversationId,
    userMessage,
    workspacePath,
  });

  streamLog('流开始', { streamId, conversationId, userMessage: userMessage.slice(0, 100) });

  const eventName = `copilot://stream/${streamId}`;
  let accumulated = '';
  let thinkingAccumulated = '';
  let finishReason: string | undefined;
  let settled = false;
  let eventCount = 0;
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

  const resolveStream = (value: string, aborted = false) => {
    if (settled) {
      return;
    }
    settled = true;
    cleanup();
    resolvePromise({ text: value, finishReason, aborted });
  };

  const rejectStream = (error: unknown) => {
    if (settled) {
      return;
    }
    settled = true;
    cleanup();
    rejectPromise(error instanceof Error ? error : new Error(String(error)));
  };

  // 前端取消时：通知后端 + 停止监听 + resolve
  if (signal) {
    const onAbort = () => {
      if (settled) return;
      streamLog('前端 abort 触发', { accumulatedLen: accumulated.length, eventCount });
      void invoke('copilot_cancel_stream', { streamId }).catch(() => {});
      resolveStream(accumulated, true);
    };
    signal.addEventListener('abort', onAbort, { once: true });
  }

  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  stopListening = await listen<any>(eventName, (event) => {
    // 双保险：即使 abort listener 未触发，也在事件回调中检查
    if (signal?.aborted) {
      streamLog('事件回调检测到已 abort，停止监听');
      resolveStream(accumulated, true);
      return;
    }

    eventCount += 1;
    const payload = event.payload;

    if (payload.error) {
      streamWarn('收到 error 事件', { error: String(payload.error).slice(0, 200) });
      rejectStream(payload.error);
      return;
    }
    if (payload.finishReason?.trim()) {
      finishReason = payload.finishReason.trim();
      streamLog('finishReason', finishReason);
    }
    if (payload.thinking && onThinking) {
      thinkingAccumulated += payload.thinking;
      onThinking(thinkingAccumulated);
    }
    if (payload.delta) {
      accumulated += payload.delta;
      // 每 20 个事件或首次打日志
      if (eventCount <= 3 || eventCount % 20 === 0) {
        streamLog('delta', {
          eventCount,
          deltaLen: payload.delta.length,
          accLen: accumulated.length,
          preview: accumulated.slice(-60),
        });
      }
      onDelta?.(accumulated);
    }
    if (payload.toolCalls && onToolCalls) {
      streamLog('toolCalls', {
        round: payload.toolCallRound ?? 0,
        count: (payload.toolCalls as AiToolCall[]).length,
        names: (payload.toolCalls as AiToolCall[]).map((t: AiToolCall) => t.name),
      });
      onToolCalls({
        calls: payload.toolCalls as AiToolCall[],
        round: payload.toolCallRound ?? 0,
      });
    }
    if (payload.toolResult && onToolResult) {
      const tr = payload.toolResult;
      streamLog('toolResult', { name: tr.name, isError: tr.isError });
      onToolResult({
        toolCallId: tr.toolCallId ?? '',
        name: tr.name ?? '',
        isError: tr.isError ?? false,
        contentPreview: tr.contentPreview ?? '',
      });
    }
    if (payload.canvasOp && onCanvasOp) {
      streamLog('canvasOp', { type: payload.canvasOp.type });
      onCanvasOp(payload.canvasOp as CanvasOpEvent);
    }
    if (payload.done) {
      streamLog('收到 done 事件', { accLen: accumulated.length, eventCount, finishReason });
      resolveStream(accumulated);
    }
  });

  return completion;
}
