import type { FlowNodeEntity } from '@flowgram.ai/free-layout-editor';

import type { AiCompletionRequest, AiGenerationParams, AiMessage } from '../types';
import { copilotComplete, copilotCompleteStream } from './tauri';

export interface NodeContextInfo {
  nodeType: string;
  label: string;
}

export interface NodeContext {
  current: NodeContextInfo;
  upstream: NodeContextInfo[];
  downstream: NodeContextInfo[];
}

const DEFAULT_SCRIPT_GENERATION_PARAMS: AiGenerationParams = {
  temperature: 0.7,
  maxTokens: 2048,
  topP: 1,
};

const DEFAULT_SCRIPT_GENERATION_TIMEOUT_MS = 60_000;

function extractNodeInfo(node: FlowNodeEntity): NodeContextInfo {
  const extInfo = (node.getExtInfo() ?? {}) as {
    label?: string;
    nodeType?: string;
  };
  return {
    nodeType: extInfo.nodeType ?? String(node.flowNodeType),
    label: extInfo.label ?? node.id,
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

const SYSTEM_PROMPT = `你是工业边缘计算工作流的脚本编写助手。根据用户需求生成可直接在 Nazh 中运行的 Rhai 脚本代码。
规则：
- 只输出可执行的 Rhai 脚本，不要输出解释、标题或 Markdown 代码块
- 输入数据直接来自变量 payload
- 如果需要修改输入，请直接修改 payload，并在最后返回 payload
- 如果需要返回新的值或对象，直接把该值作为脚本最后一行返回
- 当前运行时只提供这些可用能力：payload、ai_complete("prompt")、rand(min, max)、now_ms()、from_json(text)、to_json(value)、is_blank(text)
- 如需调用 AI，使用 ai_complete("prompt") 函数
- ai_complete() 会自动解析 JSON 格式的返回值，在 prompt 中明确要求 JSON 输出即可获得结构化数据
- rand(min, max) 返回一个闭区间整数，min 和 max 都会被包含
- now_ms() 返回当前 Unix 时间戳，单位毫秒
- from_json(text) 把 JSON 字符串解析成可继续索引的对象或数组
- to_json(value) 把对象、数组或基础值序列化成 JSON 字符串
- is_blank(text) 用于判断字符串去掉首尾空白后是否为空
- 示例：let result = ai_complete("分析数据并以 JSON 格式返回 {temperature, status}"); payload["temp"] = result["temperature"];
- 不要使用 Math.random()、random()、ctx、print()、console.log() 或其他未在上面列出的 API
- 优先使用 payload["field"] 这种字段访问方式，保持脚本简洁

示例：
payload["normalized"] = true;
payload`;

export function buildScriptGenerationPrompt(
  requirement: string,
  context: NodeContext,
): AiMessage[] {
  const upstreamText =
    context.upstream.length > 0
      ? context.upstream
          .map(
            (n) =>
              `  - ${n.label}（类型: ${n.nodeType}）`,
          )
          .join('\n')
      : '  无';
  const downstreamText =
    context.downstream.length > 0
      ? context.downstream
          .map(
            (n) =>
              `  - ${n.label}（类型: ${n.nodeType}）`,
          )
          .join('\n')
      : '  无';

  const userMessage = `节点类型：${context.current.nodeType}
节点名称：${context.current.label}

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
  model?: string | null;
  params?: AiGenerationParams;
  timeoutMs?: number | null;
}

function resolveGenerationParams(params?: AiGenerationParams): AiGenerationParams {
  return {
    temperature: params?.temperature ?? DEFAULT_SCRIPT_GENERATION_PARAMS.temperature,
    maxTokens: params?.maxTokens ?? DEFAULT_SCRIPT_GENERATION_PARAMS.maxTokens,
    topP: params?.topP ?? DEFAULT_SCRIPT_GENERATION_PARAMS.topP,
  };
}

const NL_PREFIX_PATTERNS = [
  /^(?:这是|以下是|下面是|这是生成的|这是你的|here\s+is|below\s+is|the\s+following\s+is|sure!?\s*here'?s?|certainly!?\s*here'?s?)\s*.+/i,
];

export function sanitizeGeneratedScript(content: string): string {
  const trimmed = content.trim();
  if (!trimmed) {
    return '';
  }

  const codeBlockRegex = /```(?:rhai|rust|javascript|js|typescript|ts)?\s*([\s\S]*?)```/gi;
  let lastMatch: string | null = null;
  let match: RegExpExecArray | null;
  while ((match = codeBlockRegex.exec(trimmed)) !== null) {
    if (match[1]?.trim()) {
      lastMatch = match[1].trim();
    }
  }
  if (lastMatch) {
    return lastMatch;
  }

  const lines = trimmed.split('\n');
  while (lines.length > 0) {
    const firstLine = lines[0].trim();
    if (!firstLine) {
      lines.shift();
      continue;
    }
    if (NL_PREFIX_PATTERNS.some((pattern) => pattern.test(firstLine))) {
      lines.shift();
      continue;
    }
    break;
  }

  return lines.join('\n').trim();
}

export async function generateScript(
  requirement: string,
  context: NodeContext,
  options: GenerateScriptOptions,
): Promise<string> {
  const messages = buildScriptGenerationPrompt(requirement, context);
  const request: AiCompletionRequest = {
    providerId: options.providerId,
    model: options.model ?? undefined,
    messages,
    params: resolveGenerationParams(options.params),
    timeoutMs: options.timeoutMs ?? DEFAULT_SCRIPT_GENERATION_TIMEOUT_MS,
  };
  const response = await copilotComplete(request);
  return sanitizeGeneratedScript(response.content);
}

export async function generateScriptStream(
  requirement: string,
  context: NodeContext,
  options: GenerateScriptOptions,
  onDelta: (rawText: string) => void,
  onThinking?: (text: string) => void,
): Promise<string> {
  const messages = buildScriptGenerationPrompt(requirement, context);
  const request: AiCompletionRequest = {
    providerId: options.providerId,
    model: options.model ?? undefined,
    messages,
    params: resolveGenerationParams(options.params),
    timeoutMs: options.timeoutMs ?? DEFAULT_SCRIPT_GENERATION_TIMEOUT_MS,
  };
  const rawContent = await copilotCompleteStream(request, onDelta, onThinking);
  return sanitizeGeneratedScript(rawContent);
}
