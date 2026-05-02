// toNazhWorkflowGraph 单元测试
import { describe, expect, it } from 'vitest';
import { getAllNodeDefinitions } from '../../components/flowgram/flowgram-node-library';
import { toFlowgramWorkflowJson, toNazhWorkflowGraph } from '../flowgram';
import type { WorkflowGraph } from '../../types';

/**
 * 构造最小测试用工作流图。
 */
function baseGraph(): WorkflowGraph {
  return {
    nodes: {
      a: { type: 'native', config: { message: 'hello' } },
      b: {
        type: 'code',
        config: {
          script: 'payload["reply"] = ai_complete("hello"); payload',
          ai: {
            providerId: 'deepseek',
            model: 'deepseek-v4-flash',
            systemPrompt: '你是测试助手',
            temperature: 0.2,
            maxTokens: 256,
            topP: 0.9,
            thinking: { type: 'enabled' },
            reasoningEffort: 'high',
            timeoutMs: 4000,
          },
        },
      },
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
    expect(result.nodes['b']?.type).toBe('code');
  });

  it('所有可执行节点定义都会保存回 Nazh 图', () => {
    const executableKinds = getAllNodeDefinitions()
      .map((definition) => definition.kind)
      .filter((kind) => kind !== 'subgraph');
    const graph: WorkflowGraph = {
      nodes: Object.fromEntries(
        executableKinds.map((kind, index) => [
          `${kind}_${index}`,
          { type: kind, config: {} },
        ]),
      ),
      edges: [],
    } as WorkflowGraph;

    const flowgramJson = toFlowgramWorkflowJson(graph);
    const result = toNazhWorkflowGraph(flowgramJson, graph);

    for (const [id, node] of Object.entries(graph.nodes)) {
      expect(result.nodes[id]?.type).toBe(node.type);
    }
  });

  it('未知运行时节点往返保存时保留原始 nodeType', () => {
    const graph: WorkflowGraph = {
      nodes: {
        camera: {
          type: 'opencv/detect',
          config: { model: 'surface-defect-v1' },
        },
        sink: {
          type: 'debugConsole',
          config: { label: 'plugin-output' },
        },
      },
      edges: [{ from: 'camera', to: 'sink' }],
    } as WorkflowGraph;

    const flowgramJson = toFlowgramWorkflowJson(graph);
    const result = toNazhWorkflowGraph(flowgramJson, graph);

    expect(result.nodes.camera?.type).toBe('opencv/detect');
    expect(result.nodes.camera?.config).toEqual({ model: 'surface-defect-v1' });
    expect(result.edges).toEqual([{ from: 'camera', to: 'sink' }]);
  });

  it('显式带 nodeType 的未知编辑器节点会保存为运行时节点', () => {
    const graph = baseGraph();
    const flowgramJson = toFlowgramWorkflowJson(graph);
    const pluginNode = {
      id: 'detector',
      type: 'opencv/detect',
      meta: { position: { x: 500, y: 160 } },
      data: {
        label: '缺陷检测',
        nodeType: 'opencv/detect',
        config: { model: 'surface-defect-v1' },
      },
    };
    const augmented = {
      ...flowgramJson,
      nodes: [...flowgramJson.nodes, pluginNode],
      edges: [
        ...flowgramJson.edges,
        { sourceNodeID: 'a', targetNodeID: 'detector' },
      ],
    };

    const result = toNazhWorkflowGraph(augmented, graph);

    expect(result.nodes.detector?.type).toBe('opencv/detect');
    expect(result.nodes.detector?.config).toEqual({ model: 'surface-defect-v1' });
    expect(result.edges).toContainEqual({ from: 'a', to: 'detector' });
  });

  it('保存 Code Node 时会保留为统一的 code 类型', () => {
    const graph: WorkflowGraph = {
      nodes: {
        code_1: {
          type: 'code',
          config: {
            script: 'payload',
            ai: {
              providerId: 'deepseek',
            },
          },
        },
      },
      edges: [],
    } as WorkflowGraph;

    const flowgramJson = toFlowgramWorkflowJson(graph);
    const result = toNazhWorkflowGraph(flowgramJson, graph);

    expect(result.nodes['code_1']?.type).toBe('code');
    expect(result.nodes['code_1']?.config).toEqual({
      script: 'payload',
    });
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

  it('从 previousGraph 继承节点级别的 config，并清理脚本节点本地 AI 配置', () => {
    const graph = baseGraph();
    const flowgramJson = toFlowgramWorkflowJson(graph);
    const result = toNazhWorkflowGraph(flowgramJson, graph);
    expect(result.nodes['a']?.config).toEqual({ message: 'hello' });
    expect(result.nodes['b']?.config).toEqual({
      script: 'payload["reply"] = ai_complete("hello"); payload',
    });
    expect((result.nodes['b']?.config as { ai?: unknown })?.ai).toBeUndefined();
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
