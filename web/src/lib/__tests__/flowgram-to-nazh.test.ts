// toNazhWorkflowGraph 单元测试
import { describe, expect, it } from 'vitest';
import { toFlowgramWorkflowJson, toNazhWorkflowGraph } from '../flowgram';
import type { WorkflowGraph } from '../../types';

/**
 * 构造最小测试用工作流图。
 */
function baseGraph(): WorkflowGraph {
  return {
    nodes: {
      a: { type: 'native', config: { message: 'hello' } },
      b: { type: 'rhai', config: { script: 'payload' } },
    },
    edges: [{ from: 'a', to: 'b' }],
  } as WorkflowGraph;
}

describe('toNazhWorkflowGraph', () => {
  it('往返转换后节点数量不变', () => {
    const graph = baseGraph();
    const flowgramJson = toFlowgramWorkflowJson(graph);
    const result = toNazhWorkflowGraph(flowgramJson, graph);
    expect(Object.keys(result.nodes)).toHaveLength(2);
  });

  it('往返转换后边数量不变', () => {
    const graph = baseGraph();
    const flowgramJson = toFlowgramWorkflowJson(graph);
    const result = toNazhWorkflowGraph(flowgramJson, graph);
    expect(result.edges).toHaveLength(1);
  });

  it('往返转换后节点类型被保留', () => {
    const graph = baseGraph();
    const flowgramJson = toFlowgramWorkflowJson(graph);
    const result = toNazhWorkflowGraph(flowgramJson, graph);
    expect(result.nodes['a']?.type).toBe('native');
    expect(result.nodes['b']?.type).toBe('rhai');
  });

  it('从 previousGraph 继承 name 字段', () => {
    const graph: WorkflowGraph = {
      ...baseGraph(),
      name: '测试工作流',
    };
    const flowgramJson = toFlowgramWorkflowJson(graph);
    const result = toNazhWorkflowGraph(flowgramJson, graph);
    expect(result.name).toBe('测试工作流');
  });

  it('保存回 Nazh 图时清空工程内 connections 字段', () => {
    const graph: WorkflowGraph = {
      ...baseGraph(),
      connections: [
        {
          id: 'conn-1',
          type: 'modbus',
          metadata: { host: '192.168.1.1', port: 502 },
        },
      ],
    } as WorkflowGraph;
    const flowgramJson = toFlowgramWorkflowJson(graph);
    const result = toNazhWorkflowGraph(flowgramJson, graph);
    expect(result.connections).toEqual([]);
  });

  it('result.editor_graph 等于传入的 flowgramJson', () => {
    const graph = baseGraph();
    const flowgramJson = toFlowgramWorkflowJson(graph);
    const result = toNazhWorkflowGraph(flowgramJson, graph);
    expect(result.editor_graph).toBe(flowgramJson);
  });

  it('从 previousGraph 继承节点级别的 config', () => {
    const graph = baseGraph();
    const flowgramJson = toFlowgramWorkflowJson(graph);
    const result = toNazhWorkflowGraph(flowgramJson, graph);
    // config 通过 flowgram data.config 传递，应与原始值一致
    expect(result.nodes['a']?.config).toEqual({ message: 'hello' });
    expect(result.nodes['b']?.config).toEqual({ script: 'payload' });
  });

  it('边的 from/to 映射正确还原为 Nazh 格式', () => {
    const graph = baseGraph();
    const flowgramJson = toFlowgramWorkflowJson(graph);
    const result = toNazhWorkflowGraph(flowgramJson, graph);
    expect(result.edges[0]?.from).toBe('a');
    expect(result.edges[0]?.to).toBe('b');
  });

  it('非业务节点（纯编辑器节点）不出现在 result.nodes 中', () => {
    const graph = baseGraph();
    const flowgramJson = toFlowgramWorkflowJson(graph);
    // 向 flowgramJson 注入一个无 nodeType 的编辑器专属节点
    const editorOnlyNode = {
      id: 'editor-comment',
      type: 'comment',
      meta: { position: { x: 500, y: 500 } },
      data: { text: '纯注释节点' },
    };
    const augmented = {
      ...flowgramJson,
      nodes: [...flowgramJson.nodes, editorOnlyNode],
    };
    const result = toNazhWorkflowGraph(augmented, graph);
    expect(result.nodes['editor-comment']).toBeUndefined();
  });
});
