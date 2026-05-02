/**
 * FlowgramCanvas 辅助工具函数。
 *
 * 连接类型判断、导出文件名生成、坐标归一化、图签名计算等
 * 无 UI 依赖的纯函数，从 FlowgramCanvas.tsx 拆出以降低单文件复杂度。
 */

import type { WorkflowJSON as FlowgramWorkflowJSON } from '@flowgram.ai/free-layout-editor';
import type { WorkflowWindowStatus } from '../../types';

// ---------------------------------------------------------------------------
// 连接类型判断
// ---------------------------------------------------------------------------

/** 将连接类型归一化为小写。 */
function normalizedConnectionType(connectionType: string): string {
  return connectionType.trim().toLowerCase();
}

/** 判断是否为串口类连接。 */
export function isSerialConnectionType(connectionType: string): boolean {
  switch (normalizedConnectionType(connectionType)) {
    case 'serial':
    case 'serialport':
    case 'serial_port':
    case 'uart':
    case 'rs232':
    case 'rs485':
      return true;
    default:
      return false;
  }
}

/** 判断是否为 Modbus 类连接。 */
export function isModbusConnectionType(connectionType: string): boolean {
  switch (normalizedConnectionType(connectionType)) {
    case 'modbus':
    case 'modbus_tcp':
      return true;
    default:
      return false;
  }
}

/** 判断是否为 MQTT 连接。 */
export function isMqttConnectionType(connectionType: string): boolean {
  return normalizedConnectionType(connectionType) === 'mqtt';
}

/** 判断是否为 HTTP 类连接。 */
export function isHttpConnectionType(connectionType: string): boolean {
  switch (normalizedConnectionType(connectionType)) {
    case 'http':
    case 'http_sink':
      return true;
    default:
      return false;
  }
}

/** 判断是否为 Bark 类连接。 */
export function isBarkConnectionType(connectionType: string): boolean {
  switch (normalizedConnectionType(connectionType)) {
    case 'bark':
    case 'bark_push':
      return true;
    default:
      return false;
  }
}

// ---------------------------------------------------------------------------
// 导出文件名
// ---------------------------------------------------------------------------

/** 清洗导出文件名段，去除特殊字符。 */
export function sanitizeExportFileSegment(value: string): string {
  const normalized = value
    .trim()
    .toLowerCase()
    .replace(/[\\/:*?"<>|]+/g, '-')
    .replace(/\s+/g, '-')
    .replace(/-+/g, '-')
    .replace(/^-+|-+$/g, '');

  return normalized || 'flowgram';
}

/** 构造 Flowgram 导出文件名。 */
export function buildFlowgramExportFileName(
  workflowName: string | null | undefined,
  format: string,
): string {
  const baseName = sanitizeExportFileSegment(workflowName ?? 'flowgram-workflow');
  const timestamp = new Date()
    .toISOString()
    .replace(/[:]/g, '-')
    .replace(/\.\d{3}Z$/, 'Z');

  return `${baseName}-${timestamp}.${format}`;
}

// ---------------------------------------------------------------------------
// 坐标归一化与图签名
// ---------------------------------------------------------------------------

/** 归一化画布坐标值，保留两位小数。 */
export function normalizeCanvasCoordinate(value: unknown): number | null {
  if (typeof value !== 'number' || !Number.isFinite(value)) {
    return null;
  }

  return Math.round(value * 100) / 100;
}

/** 归一化 Flowgram 边数据。 */
export function normalizeFlowgramEdge(
  edge: FlowgramWorkflowJSON['edges'][number],
) {
  return {
    sourceNodeID: edge.sourceNodeID,
    targetNodeID: edge.targetNodeID,
    sourcePortID: edge.sourcePortID ?? null,
    targetPortID: edge.targetPortID ?? null,
  };
}

/** 归一化 Flowgram 值（递归排序 key + 坐标归一化）。 */
export function normalizeFlowgramValue(value: unknown): unknown {
  if (typeof value === 'number') {
    return normalizeCanvasCoordinate(value);
  }

  if (Array.isArray(value)) {
    return value.map(normalizeFlowgramValue);
  }

  if (!value || typeof value !== 'object') {
    return value ?? null;
  }

  return Object.keys(value)
    .sort((left, right) => left.localeCompare(right))
    .reduce<Record<string, unknown>>((acc, key) => {
      acc[key] = normalizeFlowgramValue((value as Record<string, unknown>)[key]);
      return acc;
    }, {});
}

/** 归一化后的 Flowgram 节点结构。 */
export interface NormalizedFlowgramNode {
  id: string;
  type: string | number;
  meta: unknown;
  data: unknown;
  blocks: NormalizedFlowgramNode[];
  edges: ReturnType<typeof normalizeFlowgramEdge>[];
}

/** 归一化单个 Flowgram 节点。 */
export function normalizeFlowgramNode(
  node: FlowgramWorkflowJSON['nodes'][number],
): NormalizedFlowgramNode {
  return {
    id: node.id,
    type: node.type,
    meta: normalizeFlowgramValue(node.meta ?? {}),
    data: normalizeFlowgramValue(node.data ?? {}),
    blocks: (node.blocks ?? [])
      .map(normalizeFlowgramNode)
      .sort((left, right) => left.id.localeCompare(right.id)),
    edges: (node.edges ?? [])
      .map(normalizeFlowgramEdge)
      .sort((left, right) => {
        const sourceCompare = left.sourceNodeID.localeCompare(right.sourceNodeID);
        if (sourceCompare !== 0) {
          return sourceCompare;
        }

        return left.targetNodeID.localeCompare(right.targetNodeID);
      }),
  };
}

/** 计算 Flowgram 图的稳定签名（用于变更检测）。 */
export function buildFlowgramGraphSignature(graph: FlowgramWorkflowJSON): string {
  const normalizedNodes = graph.nodes
    .map(normalizeFlowgramNode)
    .sort((left, right) => left.id.localeCompare(right.id));
  const normalizedEdges = [...graph.edges]
    .map(normalizeFlowgramEdge)
    .sort((left, right) => {
      const sourceCompare = left.sourceNodeID.localeCompare(right.sourceNodeID);
      return sourceCompare !== 0 ? sourceCompare : left.targetNodeID.localeCompare(right.targetNodeID);
    });

  return JSON.stringify({
    nodes: normalizedNodes,
    edges: normalizedEdges,
  });
}

// ---------------------------------------------------------------------------
// 交互模式与状态标签
// ---------------------------------------------------------------------------

export type FlowgramInteractiveType = 'MOUSE' | 'PAD';

const FLOWGRAM_INTERACTIVE_CACHE_KEY = 'workflow_prefer_interactive_type';

/** 判断当前平台是否为 Mac 类。 */
export function isMacLikePlatform() {
  if (typeof navigator === 'undefined') {
    return false;
  }

  return /(Macintosh|MacIntel|MacPPC|Mac68K|iPad)/.test(navigator.userAgent);
}

/** 读取持久化的交互模式偏好。 */
export function getPreferredInteractiveType(): FlowgramInteractiveType {
  if (typeof window === 'undefined') {
    return isMacLikePlatform() ? 'PAD' : 'MOUSE';
  }

  try {
    const stored = window.localStorage.getItem(FLOWGRAM_INTERACTIVE_CACHE_KEY);
    if (stored === 'MOUSE' || stored === 'PAD') {
      return stored;
    }
  } catch {
    // 忽略存储异常，回退到平台默认。
  }

  return isMacLikePlatform() ? 'PAD' : 'MOUSE';
}

/** 持久化交互模式偏好。 */
export function setPreferredInteractiveType(nextType: FlowgramInteractiveType) {
  if (typeof window === 'undefined') {
    return;
  }

  try {
    window.localStorage.setItem(FLOWGRAM_INTERACTIVE_CACHE_KEY, nextType);
  } catch {
    // 忽略存储异常。
  }
}

/** 获取工作流运行状态的人类可读标签。 */
export function getCanvasWorkflowStatusLabel(status: WorkflowWindowStatus): string {
  switch (status) {
    case 'preview':
      return '预览';
    case 'idle':
      return '未运行';
    case 'deployed':
      return '已部署';
    case 'running':
      return '运行中';
    case 'completed':
      return '已完成';
    case 'failed':
      return '失败';
  }
}

// ---------------------------------------------------------------------------
// 节点端口颜色
// ---------------------------------------------------------------------------

/** 根据节点展示类型解析端口主色。 */
export function resolveNodePortColor(
  displayType: string,
  accentHex: string,
  nodeCodeColor: string,
): string {
  switch (displayType) {
    case 'timer':
      return 'color-mix(in srgb, var(--accent) 55%, var(--warning) 45%)';
    case 'serialTrigger':
      return 'color-mix(in srgb, var(--accent) 64%, var(--success) 36%)';
    case 'modbusRead':
      return 'color-mix(in srgb, var(--accent) 58%, var(--success) 42%)';
    case 'if':
      return 'var(--success)';
    case 'switch':
      return 'var(--warning)';
    case 'tryCatch':
      return 'var(--danger)';
    case 'loop':
      return 'color-mix(in srgb, var(--accent) 72%, var(--success) 28%)';
    case 'httpClient':
      return 'color-mix(in srgb, var(--warning) 56%, var(--danger) 44%)';
    case 'barkPush':
      return 'color-mix(in srgb, var(--danger) 34%, var(--accent) 66%)';
    case 'sqlWriter':
      return 'color-mix(in srgb, var(--success) 68%, var(--accent) 32%)';
    case 'debugConsole':
      return 'var(--muted)';
    case 'subgraph':
      return 'var(--accent)';
    case 'subgraphInput':
    case 'subgraphOutput':
      return 'var(--muted)';
    case 'code':
      return nodeCodeColor;
    case 'native':
    default:
      return accentHex;
  }
}

// ---------------------------------------------------------------------------
// 错误描述
// ---------------------------------------------------------------------------

/** 将任意错误值转为可读字符串。 */
export function describeFlowgramError(error: unknown): string {
  if (error instanceof Error) {
    return error.stack ?? error.message;
  }

  if (typeof error === 'string') {
    return error;
  }

  try {
    return JSON.stringify(error);
  } catch {
    return '未知异常';
  }
}
