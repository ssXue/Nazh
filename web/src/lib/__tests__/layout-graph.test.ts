// layoutGraph 单元测试
import { describe, expect, it } from 'vitest';
import type { WorkflowGraph } from '../../types';
import { layoutGraph } from '../graph';

/** 构造一个最小 WorkflowNodeDefinition 存根。 */
function makeNode(type: string) {
  return { id: '', type, config: null, buffer: 0 } as unknown as WorkflowGraph['nodes'][string];
}

describe('layoutGraph', () => {
  it('线性链（A→B→C）→ 各节点层级依次为 0、1、2', () => {
    const graph = {
      connections: [],
      nodes: {
        A: makeNode('native'),
        B: makeNode('code'),
        C: makeNode('native'),
      },
      edges: [
        { from: 'A', to: 'B' },
        { from: 'B', to: 'C' },
      ],
    } as WorkflowGraph;

    const positioned = layoutGraph(graph);
    const byId = Object.fromEntries(positioned.map((n) => [n.id, n]));

    expect(byId['A'].layer).toBe(0);
    expect(byId['B'].layer).toBe(1);
    expect(byId['C'].layer).toBe(2);
  });

  it('分叉 DAG（A→B，A→C）→ B 与 C 处于同一层级', () => {
    const graph = {
      connections: [],
      nodes: {
        A: makeNode('native'),
        B: makeNode('code'),
        C: makeNode('code'),
      },
      edges: [
        { from: 'A', to: 'B' },
        { from: 'A', to: 'C' },
      ],
    } as WorkflowGraph;

    const positioned = layoutGraph(graph);
    const byId = Object.fromEntries(positioned.map((n) => [n.id, n]));

    expect(byId['A'].layer).toBe(0);
    expect(byId['B'].layer).toBe(byId['C'].layer);
    expect(byId['B'].layer).toBe(1);
  });

  it('孤立节点（无边）→ 层级为 0', () => {
    const graph = {
      connections: [],
      nodes: {
        solo: makeNode('native'),
      },
      edges: [],
    } as WorkflowGraph;

    const positioned = layoutGraph(graph);
    expect(positioned).toHaveLength(1);
    expect(positioned[0].layer).toBe(0);
    expect(positioned[0].id).toBe('solo');
  });

  it('节点的 type 字段从节点定义中正确读取', () => {
    const graph = {
      connections: [],
      nodes: {
        script_node: makeNode('code'),
      },
      edges: [],
    } as WorkflowGraph;

    const positioned = layoutGraph(graph);
    expect(positioned[0].type).toBe('code');
  });

  it('菱形 DAG（A→B，A→C，B→D，C→D）→ D 层级最大', () => {
    const graph = {
      connections: [],
      nodes: {
        A: makeNode('native'),
        B: makeNode('code'),
        C: makeNode('code'),
        D: makeNode('native'),
      },
      edges: [
        { from: 'A', to: 'B' },
        { from: 'A', to: 'C' },
        { from: 'B', to: 'D' },
        { from: 'C', to: 'D' },
      ],
    } as WorkflowGraph;

    const positioned = layoutGraph(graph);
    const byId = Object.fromEntries(positioned.map((n) => [n.id, n]));

    expect(byId['A'].layer).toBe(0);
    expect(byId['D'].layer).toBe(2);
  });

  it('返回结果数量与节点数量相同', () => {
    const graph = {
      connections: [],
      nodes: {
        X: makeNode('native'),
        Y: makeNode('code'),
        Z: makeNode('native'),
      },
      edges: [{ from: 'X', to: 'Y' }],
    } as WorkflowGraph;

    const positioned = layoutGraph(graph);
    expect(positioned).toHaveLength(3);
  });
});
