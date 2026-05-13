// @vitest-environment jsdom

import { act, renderHook } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import {
  edgeHeatKeyFromParts,
  edgeHeatPairKey,
  findEdgeHeatEntry,
  useEdgeHeatmap,
  type EdgeHeatMap,
} from '../use-edge-heatmap';
import type { EdgeTransmitSummary } from '../../types';

function makeSummary(count = 1): EdgeTransmitSummary {
  return {
    from_node: 'source',
    from_pin: 'out',
    to_node: 'sink',
    to_pin: 'in',
    edge_kind: 'exec',
    transmit_count: count,
    max_queue_depth: 0,
    window_started_at: '2026-05-13T00:00:00Z',
    window_ended_at: '2026-05-13T00:00:00.100Z',
  };
}

describe('edge heatmap helpers', () => {
  beforeEach(() => {
    vi.useFakeTimers();
    vi.setSystemTime(new Date('2026-05-13T00:00:00Z'));
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  it('默认端口为空时归一化为 out/in', () => {
    expect(edgeHeatKeyFromParts('source', undefined, 'sink', undefined)).toBe(
      'source/out→sink/in',
    );
  });

  it('按精确端口匹配热力图 entry', () => {
    const heatmap: EdgeHeatMap = new Map([
      [
        'source/true→sink/in',
        {
          fromNode: 'source',
          fromPin: 'true',
          toNode: 'sink',
          toPin: 'in',
          transmitCount: 2,
          maxQueueDepth: 1,
          backpressure: false,
        },
      ],
    ]);

    expect(findEdgeHeatEntry(heatmap, 'source', 'true', 'sink', undefined)?.transmitCount).toBe(2);
  });

  it('端口缺省不一致时按节点对兜底匹配', () => {
    const entry = {
      fromNode: 'source',
      fromPin: 'out',
      toNode: 'sink',
      toPin: 'in',
      transmitCount: 1,
      maxQueueDepth: 0,
      backpressure: false,
    };
    const heatmap: EdgeHeatMap = new Map([[edgeHeatPairKey('source', 'sink'), entry]]);

    expect(findEdgeHeatEntry(heatmap, 'source', '', 'sink', '')).toBe(entry);
  });

  it('高频边事件会合并为限频 UI 通知', () => {
    const onUpdate = vi.fn();
    const { result } = renderHook(() => useEdgeHeatmap(onUpdate));

    act(() => {
      result.current.recordEdgeTransmit(makeSummary());
    });
    expect(onUpdate).toHaveBeenCalledTimes(1);

    act(() => {
      for (let i = 0; i < 20; i += 1) {
        result.current.recordEdgeTransmit(makeSummary());
      }
    });
    expect(onUpdate).toHaveBeenCalledTimes(1);

    act(() => {
      vi.advanceTimersByTime(99);
    });
    expect(onUpdate).toHaveBeenCalledTimes(1);

    act(() => {
      vi.advanceTimersByTime(1);
    });
    expect(onUpdate).toHaveBeenCalledTimes(2);

    const entry = findEdgeHeatEntry(result.current.getEdgeHeatmap(), 'source', 'out', 'sink', 'in');
    expect(entry?.transmitCount).toBe(21);
  });
});
