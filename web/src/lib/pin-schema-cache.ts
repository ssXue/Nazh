// ADR-0010 Phase 2：节点 pin schema 前端缓存。
//
// 节点添加到画布时主动调 IPC describe_node_pins 填缓存；
// 节点 config 保存时刷新；节点删除时清缓存。
// canAddLine 钩子从此查 (nodeId, portId) 的 pin schema。
//
// 缓存粒度按 nodeId（不是 nodeType + config hash）—— mqttClient 改 mode
// 后 pin 形态切换，必须按节点实例缓存才能区分。
//
// IPC 失败时缓存写 fallback Any/Any——UI 层宁可放过、不要误拒，部署期
// pin_validator 作为 backstop 兜底（defense in depth）。

import type { JsonValue, PinDefinition, PinType } from '../types';

import { describeNodePins } from './tauri';

interface CachedSchema {
  inputPins: PinDefinition[];
  outputPins: PinDefinition[];
}

const ANY_PIN_TYPE: PinType = { kind: 'any' };

const FALLBACK_SCHEMA: CachedSchema = {
  inputPins: [
    {
      id: 'in',
      label: 'in',
      pin_type: ANY_PIN_TYPE,
      direction: 'input',
      required: true,
    },
  ],
  outputPins: [
    {
      id: 'out',
      label: 'out',
      pin_type: ANY_PIN_TYPE,
      direction: 'output',
      required: false,
    },
  ],
};

const cache = new Map<string, CachedSchema>();

/**
 * 调 IPC describe_node_pins 拿当前节点的 pin schema 写入缓存。
 *
 * 失败时写 fallback Any/Any 并返回——不抛错，不阻断 UI 流程。
 * 调用时机：
 * 1. 节点首次添加到画布
 * 2. 节点 config 保存（mqttClient 改 mode 这种动态 pin 必须刷新）
 */
export async function refreshNodePinSchema(
  nodeId: string,
  nodeType: string,
  config: Record<string, unknown>,
): Promise<CachedSchema> {
  try {
    const response = await describeNodePins(nodeType, config);
    const schema: CachedSchema = {
      inputPins: response.inputPins,
      outputPins: response.outputPins,
    };
    cache.set(nodeId, schema);
    return schema;
  } catch (error) {
    console.warn(
      `[pin-cache] describe_node_pins 失败，节点 ${nodeId} (type=${nodeType}) 走 fallback Any/Any：`,
      error,
    );
    cache.set(nodeId, FALLBACK_SCHEMA);
    return FALLBACK_SCHEMA;
  }
}

/** 查节点的整套 pin schema，缓存未命中返回 undefined。 */
export function getCachedPinSchema(nodeId: string): CachedSchema | undefined {
  return cache.get(nodeId);
}

/**
 * 查节点指定方向的指定端口。
 *
 * 缓存未命中或端口不存在时返回 undefined。canAddLine 钩子收到 undefined
 * 时**放行**——优先 UX 体验，部署期 backstop 兜底。
 */
export function findPin(
  nodeId: string,
  portId: string | number,
  direction: 'input' | 'output',
): PinDefinition | undefined {
  const schema = cache.get(nodeId);
  if (!schema) return undefined;
  const pins = direction === 'input' ? schema.inputPins : schema.outputPins;
  const targetId = String(portId);
  return pins.find((pin) => pin.id === targetId);
}

/** 节点删除时清缓存。 */
export function invalidateNodePinSchema(nodeId: string): void {
  cache.delete(nodeId);
}

/**
 * 给端口着色用的 pin 类型 kind 字符串（缓存未命中或端口未声明时回退 `"any"`）。
 *
 * Phase 2 类型着色仅用 PinType 顶层 kind（'any' / 'json' / 'bool' / ...）
 * 决定颜色——`array` / `custom` 的 inner / name 不参与色彩区分。
 */
export function resolvePinTypeKind(
  nodeId: string,
  portId: string | number,
  direction: 'input' | 'output',
): string {
  return findPin(nodeId, portId, direction)?.pin_type.kind ?? 'any';
}

/** 测试钩子；生产路径不应直接调。 */
export function _resetCacheForTests(): void {
  cache.clear();
}

// 帮助调用方拼配置——很多场景拿到的是 JsonValue 而非 Record。
export function configToRecord(config: JsonValue | undefined | null): Record<string, unknown> {
  if (config && typeof config === 'object' && !Array.isArray(config)) {
    return config as Record<string, unknown>;
  }
  return {};
}
