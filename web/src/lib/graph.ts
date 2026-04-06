import type { WorkflowGraph } from '../types';

export interface PositionedNode {
  id: string;
  type: string;
  layer: number;
  row: number;
}

export function parseWorkflowGraph(source: string): {
  graph: WorkflowGraph | null;
  error: string | null;
} {
  try {
    const parsed = JSON.parse(source) as WorkflowGraph;
    if (!parsed.nodes || typeof parsed.nodes !== 'object') {
      return {
        graph: null,
        error: 'AST 缺少 nodes 字典。',
      };
    }

    return {
      graph: parsed,
      error: null,
    };
  } catch (error) {
    return {
      graph: null,
      error: error instanceof Error ? error.message : '未知解析错误',
    };
  }
}

export function layoutGraph(graph: WorkflowGraph): PositionedNode[] {
  const nodeIds = Object.keys(graph.nodes);
  const incoming = new Map<string, number>();
  const outgoing = new Map<string, string[]>();
  const layerMap = new Map<string, number>();

  for (const nodeId of nodeIds) {
    incoming.set(nodeId, 0);
    outgoing.set(nodeId, []);
  }

  for (const edge of graph.edges ?? []) {
    outgoing.set(edge.from, [...(outgoing.get(edge.from) ?? []), edge.to]);
    incoming.set(edge.to, (incoming.get(edge.to) ?? 0) + 1);
  }

  const queue = nodeIds.filter((nodeId) => (incoming.get(nodeId) ?? 0) === 0);
  for (const nodeId of queue) {
    layerMap.set(nodeId, 0);
  }

  while (queue.length > 0) {
    const current = queue.shift();
    if (!current) {
      continue;
    }

    const nextLayer = (layerMap.get(current) ?? 0) + 1;
    for (const neighbor of outgoing.get(current) ?? []) {
      layerMap.set(neighbor, Math.max(layerMap.get(neighbor) ?? 0, nextLayer));
      incoming.set(neighbor, (incoming.get(neighbor) ?? 1) - 1);
      if ((incoming.get(neighbor) ?? 0) === 0) {
        queue.push(neighbor);
      }
    }
  }

  const rowsByLayer = new Map<number, number>();

  return nodeIds.map((nodeId) => {
    const layer = layerMap.get(nodeId) ?? 0;
    const row = rowsByLayer.get(layer) ?? 0;
    rowsByLayer.set(layer, row + 1);

    return {
      id: nodeId,
      type: graph.nodes[nodeId]?.type ?? 'unknown',
      layer,
      row,
    };
  });
}
