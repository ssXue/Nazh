// 子图展平逻辑单元测试
import { describe, expect, it } from 'vitest';
import { flattenSubgraphs, toNazhWorkflowGraph } from '../flowgram';
import type { WorkflowGraph } from '../../types';
import type { WorkflowJSON as FlowgramWorkflowJSON } from '@flowgram.ai/free-layout-editor';

function emptyGraph(): WorkflowGraph {
  return { nodes: {}, edges: [] };
}

/** 快捷构造普通业务节点 */
function bizNode(id: string, nodeType: string, config: Record<string, unknown> = {}) {
  return {
    id,
    type: nodeType,
    data: { nodeType, config, label: id, connectionId: null, timeoutMs: null },
    meta: { position: { x: 0, y: 0 } },
  };
}

/** 快捷构造 subgraph 容器节点 */
function subgraphNode(
  id: string,
  blocks: FlowgramWorkflowJSON['nodes'],
  edges: FlowgramWorkflowJSON['edges'] = [],
  parameterBindings?: Record<string, string | number | boolean>,
) {
  const data: Record<string, unknown> = { label: id };
  if (parameterBindings) {
    data.parameterBindings = parameterBindings;
  }
  return {
    id,
    type: 'subgraph',
    data,
    meta: { position: { x: 0, y: 0 } },
    blocks,
    edges,
  };
}

/** 快捷构造桥接节点 */
function bridgeInput(id: string) {
  return {
    id,
    type: 'subgraphInput',
    data: { nodeType: 'subgraphInput', config: {}, label: id, connectionId: null, timeoutMs: null },
    meta: { position: { x: 0, y: 0 } },
  };
}

function bridgeOutput(id: string) {
  return {
    id,
    type: 'subgraphOutput',
    data: { nodeType: 'subgraphOutput', config: {}, label: id, connectionId: null, timeoutMs: null },
    meta: { position: { x: 0, y: 0 } },
  };
}

/** 快捷构造边 */
function edge(from: string, to: string) {
  return { sourceNodeID: from, targetNodeID: to };
}

// ── 测试 1: 无子图直通 ──────────────────────────────────────────

describe('flattenSubgraphs', () => {
  it('无子图时输出与输入一致', () => {
    const input: FlowgramWorkflowJSON = {
      nodes: [bizNode('timer-1', 'timer'), bizNode('native-1', 'native')],
      edges: [edge('timer-1', 'native-1')],
    };

    const flat = flattenSubgraphs(input);

    expect(flat.nodes).toHaveLength(2);
    expect(flat.edges).toHaveLength(1);
    expect(flat.nodes.map((n) => n.id).sort()).toEqual(['native-1', 'timer-1']);
    expect(flat.edges[0]).toEqual(
      expect.objectContaining({ sourceNodeID: 'timer-1', targetNodeID: 'native-1' }),
    );
  });

  // ── 测试 2: 单层子图 ──────────────────────────────────────────

  it('单层子图展平：桥接节点重写外部边', () => {
    // 拓扑：timer-1 → subgraph(sg-in → code-1 → sg-out) → sql-1
    const sub = subgraphNode(
      'sub-1',
      [bridgeInput('sg-in'), bizNode('code-1', 'code'), bridgeOutput('sg-out')],
      [edge('sg-in', 'code-1'), edge('code-1', 'sg-out')],
    );

    const input: FlowgramWorkflowJSON = {
      nodes: [bizNode('timer-1', 'timer'), sub, bizNode('sql-1', 'sqlWriter')],
      edges: [edge('timer-1', 'sub-1'), edge('sub-1', 'sql-1')],
    };

    const flat = flattenSubgraphs(input);

    // 5 个节点：timer-1, sub-1/sg-in, sub-1/code-1, sub-1/sg-out, sql-1
    const ids = flat.nodes.map((n) => n.id).sort();
    expect(ids).toEqual(['sql-1', 'sub-1/code-1', 'sub-1/sg-in', 'sub-1/sg-out', 'timer-1']);

    // 4 条边：timer→sg-in, sg-in→code-1, code-1→sg-out, sg-out→sql
    const edgePairs = flat.edges.map((e) => `${e.sourceNodeID}->${e.targetNodeID}`).sort();
    expect(edgePairs).toEqual([
      'sub-1/code-1->sub-1/sg-out',
      'sub-1/sg-in->sub-1/code-1',
      'sub-1/sg-out->sql-1',
      'timer-1->sub-1/sg-in',
    ]);
  });

  // ── 测试 3: 嵌套子图（2 层） ──────────────────────────────────

  it('嵌套子图：双层前缀 outer/inner/node', () => {
    // 内层子图
    const inner = subgraphNode(
      'inner',
      [bridgeInput('in-in'), bizNode('native-1', 'native'), bridgeOutput('in-out')],
      [edge('in-in', 'native-1'), edge('native-1', 'in-out')],
    );

    // 外层子图
    const outer = subgraphNode(
      'outer',
      [bridgeInput('out-in'), inner, bridgeOutput('out-out')],
      [edge('out-in', 'inner'), edge('inner', 'out-out')],
    );

    const input: FlowgramWorkflowJSON = {
      nodes: [bizNode('timer-1', 'timer'), outer, bizNode('sql-1', 'sqlWriter')],
      edges: [edge('timer-1', 'outer'), edge('outer', 'sql-1')],
    };

    const flat = flattenSubgraphs(input);

    const ids = flat.nodes.map((n) => n.id);
    expect(ids).toContain('outer/inner/native-1');
    expect(ids).toContain('timer-1');
    expect(ids).toContain('sql-1');
  });

  // ── 测试 4: 深度限制（8 层） ──────────────────────────────────

  it('嵌套超过 8 层抛出错误', () => {
    // 递归构造 9 层嵌套子图
    function nestedSubgraph(depth: number): FlowgramWorkflowJSON['nodes'][number] {
      if (depth <= 0) {
        return bizNode('leaf', 'native');
      }
      const innerBlock = nestedSubgraph(depth - 1);
      return subgraphNode(`sg-${depth}`, [innerBlock], []);
    }

    // 9 层嵌套 = depth 9
    const root = nestedSubgraph(9);
    const input: FlowgramWorkflowJSON = {
      nodes: [root],
      edges: [],
    };

    expect(() => flattenSubgraphs(input)).toThrow('子图嵌套超过 8 层上限');
  });

  // ── 测试 5: 参数替换 ──────────────────────────────────────────

  it('参数绑定替换 config 中的 {{paramName}}', () => {
    const codeWithParam = bizNode('code-1', 'code', {
      host: '{{host}}',
      port: '{{port}}',
      label: 'static-value',
    });

    const sub = subgraphNode(
      'sub-1',
      [bridgeInput('sg-in'), codeWithParam, bridgeOutput('sg-out')],
      [edge('sg-in', 'code-1'), edge('code-1', 'sg-out')],
      { host: '192.168.1.10', port: 502 },
    );

    const input: FlowgramWorkflowJSON = {
      nodes: [sub],
      edges: [],
    };

    const flat = flattenSubgraphs(input);

    const codeNode = flat.nodes.find((n) => n.id === 'sub-1/code-1');
    expect(codeNode).toBeDefined();

    const config = (codeNode!.data as Record<string, unknown>)?.config as Record<string, unknown>;
    expect(config.host).toBe('192.168.1.10');
    expect(config.port).toBe('502');
    expect(config.label).toBe('static-value');
  });

  // ── 测试 6: 未绑定参数保留原值 ──────────────────────────────────

  it('未绑定参数保留 {{unbound}} 原值', () => {
    const codeWithParam = bizNode('code-1', 'code', {
      host: '{{bound}}',
      extra: '{{unbound}}',
    });

    const sub = subgraphNode(
      'sub-1',
      [bridgeInput('sg-in'), codeWithParam, bridgeOutput('sg-out')],
      [edge('sg-in', 'code-1'), edge('code-1', 'sg-out')],
      { bound: 'replaced' },
    );

    const input: FlowgramWorkflowJSON = {
      nodes: [sub],
      edges: [],
    };

    const flat = flattenSubgraphs(input);

    const codeNode = flat.nodes.find((n) => n.id === 'sub-1/code-1');
    const config = (codeNode!.data as Record<string, unknown>)?.config as Record<string, unknown>;
    expect(config.host).toBe('replaced');
    expect(config.extra).toBe('{{unbound}}');
  });

  // ── 测试 7: 空子图 ──────────────────────────────────────────

  it('空子图（无 blocks）不崩溃，产出 0 内部节点', () => {
    const sub = subgraphNode('empty-sub', [], []);

    const input: FlowgramWorkflowJSON = {
      nodes: [bizNode('timer-1', 'timer'), sub],
      edges: [],
    };

    const flat = flattenSubgraphs(input);

    // 只有 timer-1，子图无内部节点
    const ids = flat.nodes.map((n) => n.id);
    expect(ids).toEqual(['timer-1']);
  });

  // ── 测试 8: 无桥接节点的子图 ──────────────────────────────────

  it('无桥接节点的子图不崩溃（外部边可能被丢弃）', () => {
    // 子图有内部节点但没有 subgraphInput / subgraphOutput
    const sub = subgraphNode(
      'sub-1',
      [bizNode('code-1', 'code')],
      [],
    );

    const input: FlowgramWorkflowJSON = {
      nodes: [bizNode('timer-1', 'timer'), sub, bizNode('sql-1', 'sqlWriter')],
      edges: [edge('timer-1', 'sub-1'), edge('sub-1', 'sql-1')],
    };

    const flat = flattenSubgraphs(input);

    // 内部节点带前缀
    const ids = flat.nodes.map((n) => n.id).sort();
    expect(ids).toContain('sub-1/code-1');
    expect(ids).toContain('timer-1');
    expect(ids).toContain('sql-1');

    // 外部边连接到子图但没有桥接，两端不匹配 flatNodeIds，应被丢弃
    expect(flat.edges).toHaveLength(0);
  });
});

// ── toNazhWorkflowGraph 集成验证 ──────────────────────────────────

describe('toNazhWorkflowGraph 与子图展平', () => {
  it('展平后业务节点可正确转化为 Nazh 图', () => {
    const sub = subgraphNode(
      'sub-1',
      [bridgeInput('sg-in'), bizNode('code-1', 'code', { script: 'payload' }), bridgeOutput('sg-out')],
      [edge('sg-in', 'code-1'), edge('code-1', 'sg-out')],
    );

    const input: FlowgramWorkflowJSON = {
      nodes: [bizNode('timer-1', 'timer'), sub, bizNode('sql-1', 'sqlWriter')],
      edges: [edge('timer-1', 'sub-1'), edge('sub-1', 'sql-1')],
    };

    const result = toNazhWorkflowGraph(input, emptyGraph());

    // subgraphInput / subgraphOutput 是业务节点，应该出现在结果中
    expect(Object.keys(result.nodes).sort()).toEqual(
      ['sql-1', 'sub-1/code-1', 'sub-1/sg-in', 'sub-1/sg-out', 'timer-1'],
    );

    // 边正确
    const resultEdges = result.edges.map((e) => `${e.from}->${e.to}`).sort();
    expect(resultEdges).toEqual([
      'sub-1/code-1->sub-1/sg-out',
      'sub-1/sg-in->sub-1/code-1',
      'sub-1/sg-out->sql-1',
      'timer-1->sub-1/sg-in',
    ]);
  });
});
