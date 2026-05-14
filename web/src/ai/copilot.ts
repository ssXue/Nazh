/// Copilot 编排核心。
///
/// 负责系统提示构建、对话历史管理、多轮工具调用循环。
/// AI HTTP 调用通过 ai-sdk 的 streamText 直接发到 provider。
/// 工具执行通过 IPC 调度到 Rust 引擎。
///
/// reasoning_content 的提取和回传由 @ai-sdk/openai-compatible 原生处理，
/// 不需要手动注入。

import { streamText, stepCountIs } from 'ai';

import type { AiGenerationParams, AiProviderView } from '../types';
import { createLanguageModel } from './providers';
import { buildCopilotTools, type CanvasOpEvent } from './copilot-tools';

const DEBUG_COPILOT = true;

function copilotLog(...args: unknown[]) {
  if (DEBUG_COPILOT) console.log('[ai/copilot]', ...args);
}

function copilotWarn(...args: unknown[]) {
  if (DEBUG_COPILOT) console.warn('[ai/copilot]', ...args);
}

/// 内置系统提示。
const BUILTIN_SYSTEM_PROMPT = `\
你是 Nazh 工业边缘平台的对话式副驾驶。Nazh 是一个本地运行的工业边缘工作流编排引擎，\
集成了设备数据采集、协议适配（Modbus、MQTT、串口、CAN/EtherCAT）、数据变换、脚本逻辑（Rhai）、\
AI 辅助和桌面运维 UI。

你的职责是帮助用户完成以下任务：
- 查询和解释工作流节点类型、设备资产、能力资产
- 解答 Nazh 平台的使用问题和工作流设计建议
- 根据用户描述创建工作流

回答时请遵循：
1. 用中文回答
2. 结合 Nazh 平台上下文作答，不要泛泛而谈
3. 使用 Markdown 格式回复，代码块用对应的语言标记`;

/// 工具调用规则提示（启用工具调用时追加到系统提示）。
const TOOL_CALLING_PROMPT = `

## 工具调用规则

你必须通过调用工具来完成用户的请求，不要只描述你打算做什么。
当用户要求创建或修改工作流时，直接调用工具，不要先说"我来查看"或"让我先了解一下"。

### 上下文查询工具
画布状态（节点、连线、选中节点）已作为系统提示的一部分自动注入，通常不需要主动查询。仅在需要精确的运行时数据（如变量值、执行时间、部署状态）时才调用以下工具。
- \`query_node_catalog\`：列出所有可用节点类型
- \`describe_node\`：查看指定节点的输入/输出 pin schema
- \`list_connections\`：查看已配置的连接
- \`search_devices\` / \`search_capabilities\`：搜索设备和能力资产
- \`get_active_workflow\` / \`query_workflow_status\`：查看当前工作流状态
- \`read_asset_yaml\`：读取设备或能力资产的完整 YAML

### 画布操作工具
- \`create_workflow\`：创建新工作流（初始化画布）
- \`add_workflow_node\`：添加节点，用 \`ref\` 标识（如 "timer"、"debug"）
- \`add_workflow_edge\`：连接节点，用 \`from_ref\`/\`to_ref\` 引用节点
- \`validate_workflow\`：校验工作流 JSON 结构

### 构建工作流的标准流程
1. 如果不清楚有哪些节点，调用 \`query_node_catalog\`
2. 调用 \`create_workflow\` 初始化画布
3. 依次调用 \`add_workflow_node\` 添加每个节点
4. **必须**调用 \`add_workflow_edge\` 将所有节点按逻辑顺序连接起来，\
每个有后续处理的节点都必须有连线指向下游，不允许出现孤立节点或遗漏连线

### 关键约束
- 不要输出关于工具调用的说明文字，直接调用工具
- 节点类型只能从 \`query_node_catalog\` 返回的列表中选择
- \`ref\` 是你自定义的简短英文别名（如 timer、modbus_read），不是系统 ID
- 需要连接的节点首选 capabilityCall（业务能力调用，自动按 capability 实现走 Modbus/MQTT/Serial/CAN）；低层协议节点（modbusRead、serialTrigger、canRead、canWrite）仅用于调试或兼容场景，都需传入 \`connection_id\`
- 对于工业场景，优先从最小可运行链路开始
- 纯问答（不涉及创建/修改工作流）直接用 Markdown 回答，不需要调用工具

### 完成后的回复要求
所有工具调用执行完毕后，你必须用简洁的中文回复用户，概括你完成了什么操作。
例如："已完成工作流创建，共添加了 3 个节点（定时器 → Modbus 读取 → 调试输出）并完成连线。"
不要只返回空文本。`;

export interface CopilotCallbacks {
  onDelta?: (accumulated: string) => void;
  onThinking?: (accumulated: string) => void;
  onToolCalls?: (info: { names: string[] }) => void;
  onToolResult?: (info: { name: string; isError: boolean }) => void;
  onCanvasOp?: (op: CanvasOpEvent) => void;
}

export interface CopilotResult {
  text: string;
  thinking?: string;
  finishReason?: string;
  aborted?: boolean;
}

export interface CopilotStreamOptions {
  provider: AiProviderView;
  modelOverride?: string;
  messages: Array<{ role: 'system' | 'user' | 'assistant'; content: string }>;
  toolCallingEnabled: boolean;
  userSystemPrompt?: string;
  /// 运行时画布上下文文本（由 copilot-context.ts 构建）。
  runtimeContextPrompt?: string;
  params?: AiGenerationParams;
  workspacePath?: string;
  signal?: AbortSignal;
  callbacks?: CopilotCallbacks;
}

/// 执行 Copilot 流式 AI 对话（含多轮工具调用）。
export async function copilotStream(options: CopilotStreamOptions): Promise<CopilotResult> {
  const {
    provider, modelOverride, messages, toolCallingEnabled, userSystemPrompt,
    runtimeContextPrompt, params, workspacePath, signal, callbacks,
  } = options;

  const model = await createLanguageModel({ provider, modelOverride });

  // 构建系统提示
  const systemParts = [BUILTIN_SYSTEM_PROMPT];
  if (toolCallingEnabled) {
    systemParts.push(TOOL_CALLING_PROMPT);
  }
  if (userSystemPrompt?.trim()) {
    systemParts.push(`\n\n用户补充指令：${userSystemPrompt.trim()}`);
  }
  if (runtimeContextPrompt) {
    systemParts.push(runtimeContextPrompt);
  }
  const systemPrompt = systemParts.join('\n\n');

  // 构建工具
  const tools = buildCopilotTools(callbacks?.onCanvasOp, undefined, workspacePath);

  copilotLog('流开始', { msgCount: messages.length, toolCallingEnabled });

  const result = streamText({
    model,
    system: systemPrompt,
    messages,
    tools: toolCallingEnabled ? tools : undefined,
    stopWhen: toolCallingEnabled ? stepCountIs(200) : stepCountIs(1),
    maxOutputTokens: params?.maxTokens ?? 8192,
    temperature: params?.temperature,
    topP: params?.topP,
    abortSignal: signal,
    onStepFinish: ({ toolCalls, toolResults }) => {
      if (toolCalls && toolCalls.length > 0) {
        const names = toolCalls.map((tc) => tc.toolName);
        copilotLog('tool calls', { names });
        callbacks?.onToolCalls?.({ names });
      }
      if (toolResults && toolResults.length > 0) {
        for (const tr of toolResults) {
          const output = tr.output;
          const isError = typeof output === 'string' && output.startsWith('错误:');
          copilotLog('tool result', { name: tr.toolName, isError });
          callbacks?.onToolResult?.({ name: tr.toolName, isError });
        }
      }
    },
  });

  let accumulated = '';
  let thinkingAccumulated = '';

  try {
    for await (const part of result.fullStream) {
      if (signal?.aborted) {
        copilotLog('abort 检测到，中断流');
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
          copilotWarn('stream error', { error: part.error });
          throw new Error(String(part.error));
        }
        default:
          break;
      }
    }
  } catch (error) {
    if (signal?.aborted) {
      copilotLog('流被 abort', { accLen: accumulated.length });
      return { text: accumulated, aborted: true };
    }
    throw error;
  }

  const finishReason = await result.finishReason;
  copilotLog('流结束', { accLen: accumulated.length, finishReason });

  return {
    text: accumulated,
    thinking: thinkingAccumulated || undefined,
    finishReason,
    aborted: signal?.aborted ?? false,
  };
}
