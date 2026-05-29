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
  } catch {
    // fallback：generateText + JSON.parse + schema.parse
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

/// 从 AI 响应中提取 JSON（去除 markdown 代码块包裹）。
function extractJsonFromResponse(content: string): string {
  const trimmed = content.trim();
  const jsonMatch = trimmed.match(/^```(?:json)?\s*\n([\s\S]*?)\n?```$/);
  if (jsonMatch) return jsonMatch[1].trim();
  return trimmed;
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
const MAX_CORRECTION_ROUNDS = 2;

/// 四阶段抽取管道。
///
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
  // 截断过长文本
  let text = rawText;
  let truncated = false;
  if (text.length > MAX_TEXT_LENGTH) {
    text = text.slice(0, MAX_TEXT_LENGTH) + '\n\n[文件内容过长，已截断。如需完整信息请分批提供。]';
    truncated = true;
  }

  // Stage 1：大纲抽取
  const outline = await extractWithSchema(
    `从以下设备说明书中抽取设备大纲：\n---\n${text}\n---`,
    DeviceOutlineSchema,
    provider,
    OUTLINE_SYSTEM_PROMPT,
  );

  // Stage 2：信号填充
  const outlineContext = JSON.stringify(outline, null, 2);
  let signalDetail = await extractWithSchema(
    `设备大纲：\n${outlineContext}\n\n原始说明书：\n---\n${text}\n---\n\n请填充所有信号的完整细节。`,
    SignalDetailSchema,
    provider,
    SIGNAL_FILL_SYSTEM_PROMPT,
  );

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
  };
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
