/// 设备 / 能力 AI 结构化抽取。
///
/// 将 Rust 端的 AI 抽取提示词模板和 HTTP 调用前移到前端，
/// 使用 Vercel AI SDK 直连 provider。
/// PDF 文本提取仍通过 IPC 调度到 Rust（JS 无等价物）。

import { generateText, streamText } from 'ai';

import type { AiProviderView } from '../types';
import { createLanguageModel } from './providers';

// ── 提示词模板（从 Rust 端迁移） ──

const EXTRACTION_SYSTEM_PROMPT = '你是一个工业设备建模专家。从用户提供的说明书文本中抽取设备信息，输出 YAML 格式的 DeviceSpec。只输出 YAML，不要解释。';

const PROPOSAL_SYSTEM_PROMPT = `你是一个工业设备建模专家。从用户提供的说明书文本中抽取设备信息并推断设备能力。\
输出严格 JSON 格式，结构如下：
{"deviceYaml": "<DeviceSpec YAML 文本>", "capabilityYamls": ["<CapabilitySpec YAML 文本>", ...], "uncertainties": [{"fieldPath": "...", "guessedValue": "...", "reason": "..."}], "warnings": ["..."]}

规则：
- deviceYaml 必须是合法的 DeviceSpec YAML
- capabilityYamls 从写信号（analog_output / digital_output）推断底层操作能力，每个能力封装一个写信号
- uncertainties 用于标记信息不完整或需要人工确认的字段
- warnings 用于标记潜在安全问题或不一致
- 只输出 JSON，不要解释`;

/// DeviceSpec YAML 模板（buildExtractionPrompt / buildProposalPrompt 共用）。
const DEVICE_SPEC_YAML_TEMPLATE = `\
DeviceSpec 结构参考：
\`\`\`yaml
id: <设备唯一标识>
type: <设备类型>
manufacturer: <厂商>  # 可选
model: <型号>  # 可选
signals:
  - id: <信号 ID>
    signal_type: <analog_input / analog_output / digital_input / digital_output>
    unit: <单位>  # 可选
    range: [min, max]  # 可选
    source:  # 三种类型，必须提供对应字段
      # register 类型（Modbus）：
      type: register
      register: <地址，整数>
      data_type: <bool / u16 / i16 / u32 / i32 / float32 / float64 / string>
      access: <read / write / read_write>  # 默认 read
      bit: <位号>  # 可选
      # topic 类型（MQTT）：
      # type: topic
      # topic: <MQTT 主题路径>
      # serial_command 类型（串口）：
      # type: serial_command
      # command: <串口命令字符串>
alarms:
  - id: <告警 ID>
    condition: <Rhai 条件表达式>
    severity: <info / warning / critical>
    action: <动作>  # 可选
\`\`\`

重要规则：
- source.type 为 register 时必须提供 register 和 data_type 字段
- source.type 为 topic 时必须提供 topic 字段
- source.type 为 serial_command 时必须提供 command 字段
- 如果说明书中未明确指定协议，优先使用 register 类型`;

/// CapabilitySpec YAML 模板（buildProposalPrompt 专用）。
const CAPABILITY_SPEC_YAML_TEMPLATE = `\
CapabilitySpec 结构参考：
\`\`\`yaml
id: <能力 ID，格式 device.action>
device_id: <关联设备 ID>
description: <能力描述>
inputs:
  - id: <参数 ID>
    unit: <单位>
    range: [min, max]
    required: true
outputs:
  - id: <输出 ID>
    type: <bool / f64 / string>
preconditions:
  - <Rhai 前置条件表达式>
implementation:
  type: <modbus-write / mqtt-publish / serial-command>
  register: <目标寄存器>
  value: <值表达式，如 $param_id>
safety:
  level: <high / medium / low>
  requires_approval: false
\`\`\``;

function buildExtractionPrompt(text: string): string {
  return `请从以下设备说明书中抽取设备信息，输出 YAML 格式的 DeviceSpec。

${DEVICE_SPEC_YAML_TEMPLATE}

说明书文本：
---
${text}
---`;
}

function buildProposalPrompt(text: string): string {
  return `请从以下设备说明书中抽取设备信息和推断设备能力。

${DEVICE_SPEC_YAML_TEMPLATE}

${CAPABILITY_SPEC_YAML_TEMPLATE}

说明书文本：
---
${text}
---`;
}

function buildCorrectionPrompt(failedYaml: string, error: string): string {
  return `之前生成的设备 DSL YAML 保存时验证失败，请**仅修正**导致错误的字段，不要改动其他已经正确的部分。

失败的 deviceYaml：
\`\`\`yaml
${failedYaml}
\`\`\`

验证错误：${error}

请输出修正后的完整 JSON（与首次抽取相同的 JSON 结构）。`;
}

// ── 响应解析 ──

/// 从 AI 响应中提取 YAML（去除 markdown 代码块包裹）。
function extractYamlFromResponse(content: string): string {
  const trimmed = content.trim();
  const yamlMatch = trimmed.match(/^```(?:yaml|yml)?\s*\n([\s\S]*?)\n?```$/);
  if (yamlMatch) return yamlMatch[1].trim();
  return trimmed;
}

/// 从 AI 响应中提取 JSON（去除 markdown 代码块包裹）。
function extractJsonFromResponse(content: string): string {
  const trimmed = content.trim();
  const jsonMatch = trimmed.match(/^```(?:json)?\s*\n([\s\S]*?)\n?```$/);
  if (jsonMatch) return jsonMatch[1].trim();
  return trimmed;
}

// ── 公共类型 ──

export interface UncertaintyItem {
  fieldPath: string;
  guessedValue: string;
  reason: string;
}

export interface ExtractionProposal {
  deviceYamls: string[];
  capabilityYamls: string[];
  uncertainties: UncertaintyItem[];
  warnings: string[];
}

interface RawExtractionProposal {
  deviceYaml: string;
  capabilityYamls?: string[];
  uncertainties?: UncertaintyItem[];
  warnings?: string[];
}

// ── 公共 API ──

/// 从文本中 AI 抽取 DeviceSpec YAML（非流式）。
export async function extractDeviceFromText(
  text: string,
  provider: AiProviderView,
): Promise<string> {
  const model = await createLanguageModel({ provider });

  const result = await generateText({
    model,
    system: EXTRACTION_SYSTEM_PROMPT,
    prompt: buildExtractionPrompt(text),
    maxOutputTokens: 16384,
    temperature: 0.1,
  });

  return extractYamlFromResponse(result.text);
}

/// 从文本中 AI 抽取设备 + 能力结构化提案（非流式）。
export async function extractDeviceProposal(
  text: string,
  provider: AiProviderView,
  correction?: { yaml: string; error: string },
): Promise<ExtractionProposal> {
  const model = await createLanguageModel({ provider });

  const userContent = correction
    ? buildCorrectionPrompt(correction.yaml, correction.error)
    : buildProposalPrompt(text);

  if (!correction && text.trim().length === 0) {
    throw new Error('首次抽取模式需要提供非空的设备说明文本');
  }

  const result = await generateText({
    model,
    system: PROPOSAL_SYSTEM_PROMPT,
    prompt: userContent,
    maxOutputTokens: 16384,
    temperature: 0.1,
  });

  const jsonText = extractJsonFromResponse(result.text);
  const raw: RawExtractionProposal = JSON.parse(jsonText);

  return {
    deviceYamls: [raw.deviceYaml],
    capabilityYamls: raw.capabilityYamls ?? [],
    uncertainties: raw.uncertainties ?? [],
    warnings: raw.warnings ?? [],
  };
}

/// 流式 AI 抽取设备 + 能力结构化提案。
export async function extractDeviceProposalStream(
  text: string,
  provider: AiProviderView,
  callbacks: {
    onDelta: (accumulated: string) => void;
    onThinking?: (accumulated: string) => void;
  },
  correction?: { yaml: string; error: string },
  signal?: AbortSignal,
): Promise<string> {
  const model = await createLanguageModel({ provider });

  const userContent = correction
    ? buildCorrectionPrompt(correction.yaml, correction.error)
    : buildProposalPrompt(text);

  if (!correction && text.trim().length === 0) {
    throw new Error('首次抽取模式需要提供非空的设备说明文本');
  }

  const result = streamText({
    model,
    system: PROPOSAL_SYSTEM_PROMPT,
    prompt: userContent,
    maxOutputTokens: 16384,
    temperature: 0.1,
    abortSignal: signal,
  });

  let accumulated = '';
  let thinkingAccumulated = '';

  for await (const part of result.fullStream) {
    if (signal?.aborted) break;

    switch (part.type) {
      case 'text-delta': {
        accumulated += part.text;
        callbacks.onDelta(accumulated);
        break;
      }
      case 'reasoning-delta': {
        thinkingAccumulated += part.text;
        callbacks.onThinking?.(thinkingAccumulated);
        break;
      }
      case 'error': {
        throw new Error(String(part.error));
      }
    }
  }

  return accumulated;
}
