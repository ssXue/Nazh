import type { NazhNodeKind } from '../components/flowgram/flowgram-node-library';
import type { JsonValue } from '../types';
import { allocateNodeId } from './workflow-node-id';
import { normalizeWorkflowAiNodeKind } from './workflow-node-capabilities';

/// 调试日志开关——开发期间保持 true，上线后可关闭。
const DEBUG_PROTOCOL = true;

function protoLog(...args: unknown[]) {
  if (DEBUG_PROTOCOL) console.log('[copilot-protocol]', ...args);
}

function protoWarn(...args: unknown[]) {
  if (DEBUG_PROTOCOL) console.warn('[copilot-protocol]', ...args);
}

// --- 操作类型 ---

export interface ProjectMetadataOperation {
  type: 'project';
  name?: string;
  description?: string;
}

export interface CreateNodeOperation {
  type: 'create_node';
  ref: string;
  nodeType: NazhNodeKind;
  label?: string;
  connectionId?: string | null;
  config?: JsonValue;
}

export interface CreateEdgeOperation {
  type: 'create_edge';
  fromRef: string;
  toRef: string;
  sourcePortId?: string;
  targetPortId?: string;
}

export interface DoneOperation {
  type: 'done';
  summary?: string;
}

export type ProtocolOperation =
  | ProjectMetadataOperation
  | CreateNodeOperation
  | CreateEdgeOperation
  | DoneOperation;

// --- 会话状态 ---

export interface CopilotProtocolSession {
  nodeRefs: Record<string, string>;
  operations: ProtocolOperation[];
}

export function createInitialProtocolSession(): CopilotProtocolSession {
  return { nodeRefs: {}, operations: [] };
}

// --- JSON 解析辅助 ---

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null && !Array.isArray(value);
}

function hasOwnKey(value: Record<string, unknown>, key: string): boolean {
  return Object.prototype.hasOwnProperty.call(value, key);
}

function readString(record: Record<string, unknown>, keys: string[]): string | undefined {
  for (const key of keys) {
    const value = record[key];
    if (typeof value === 'string' && value.trim()) {
      return value.trim();
    }
  }
  return undefined;
}

// --- normalizeOperation ---

function normalizeOperation(input: unknown): ProtocolOperation | null {
  if (!isRecord(input)) {
    return null;
  }

  const rawType = readString(input, ['type', 'op', 'action', 'kind']);
  if (!rawType) {
    return null;
  }

  switch (rawType) {
    case 'project':
    case 'project_meta':
      return {
        type: 'project',
        name: readString(input, ['name']),
        description: readString(input, ['description', 'desc']),
      };

    case 'create_node': {
      const ref = readString(input, ['ref']);
      const nodeType = normalizeWorkflowAiNodeKind(
        readString(input, ['nodeType', 'node_type', 'kind']),
      );
      if (!ref || !nodeType) {
        return null;
      }
      const config = isRecord(input.config) || Array.isArray(input.config)
        ? (input.config as JsonValue)
        : undefined;
      const hasConnectionId =
        hasOwnKey(input, 'connectionId') || hasOwnKey(input, 'connection_id');
      const nextConnectionId = hasConnectionId
        ? readString(input, ['connectionId', 'connection_id']) ?? null
        : undefined;

      return {
        type: 'create_node',
        ref,
        nodeType,
        label: readString(input, ['label', 'title']),
        connectionId: nextConnectionId,
        config,
      };
    }

    case 'create_edge': {
      const fromRef = readString(input, ['fromRef']);
      const toRef = readString(input, ['toRef']);
      if (!fromRef || !toRef) {
        return null;
      }
      return {
        type: 'create_edge',
        fromRef,
        toRef,
        sourcePortId: readString(input, ['sourcePortId', 'source_port_id', 'sourcePort']),
        targetPortId: readString(input, ['targetPortId', 'target_port_id', 'targetPort']),
      };
    }

    case 'done':
    case 'complete':
      return {
        type: 'done',
        summary: readString(input, ['summary', 'message']),
      };

    default:
      return null;
  }
}

// --- parseOperationLine ---

export function parseOperationLine(line: string): ProtocolOperation | null {
  const trimmed = line.trim();
  if (!trimmed || trimmed === '```') {
    return null;
  }

  const normalizedLine = trimmed.replace(/^data:\s*/i, '');
  const startIndex = normalizedLine.indexOf('{');
  const endIndex = normalizedLine.lastIndexOf('}');
  if (startIndex === -1 || endIndex === -1 || endIndex <= startIndex) {
    protoWarn('parseOperationLine: 非 JSON 行，跳过', { line: trimmed.slice(0, 120) });
    return null;
  }

  const jsonStr = normalizedLine.slice(startIndex, endIndex + 1);
  let parsed: unknown;
  try {
    parsed = JSON.parse(jsonStr);
  } catch (err) {
    protoWarn('parseOperationLine: JSON 解析失败', { json: jsonStr.slice(0, 200), err });
    return null;
  }

  const op = normalizeOperation(parsed);
  if (op) {
    protoLog('parseOperationLine: ✓', op.type, op);
  } else {
    protoWarn('parseOperationLine: normalizeOperation 返回 null', { parsed });
  }
  return op;
}

// --- consumeOperationLines ---

export function consumeOperationLines(
  rawText: string,
  processedLength: number,
): {
  nextProcessedLength: number;
  operations: ProtocolOperation[];
} {
  const unprocessedText = rawText.slice(processedLength);
  if (!unprocessedText) {
    return { nextProcessedLength: processedLength, operations: [] };
  }

  const newlineMatches = [...unprocessedText.matchAll(/\r?\n/g)];
  const lastNewline = newlineMatches[newlineMatches.length - 1];
  if (!lastNewline || lastNewline.index === undefined) {
    // 尚无完整行（未出现换行符），记录剩余文本长度
    protoLog('consumeOperationLines: 等待换行', {
      unprocessedLen: unprocessedText.length,
      preview: unprocessedText.slice(-80),
    });
    return { nextProcessedLength: processedLength, operations: [] };
  }

  const consumedText = unprocessedText.slice(0, lastNewline.index + lastNewline[0].length);
  const completeLines = consumedText.split(/\r?\n/).filter((l) => l.trim().length > 0);

  protoLog('consumeOperationLines:', {
    rawLen: rawText.length,
    processedWas: processedLength,
    unprocessedLen: unprocessedText.length,
    lineCount: completeLines.length,
    nextProcessed: processedLength + consumedText.length,
  });

  const operations = completeLines
    .map((l) => parseOperationLine(l))
    .filter((op): op is ProtocolOperation => op !== null);

  if (operations.length > 0) {
    protoLog('consumeOperationLines: 发现操作', operations.map((o) => o.type));
  }

  return {
    nextProcessedLength: processedLength + consumedText.length,
    operations,
  };
}

/// 流结束时强制 flush 剩余未解析的行。
///
/// `consumeOperationLines` 只处理到最后一个 `\n` 为止，
/// 流结束后最后一行（通常是 `done`）可能没有尾部换行符。
/// 此函数在流结束时调用，用伪换行符触发最终解析。
export function flushRemainingProtocolLines(
  rawText: string,
  processedLength: number,
): {
  nextProcessedLength: number;
  operations: ProtocolOperation[];
} {
  const remaining = rawText.slice(processedLength).trim();
  if (!remaining) {
    return { nextProcessedLength: rawText.length, operations: [] };
  }

  protoLog('flushRemainingProtocolLines: 处理剩余文本', { remainingLen: remaining.length, preview: remaining.slice(0, 120) });

  // 将剩余文本视为完整行（可能包含多行无尾部换行的文本）
  const lines = remaining.split(/\r?\n/).filter((l) => l.trim().length > 0);
  const operations = lines
    .map((l) => parseOperationLine(l))
    .filter((op): op is ProtocolOperation => op !== null);

  if (operations.length > 0) {
    protoLog('flushRemainingProtocolLines: 发现剩余操作', operations.map((o) => o.type));
  } else {
    protoWarn('flushRemainingProtocolLines: 无有效操作', { lines: lines.length });
  }

  return { nextProcessedLength: rawText.length, operations };
}

// --- 会话状态应用 ---

export function protocolSessionApplyOp(
  session: CopilotProtocolSession,
  op: ProtocolOperation,
): CopilotProtocolSession {
  const nextNodeRefs = { ...session.nodeRefs };

  if (op.type === 'create_node') {
    const nodeId = nextNodeRefs[op.ref] ?? allocateNodeId();
    nextNodeRefs[op.ref] = nodeId;
    protoLog('protocolSessionApplyOp: create_node ref→nodeId', {
      ref: op.ref,
      nodeId,
      nodeType: op.nodeType,
      nodeRefs: { ...nextNodeRefs },
    });
  }

  return {
    nodeRefs: nextNodeRefs,
    operations: [...session.operations, op],
  };
}

// --- 展示文本 ---

export function buildOperationSummary(operations: ProtocolOperation[]): string {
  const parts: string[] = [];

  for (const op of operations) {
    switch (op.type) {
      case 'project':
        parts.push(`创建工程${op.name ? `「${op.name}」` : ''}`);
        break;
      case 'create_node':
        parts.push(`添加 ${op.label ?? op.nodeType}`);
        break;
      case 'create_edge':
        parts.push(`连接 ${op.fromRef} → ${op.toRef}`);
        break;
      case 'done':
        break;
    }
  }

  return parts.join(' → ');
}
