/// RFC-0006 Phase 5：多阶段结构化抽取管道。
///
/// 替代旧 `device-extraction.ts` 的单次 `generateText` + 手写 JSON parse，
/// 用 `generateObject` + Zod schema 约束生成层，多阶段管道提升准确性。

import { generateObject, generateText } from 'ai';
import { z } from 'zod';

import type { AiProviderView } from '../types';
import { createLanguageModel } from './providers';
import { DeviceOutlineSchema, type DeviceOutline } from './extraction-schemas';
import { SignalDetailSchema, type SignalDetail } from './extraction-schemas';

// ── 通用 helper ──

/// 用 structured output 抽取，失败时 fallback 到 generateText + JSON.parse + schema.parse。
export async function extractWithSchema<T>(
  prompt: string,
  schema: z.ZodSchema<T>,
  provider: AiProviderView,
  systemPrompt?: string,
): Promise<T> {
  const model = await createLanguageModel({ provider });

  try {
    const result = await generateObject({
      model,
      schema,
      prompt,
      ...(systemPrompt ? { system: systemPrompt } : {}),
      temperature: 0.1,
      maxOutputTokens: 16384,
    });
    return result.object;
  } catch (primaryError) {
    // fallback：generateText + JSON.parse + schema.parse
    console.warn('[extraction-pipeline] generateObject 失败，fallback 到 generateText', primaryError instanceof Error ? primaryError.message : String(primaryError));
    const textResult = await generateText({
      model,
      prompt,
      ...(systemPrompt ? { system: systemPrompt } : {}),
      temperature: 0.1,
      maxOutputTokens: 16384,
    });
    const jsonText = extractJsonFromResponse(textResult.text);
    const parsed = JSON.parse(jsonText);
    return schema.parse(parsed);
  }
}

/// 从 AI 响应中提取 JSON。
///
/// 依次尝试：markdown 代码块 → 原始文本 → 修复后 JSON。
function extractJsonFromResponse(content: string): string {
  const trimmed = content.trim();

  // 尝试提取 ```json ... ``` 代码块
  const jsonMatch = trimmed.match(/^```(?:json)?\s*\n([\s\S]*?)\n?```$/);
  if (jsonMatch) return jsonMatch[1].trim();

  // 尝试直接 parse
  try {
    JSON.parse(trimmed);
    return trimmed;
  } catch {
    // 继续尝试修复
  }

  // 尝试提取文本中间的 JSON 对象/数组
  const objMatch = trimmed.match(/\{[\s\S]*\}/);
  if (objMatch) {
    const repaired = repairJson(objMatch[0]);
    try {
      JSON.parse(repaired);
      return repaired;
    } catch {
      // 继续尝试
    }
  }

  const arrMatch = trimmed.match(/\[[\s\S]*\]/);
  if (arrMatch) {
    const repaired = repairJson(arrMatch[0]);
    try {
      JSON.parse(repaired);
      return repaired;
    } catch {
      // 放弃修复
    }
  }

  // 全部失败，返回原始文本（由 schema.parse 报错）
  return trimmed;
}

/// 尝试修复常见 JSON 问题：截断的括号、trailing comma。
function repairJson(text: string): string {
  let s = text;

  // 移除 trailing comma（} 或 ] 前面的逗号）
  s = s.replace(/,\s*([}\]])/g, '$1');

  // 补全截断的括号
  let openBraces = 0;
  let openBrackets = 0;
  let inString = false;
  let escape = false;
  for (const ch of s) {
    if (escape) { escape = false; continue; }
    if (ch === '\\') { escape = true; continue; }
    if (ch === '"') { inString = !inString; continue; }
    if (inString) continue;
    if (ch === '{') openBraces++;
    else if (ch === '}') openBraces--;
    else if (ch === '[') openBrackets++;
    else if (ch === ']') openBrackets--;
  }

  // 截断可能在字符串中间，先关闭字符串
  if (inString) s += '"';

  // 截断可能在值中间（如数字、true 等），不追加额外字符
  s += ']'.repeat(Math.max(0, openBrackets));
  s += '}'.repeat(Math.max(0, openBraces));

  return s;
}

// ── 提示词 ──

const OUTLINE_SYSTEM_PROMPT = '你是一个工业设备建模专家。从说明书文本中抽取设备大纲信息。必须严格遵循输出 schema。';

const SIGNAL_FILL_SYSTEM_PROMPT = `你是一个工业设备建模专家。根据设备大纲和原始说明书文本，填充每个信号的完整细节。
必须严格遵循输出 schema。对于说明书未明确提供的寄存器地址、CAN ID、PDO 索引等安全相关字段，
不要编造数值——省略该信号或将其 confidence 设为 "low"。`;

const CORRECTION_SYSTEM_PROMPT = '你是一个工业设备 DSL 修正专家。根据校验错误修正信号细节，严格遵循输出 schema。';

// ── 管道输出 ──

export interface PipelineResult {
  outline: DeviceOutline;
  signalDetail: SignalDetail;
  validationErrors: Array<{ path: string; message: string }>;
  validationWarnings: Array<{ path: string; message: string }>;
  correctionRounds: number;
  truncated: boolean;
  stage2Skipped: boolean;
}

/// 将管道结果格式化为 Markdown 文本，注入 copilot 对话。
export function formatPipelineResult(result: PipelineResult): string {
  const parts: string[] = [];

  parts.push('## 设备大纲');
  parts.push(`- ID: ${result.outline.id}`);
  parts.push(`- 类型: ${result.outline.type}`);
  if (result.outline.protocol) parts.push(`- 协议: ${result.outline.protocol}`);
  parts.push(`- 信号数: ${result.outline.signalCount}`);

  if (result.signalDetail.signals.length > 0) {
    parts.push('\n## 信号列表');
    parts.push('| ID | 类型 | 单位 | 量程 | 数据源 | 置信度 |');
    parts.push('|----|------|------|------|--------|--------|');
    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    for (const sig of result.signalDetail.signals as any[]) {
      const unit = sig.unit ?? '-';
      const range = sig.range ? `${sig.range[0]}~${sig.range[1]}` : '-';
      const source = formatSource(sig.source as Record<string, unknown>);
      parts.push(`| ${sig.id} | ${sig.signalType} | ${unit} | ${range} | ${source} | ${sig.confidence} |`);
    }
  }

  if (result.signalDetail.alarms && result.signalDetail.alarms.length > 0) {
    parts.push('\n## 告警');
    for (const alarm of result.signalDetail.alarms) {
      parts.push(`- ${alarm.id}: ${alarm.condition} (${alarm.severity})`);
    }
  }

  const totalDiags = result.validationErrors.length + result.validationWarnings.length;
  if (totalDiags > 0) {
    parts.push(`\n## 校验结果`);
    parts.push(`- ${result.validationErrors.length} errors, ${result.validationWarnings.length} warnings`);
    for (const e of result.validationErrors) {
      parts.push(`- ❌ ${e.path}: ${e.message}`);
    }
    for (const w of result.validationWarnings) {
      parts.push(`- ⚠️ ${w.path}: ${w.message}`);
    }
  }

  if (result.outline.uncertainties && result.outline.uncertainties.length > 0) {
    parts.push('\n## 不确定字段');
    for (const u of result.outline.uncertainties) {
      parts.push(`- ${u.fieldPath}: 推测值 ${u.guessedValue}，原因：${u.reason}`);
    }
  }

  if (result.correctionRounds > 0) {
    parts.push(`\n_自动修正 ${result.correctionRounds} 轮_`);
  }
  if (result.truncated) {
    parts.push('\n_⚠️ 原始文本过长已截断（30k 字符），部分信息可能缺失_');
  }
  if (result.stage2Skipped) {
    parts.push('\n_ℹ️ 简单设备已跳过信号填充阶段，细节可能不完整。可让 Copilot 补充具体寄存器地址等细节_');
  }

  return parts.join('\n');
}

function formatSource(source: Record<string, unknown>): string {
  const type = source.type as string;
  switch (type) {
    case 'register': return `register:${source.register}/${source.access ?? 'read'}/${source.data_type}`;
    case 'topic': return `topic:${source.topic}`;
    case 'serial_command': return `cmd:${source.command}`;
    case 'can_frame': return `can:${source.can_id}/${source.byte_offset}+${source.byte_length}`;
    case 'ethercat_pdo': return `pdo:${source.pdo_index}/${source.entry_index}`;
    default: return type;
  }
}

// ── 管道主入口 ──

const MAX_TEXT_LENGTH = 30_000;
const CHUNK_SIZE = 15_000;
const MAX_CORRECTION_ROUNDS = 2;

/// 四阶段抽取管道。
///
/// Stage 0（可选）：大文本分片摘要
/// Stage 1：大纲抽取（generateObject + DeviceOutlineSchema）
/// Stage 2：信号填充（generateObject + SignalDetailSchema）
/// Stage 3：Rust 侧校验（validate_device_yaml IPC）
/// Stage 4：自动修正循环（最多 2 轮）
export async function extractDevicePipeline(
  rawText: string,
  provider: AiProviderView,
  validateDeviceYaml: (yaml: string) => Promise<{
    valid: boolean;
    errors: Array<{ path: string; message: string }>;
    warnings: Array<{ path: string; message: string }>;
  }>,
): Promise<PipelineResult> {
  // Stage 0：大文本预处理
  let text = rawText;
  let truncated = false;
  if (text.length > MAX_TEXT_LENGTH) {
    text = await summarizeLongText(text, provider);
    truncated = true;
  }

  // Stage 1：大纲抽取
  const outline = await extractWithSchema(
    `从以下设备说明书中抽取设备大纲：\n---\n${text}\n---`,
    DeviceOutlineSchema,
    provider,
    OUTLINE_SYSTEM_PROMPT,
  );

  // Stage 2：信号填充（简单设备跳过）
  const SIMPLE_SIGNAL_THRESHOLD = 3;
  let signalDetail: SignalDetail;
  let stage2Skipped = false;

  if (outline.signalCount <= SIMPLE_SIGNAL_THRESHOLD) {
    // 简单设备：直接从 signalSummaries 构建 SignalDetail，source 填低置信度占位
    signalDetail = buildSimpleSignalDetail(outline);
    stage2Skipped = true;
  } else {
    const outlineContext = JSON.stringify(outline, null, 2);
    signalDetail = await extractWithSchema(
      `设备大纲：\n${outlineContext}\n\n原始说明书：\n---\n${text}\n---\n\n请填充所有信号的完整细节。`,
      SignalDetailSchema,
      provider,
      SIGNAL_FILL_SYSTEM_PROMPT,
    ) as SignalDetail;
  }

  // Stage 3 + 4：校验 + 自动修正循环
  let validationErrors: Array<{ path: string; message: string }> = [];
  let validationWarnings: Array<{ path: string; message: string }> = [];
  let correctionRounds = 0;

  const deviceYaml = buildDeviceYaml(outline, signalDetail as any);

  try {
    const validation = await validateDeviceYaml(deviceYaml);
    validationErrors = validation.errors;
    validationWarnings = validation.warnings;

    // Stage 4：自动修正循环
    while (validationErrors.length > 0 && correctionRounds < MAX_CORRECTION_ROUNDS) {
      correctionRounds++;
      const errorList = validationErrors.map(e => `${e.path}: ${e.message}`).join('\n');

      signalDetail = await extractWithSchema(
        `当前信号细节有校验错误，请修正：\n${JSON.stringify(signalDetail, null, 2)}\n\n校验错误：\n${errorList}`,
        SignalDetailSchema,
        provider,
        CORRECTION_SYSTEM_PROMPT,
      ) as SignalDetail;

      const newYaml = buildDeviceYaml(outline, signalDetail as any);
      const newValidation = await validateDeviceYaml(newYaml);
      validationErrors = newValidation.errors;
      validationWarnings = newValidation.warnings;
    }
  } catch {
    // validate_device_yaml IPC 失败时不阻断，返回未校验结果
  }

  return {
    outline,
    signalDetail: signalDetail as any as SignalDetail,
    validationErrors,
    validationWarnings,
    correctionRounds,
    truncated,
    stage2Skipped,
  };
}

// ── 大文本分片摘要 ──

/// 将过长文本分片摘要，保留关键设备信息。
/// ≤30k：直接使用（不应进入此函数）
/// 30k-100k：按段落分片，逐片摘要，合并
/// >100k：分片摘要后再做一轮总结压缩
async function summarizeLongText(text: string, provider: AiProviderView): Promise<string> {
  const chunks = splitIntoChunks(text, CHUNK_SIZE);
  const summaries = await Promise.all(
    chunks.map((chunk, i) => summarizeChunk(chunk, i + 1, chunks.length, provider)),
  );
  const merged = summaries.join('\n\n');

  // 二次压缩：合并后仍超限
  if (merged.length > MAX_TEXT_LENGTH) {
    const compressed = await summarizeChunk(merged, 1, 1, provider);
    return compressed + '\n\n[原文过长，经分片摘要+二次压缩，细节可能有丢失]';
  }

  return merged + '\n\n[原文过长，经分片摘要处理，细节可能有丢失]';
}

/// 按段落边界分片，尽量不截断段落。
function splitIntoChunks(text: string, maxChunkSize: number): string[] {
  const paragraphs = text.split(/\n{2,}/);
  const chunks: string[] = [];
  let current = '';

  for (const para of paragraphs) {
    if (current.length + para.length + 2 > maxChunkSize && current.length > 0) {
      chunks.push(current.trim());
      current = para;
    } else {
      current = current ? `${current}\n\n${para}` : para;
    }
  }
  if (current.trim()) chunks.push(current.trim());

  return chunks;
}

/// 对单个分片做 AI 摘要，保留设备相关信息。
async function summarizeChunk(chunk: string, index: number, total: number, provider: AiProviderView): Promise<string> {
  const model = await createLanguageModel({ provider });
  const result = await generateText({
    model,
    prompt: `以下是一份工业设备说明书文本的第 ${index}/${total} 部分。请提取并保留所有与设备型号、通信协议、寄存器地址、信号定义、参数范围、告警条件相关的信息。去除重复内容、通用说明和不相关的法律/版权声明。\n\n---\n${chunk}\n---`,
    temperature: 0.1,
    maxOutputTokens: 8192,
  });
  return result.text;
}

/// 简单设备（≤3 信号）快速构建：从大纲的 signalSummaries 直接构建 SignalDetail。
/// source 字段填低置信度占位（type: register + register: 0）。
function buildSimpleSignalDetail(outline: DeviceOutline): SignalDetail {
  const signals = outline.signalSummaries.map((summary) => ({
    id: summary.id,
    signalType: summary.signalType,
    source: { type: 'register' as const, register: 0, access: 'read' as const, data_type: 'u16' as const },
    confidence: 'low' as const,
    ...(!['digital_input', 'digital_output'].includes(summary.signalType) ? { unit: '-', range: [0, 100] as [number, number] } : {}),
  }));
  return { signals, alarms: [] };
}

/// 将大纲 + 信号细节组装为 DeviceSpec YAML 文本。
function buildDeviceYaml(outline: DeviceOutline, detail: SignalDetail): string {
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  const signals = detail.signals as any[];
  const lines: string[] = [];
  lines.push(`id: ${outline.id}`);
  lines.push(`type: ${outline.type}`);
  if (outline.manufacturer) lines.push(`manufacturer: "${outline.manufacturer}"`);
  if (outline.model) lines.push(`model: "${outline.model}"`);

  if (outline.protocol) {
    const connId = outline.connection?.id ?? `${outline.id}_conn`;
    lines.push('connection:');
    lines.push(`  type: ${outline.protocol}`);
    lines.push(`  id: "${connId}"`);
    if (outline.connection?.unit != null) lines.push(`  unit: ${outline.connection.unit}`);
  }

  if (signals.length > 0) {
    lines.push('signals:');
    for (const sig of signals) {
      lines.push(`  - id: ${sig.id}`);
      lines.push(`    signal_type: ${sig.signalType}`);
      if ('unit' in sig && sig.unit) lines.push(`    unit: "${sig.unit}"`);
      if ('range' in sig && sig.range) lines.push(`    range: [${sig.range[0]}, ${sig.range[1]}]`);
      lines.push('    source:');
      lines.push(formatSourceYaml(sig.source as Record<string, unknown>));
      if ('scale' in sig && sig.scale) lines.push(`    scale: "${sig.scale}"`);
    }
  }

  if (detail.alarms && detail.alarms.length > 0) {
    lines.push('alarms:');
    for (const alarm of detail.alarms) {
      lines.push(`  - id: ${alarm.id}`);
      lines.push(`    condition: "${alarm.condition}"`);
      lines.push(`    severity: ${alarm.severity}`);
      if (alarm.action) lines.push(`    action: ${alarm.action}`);
    }
  }

  return lines.join('\n');
}

function formatSourceYaml(source: Record<string, unknown>, indent = 6): string {
  const pad = ' '.repeat(indent);
  const type = source.type as string;
  switch (type) {
    case 'register':
      return [
        `${pad}type: register`,
        `${pad}register: ${source.register}`,
        `${pad}access: ${source.access ?? 'read'}`,
        `${pad}data_type: ${source.data_type}`,
        ...(source.bit != null ? [`${pad}bit: ${source.bit}`] : []),
      ].join('\n');
    case 'topic':
      return `${pad}type: topic\n${pad}topic: "${source.topic}"`;
    case 'serial_command':
      return `${pad}type: serial_command\n${pad}command: "${source.command}"`;
    case 'can_frame':
      return [
        `${pad}type: can_frame`,
        `${pad}can_id: ${source.can_id}`,
        `${pad}is_extended: ${source.is_extended ?? false}`,
        `${pad}byte_offset: ${source.byte_offset}`,
        `${pad}byte_length: ${source.byte_length}`,
        `${pad}data_type: ${source.data_type}`,
        `${pad}byte_order: ${source.byte_order ?? 'big_endian'}`,
      ].join('\n');
    case 'ethercat_pdo':
      return [
        `${pad}type: ethercat_pdo`,
        ...(source.slave_address != null ? [`${pad}slave_address: ${source.slave_address}`] : []),
        `${pad}pdo_index: ${source.pdo_index}`,
        `${pad}entry_index: ${source.entry_index}`,
        `${pad}sub_index: ${source.sub_index}`,
        `${pad}bit_len: ${source.bit_len}`,
      ].join('\n');
    default:
      return `${pad}type: ${type}`;
  }
}
