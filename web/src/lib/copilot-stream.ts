/// Copilot 流式对话接口。
///
/// 前端直调 AI provider（通过 ai-sdk），不再经过 Tauri event 中间层。
/// 对话持久化（保存/加载消息）仍通过 IPC 在 Rust 侧管理。

import { invoke } from '@tauri-apps/api/core';

import { copilotStream, type CanvasOpEvent, type CopilotCallbacks, type CopilotResult } from '../ai';
import type { AiProviderView } from '../types';

export type { CanvasOpEvent, CopilotCallbacks };

export interface ToolCallInfo {
  names: string[];
}

export interface ToolResultInfo {
  name: string;
  isError: boolean;
}

export interface CopilotStreamResult {
  text: string;
  finishReason?: string;
  aborted?: boolean;
}

/// 发送用户消息并流式获取 AI 回复。
///
/// 流程：
/// 1. 保存用户消息到 DB
/// 2. 加载会话历史
/// 3. 前端直调 AI（多轮工具调用）
/// 4. 保存 AI 回复到 DB
export async function copilotChatStream(
  conversationId: string,
  userMessage: string,
  provider: AiProviderView,
  options: {
    toolCallingEnabled: boolean;
    userSystemPrompt?: string;
    runtimeContextPrompt?: string;
    temperature?: number;
    maxTokens?: number;
    topP?: number;
    workspacePath?: string;
  },
  callbacks?: {
    onDelta?: (text: string) => void;
    onThinking?: (text: string) => void;
    onToolCalls?: (info: ToolCallInfo) => void;
    onToolResult?: (info: ToolResultInfo) => void;
    onCanvasOp?: (op: CanvasOpEvent) => void;
  },
  signal?: AbortSignal,
): Promise<CopilotStreamResult> {
  // 1. 保存用户消息
  await invoke('copilot_save_message', {
    conversationId,
    role: 'user',
    content: userMessage,
  });

  // 2. 加载会话历史
  const history = await invoke<Array<{ id: string; role: string; content: string; thinking?: string }>>(
    'copilot_load_conversation',
    { id: conversationId },
  );

  const messages = history.map((m) => ({
    role: m.role as 'user' | 'assistant',
    content: m.content,
  }));

  // 3. 前端直调 AI
  const result = await copilotStream({
    provider,
    messages,
    toolCallingEnabled: options.toolCallingEnabled,
    userSystemPrompt: options.userSystemPrompt,
    runtimeContextPrompt: options.runtimeContextPrompt,
    params: {
      temperature: options.temperature,
      maxTokens: options.maxTokens,
      topP: options.topP,
    },
    workspacePath: options.workspacePath,
    signal,
    callbacks: {
      onDelta: callbacks?.onDelta,
      onThinking: callbacks?.onThinking,
      onToolCalls: callbacks?.onToolCalls
        ? (info) => callbacks.onToolCalls?.({ names: info.names })
        : undefined,
      onToolResult: callbacks?.onToolResult,
      onCanvasOp: callbacks?.onCanvasOp,
    },
  });

  // 4. 保存 AI 回复
  if (result.text.trim()) {
    await invoke('copilot_save_message', {
      conversationId,
      role: 'assistant',
      content: result.text,
      thinking: result.thinking,
    }).catch((error) => {
      console.warn('[copilot-stream] 保存 AI 回复失败', error);
    });
  }

  return result;
}
