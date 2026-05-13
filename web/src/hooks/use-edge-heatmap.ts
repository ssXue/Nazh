//! ADR-0016 边热力图状态管理。
//!
//! 维护最近窗口内的边传输统计，供 FlowgramCanvas 边渲染使用。
//! 数据变更后通过 onUpdate 回调通知 Canvas 同步触发 FlowGram linesManager.forceUpdate()。
//! 边 key 使用 `from_node/from_pin→to_node/to_pin` 格式。

import { useCallback, useRef } from 'react';

import type { EdgeTransmitSummary, BackpressureDetected } from '../types';

const DEFAULT_SOURCE_PIN = 'out';
const DEFAULT_TARGET_PIN = 'in';

export interface EdgeHeatEntry {
  /** 源节点 ID。 */
  fromNode: string;
  /** 源端口 ID。 */
  fromPin: string;
  /** 目标节点 ID。 */
  toNode: string;
  /** 目标端口 ID。 */
  toPin: string;
  /** 最近窗口内的累计传输次数。 */
  transmitCount: number;
  /** 最近窗口内的最大队列深度。 */
  maxQueueDepth: number;
  /** 是否处于背压状态（收到 BackpressureDetected 且未过期）。 */
  backpressure: boolean;
}

export type EdgeHeatMap = Map<string, EdgeHeatEntry>;

function normalizePinId(pinId: string | number | null | undefined, fallback: string): string {
  if (typeof pinId === 'number') {
    return String(pinId);
  }
  return pinId?.trim() || fallback;
}

/** 构建边 key：`from_node/from_pin→to_node/to_pin`。 */
export function edgeHeatKeyFromParts(
  fromNode: string,
  fromPin: string | number | null | undefined,
  toNode: string,
  toPin: string | number | null | undefined,
): string {
  return `${fromNode}/${normalizePinId(fromPin, DEFAULT_SOURCE_PIN)}→${toNode}/${normalizePinId(toPin, DEFAULT_TARGET_PIN)}`;
}

/** 构建节点对 key：`from_node→to_node`，用于端口缺省或历史数据兜底。 */
export function edgeHeatPairKey(fromNode: string, toNode: string): string {
  return `${fromNode}→${toNode}`;
}

/** 构建边 key：`from_node/from_pin→to_node/to_pin`。 */
export function edgeHeatKey(summary: EdgeTransmitSummary): string;
export function edgeHeatKey(bp: BackpressureDetected): string;
export function edgeHeatKey(
  payload: EdgeTransmitSummary | BackpressureDetected,
): string {
  if ('from_node' in payload) {
    return edgeHeatKeyFromParts(payload.from_node, payload.from_pin, payload.to_node, payload.to_pin);
  }
  return `${payload.at_node}/${payload.incoming_pin}`;
}

/** 按 FlowGram 线条信息查找热力图 entry，优先精确端口，兜底节点对。 */
export function findEdgeHeatEntry(
  heatmap: EdgeHeatMap,
  fromNode: string,
  fromPin: string | number | null | undefined,
  toNode: string,
  toPin: string | number | null | undefined,
): EdgeHeatEntry | null {
  return (
    heatmap.get(edgeHeatKeyFromParts(fromNode, fromPin, toNode, toPin)) ??
    heatmap.get(edgeHeatPairKey(fromNode, toNode)) ??
    null
  );
}

export function useEdgeHeatmap(onUpdate?: () => void) {
  const heatRef = useRef<EdgeHeatMap>(new Map());
  const bpTimersRef = useRef<Map<string, ReturnType<typeof setTimeout>>>(new Map());
  const onUpdateRef = useRef(onUpdate);
  onUpdateRef.current = onUpdate;

  const notify = useCallback(() => {
    onUpdateRef.current?.();
  }, []);

  const recordEdgeTransmit = useCallback((summary: EdgeTransmitSummary) => {
    const key = edgeHeatKey(summary);
    const pairKey = edgeHeatPairKey(summary.from_node, summary.to_node);
    const existing = heatRef.current.get(key);
    const next: EdgeHeatEntry = {
      fromNode: summary.from_node,
      fromPin: normalizePinId(summary.from_pin, DEFAULT_SOURCE_PIN),
      toNode: summary.to_node,
      toPin: normalizePinId(summary.to_pin, DEFAULT_TARGET_PIN),
      transmitCount: (existing?.transmitCount ?? 0) + summary.transmit_count,
      maxQueueDepth: Math.max(existing?.maxQueueDepth ?? 0, summary.max_queue_depth),
      backpressure: existing?.backpressure ?? false,
    };
    heatRef.current.set(key, next);
    heatRef.current.set(pairKey, next);
    notify();
  }, [notify]);

  const recordBackpressure = useCallback((bp: BackpressureDetected) => {
    const needle = edgeHeatKey(bp);
    let changed = false;
    for (const [key, entry] of heatRef.current) {
      if (
        (entry.toNode === bp.at_node &&
          entry.toPin === normalizePinId(bp.incoming_pin, DEFAULT_TARGET_PIN)) ||
        key.includes(needle)
      ) {
        entry.backpressure = true;
        changed = true;
        const existingTimer = bpTimersRef.current.get(key);
        if (existingTimer) {
          clearTimeout(existingTimer);
        }
        bpTimersRef.current.set(
          key,
          setTimeout(() => {
            const e = heatRef.current.get(key);
            if (e) {
              e.backpressure = false;
            }
            bpTimersRef.current.delete(key);
            notify();
          }, 3000),
        );
      }
    }
    if (changed) {
      notify();
    }
  }, [notify]);

  const clearEdgeHeatmap = useCallback(() => {
    heatRef.current.clear();
    for (const timer of bpTimersRef.current.values()) {
      clearTimeout(timer);
    }
    bpTimersRef.current.clear();
    // 清空不需要触发 UI 更新（工作流已反部署）
  }, []);

  const getEdgeHeatmap = useCallback((): EdgeHeatMap => heatRef.current, []);

  return {
    recordEdgeTransmit,
    recordBackpressure,
    clearEdgeHeatmap,
    getEdgeHeatmap,
  };
}

/** 根据边传输次数计算热力图等级（0-4）。 */
export function edgeHeatLevel(transmitCount: number): number {
  if (transmitCount === 0) {
    return 0;
  }
  if (transmitCount <= 5) {
    return 1;
  }
  if (transmitCount <= 20) {
    return 2;
  }
  if (transmitCount <= 50) {
    return 3;
  }
  return 4;
}
