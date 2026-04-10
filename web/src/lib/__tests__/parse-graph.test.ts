// parseWorkflowGraph 单元测试
import { describe, expect, it } from 'vitest';
import { parseWorkflowGraph } from '../graph';

/** 最小合法工作流 AST JSON 字符串。 */
const VALID_AST = JSON.stringify({
  name: '测试工作流',
  connections: [],
  nodes: {
    node_a: {
      id: 'node_a',
      type: 'native',
      config: null,
      buffer: 0,
    },
  },
  edges: [],
});

describe('parseWorkflowGraph', () => {
  it('有效 JSON 且包含 nodes 字典 → 返回 graph，error 为 null', () => {
    const result = parseWorkflowGraph(VALID_AST);
    expect(result.error).toBeNull();
    expect(result.graph).not.toBeNull();
    expect(result.graph?.nodes).toHaveProperty('node_a');
  });

  it('缺少 nodes 字段 → 返回 error 信息含 "nodes"，graph 为 null', () => {
    const noNodes = JSON.stringify({ name: '无节点工作流', connections: [], edges: [] });
    const result = parseWorkflowGraph(noNodes);
    expect(result.graph).toBeNull();
    expect(result.error).not.toBeNull();
    expect(result.error).toContain('nodes');
  });

  it('nodes 字段为 null → 返回 error，graph 为 null', () => {
    const badNodes = JSON.stringify({ nodes: null, edges: [] });
    const result = parseWorkflowGraph(badNodes);
    expect(result.graph).toBeNull();
    expect(result.error).not.toBeNull();
    expect(result.error).toContain('nodes');
  });

  it('无效 JSON 字符串 → 返回解析错误，graph 为 null', () => {
    const result = parseWorkflowGraph('{ 这不是合法 JSON }');
    expect(result.graph).toBeNull();
    expect(result.error).not.toBeNull();
  });

  it('空字符串 → 返回错误，graph 为 null', () => {
    const result = parseWorkflowGraph('');
    expect(result.graph).toBeNull();
    expect(result.error).not.toBeNull();
  });

  it('解析成功时返回的 graph 包含原始 name 字段', () => {
    const result = parseWorkflowGraph(VALID_AST);
    expect(result.graph?.name).toBe('测试工作流');
  });
});
