import type { FlowNodeEntity } from '@flowgram.ai/free-layout-editor';

import type { AiCompletionRequest } from '../generated/AiCompletionRequest';
import type { AiGenerationParams } from '../generated/AiGenerationParams';
import type { AiMessage } from '../generated/AiMessage';
import { copilotComplete } from './tauri';

export interface NodeContextInfo {
  nodeType: string;
  label: string;
  aiDescription: string;
}

export interface NodeContext {
  current: NodeContextInfo;
  upstream: NodeContextInfo[];
  downstream: NodeContextInfo[];
}

function extractNodeInfo(node: FlowNodeEntity): NodeContextInfo {
  const extInfo = (node.getExtInfo() ?? {}) as {
    label?: string;
    nodeType?: string;
    aiDescription?: string | null;
  };
  return {
    nodeType: extInfo.nodeType ?? node.flowNodeType,
    label: extInfo.label ?? node.id,
    aiDescription: extInfo.aiDescription ?? '',
  };
}

export function getNodeContext(node: FlowNodeEntity): NodeContext {
  const inputNodes = node.lines.inputNodes as FlowNodeEntity[];
  const outputNodes = node.lines.outputNodes as FlowNodeEntity[];
  return {
    current: extractNodeInfo(node),
    upstream: inputNodes.map(extractNodeInfo),
    downstream: outputNodes.map(extractNodeInfo),
  };
}

const SYSTEM_PROMPT = `你是工业边缘计算工作流的脚本编写助手。根据用户需求生成 Rhai 脚本代码。
规则：
- 只输出可执行的 Rhai 脚本，不要输出解释文字
- 脚本可通过 ctx.payload() 获取输入数据
- 脚本可通过 ctx.set_output(value) 设置输出
- 如需调用 AI，使用 ai_complete("prompt") 函数
- 不要使用 print() 等调试语句
- 保持简洁，专注于数据处理和转换逻辑`;

export function buildScriptGenerationPrompt(
  requirement: string,
  context: NodeContext,
): AiMessage[] {
  const upstreamText =
    context.upstream.length > 0
      ? context.upstream
          .map(
            (n) =>
              `  - ${n.label}（类型: ${n.nodeType}${n.aiDescription ? `，描述: ${n.aiDescription}` : ''}）`,
          )
          .join('\n')
      : '  无';
  const downstreamText =
    context.downstream.length > 0
      ? context.downstream
          .map(
            (n) =>
              `  - ${n.label}（类型: ${n.nodeType}${n.aiDescription ? `，描述: ${n.aiDescription}` : ''}）`,
          )
          .join('\n')
      : '  无';

  const userMessage = `节点类型：${context.current.nodeType}
节点名称：${context.current.label}
节点描述：${context.current.aiDescription || '无'}

上下游信息：
- 上游节点：
${upstreamText}
- 下游节点：
${downstreamText}

用户需求：
${requirement}`;

  return [
    { role: 'system', content: SYSTEM_PROMPT },
    { role: 'user', content: userMessage },
  ];
}

export interface GenerateScriptOptions {
  providerId: string;
  model?: string;
  timeoutMs?: number;
}

export async function generateScript(
  requirement: string,
  context: NodeContext,
  options: GenerateScriptOptions,
): Promise<string> {
  const messages = buildScriptGenerationPrompt(requirement, context);
  const params: AiGenerationParams = {
    temperature: 0.2,
    maxTokens: 2048,
    topP: 0.9,
  };
  const request: AiCompletionRequest = {
    providerId: options.providerId,
    model: options.model,
    messages,
    params,
    timeoutMs: options.timeoutMs ?? BigInt(60000),
  };
  const response = await copilotComplete(request);
  return response.content.trim();
}
