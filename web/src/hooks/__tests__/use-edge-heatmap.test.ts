import { describe, expect, it } from 'vitest';

import {
  edgeHeatKeyFromParts,
  edgeHeatPairKey,
  findEdgeHeatEntry,
  type EdgeHeatMap,
} from '../use-edge-heatmap';

describe('edge heatmap helpers', () => {
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
});
