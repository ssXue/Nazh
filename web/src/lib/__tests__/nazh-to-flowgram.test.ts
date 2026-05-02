// toFlowgramWorkflowJson 单元测试
import { describe, expect, it } from 'vitest';
import { toFlowgramWorkflowJson } from '../flowgram';
import type { WorkflowGraph } from '../../types';

/**
 * 构造带有固定坐标的最小测试图，
 * 避免 layoutGraph 自动排列影响位置断言。
 */
function baseGraph(): WorkflowGraph {
  return {
    nodes: {
      a: {
        type: 'native',
        config: { message: 'hello' },
        meta: { position: { x: 0, y: 0 } },
      },
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
        meta: { position: { x: 320, y: 0 } },
      },
    },
    edges: [{ from: 'a', to: 'b' }],
  } as WorkflowGraph;
}

describe('toFlowgramWorkflowJson', () => {
  it('节点数量与源图一致', () => {
    const result = toFlowgramWorkflowJson(baseGraph());
    expect(result.nodes).toHaveLength(2);
  });

  it('边数量与源图一致', () => {
    const result = toFlowgramWorkflowJson(baseGraph());
    expect(result.edges).toHaveLength(1);
  });

  it('节点数据包含 nodeType 字段', () => {
    const result = toFlowgramWorkflowJson(baseGraph());
    const nodeA = result.nodes.find((n) => n.id === 'a');
    const nodeB = result.nodes.find((n) => n.id === 'b');
    expect((nodeA?.data as { nodeType?: string })?.nodeType).toBe('native');
    expect((nodeB?.data as { nodeType?: string })?.nodeType).toBe('code');
  });

  it('脚本节点的 AI 配置会被映射到 FlowGram data.config', () => {
    const result = toFlowgramWorkflowJson(baseGraph());
    const nodeB = result.nodes.find((n) => n.id === 'b');
    expect((nodeB?.data as { config?: { ai?: unknown } })?.config?.ai).toEqual({
      providerId: 'deepseek',
      model: 'deepseek-v4-flash',
      systemPrompt: '你是测试助手',
      temperature: 0.2,
      maxTokens: 256,
      topP: 0.9,
      thinking: { type: 'enabled' },
      reasoningEffort: 'high',
      timeoutMs: 4000,
    });
  });

  it('边将 from/to 映射为 sourceNodeID/targetNodeID', () => {
    const result = toFlowgramWorkflowJson(baseGraph());
    const edge = result.edges[0];
    expect(edge?.sourceNodeID).toBe('a');
    expect(edge?.targetNodeID).toBe('b');
  });

  it('保留 meta.position 坐标', () => {
    const result = toFlowgramWorkflowJson(baseGraph());
    const nodeA = result.nodes.find((n) => n.id === 'a');
    const nodeB = result.nodes.find((n) => n.id === 'b');
    expect(nodeA?.meta?.position).toEqual({ x: 0, y: 0 });
    expect(nodeB?.meta?.position).toEqual({ x: 320, y: 0 });
  });

  it('无 meta.position 时使用自动计算坐标', () => {
    const graph: WorkflowGraph = {
      nodes: {
        x: { type: 'native', config: {} },
      },
      edges: [],
    } as WorkflowGraph;
    const result = toFlowgramWorkflowJson(graph);
    const nodeX = result.nodes.find((n) => n.id === 'x');
    // 自动坐标应是数值，不应为 undefined
    expect(typeof nodeX?.meta?.position?.x).toBe('number');
    expect(typeof nodeX?.meta?.position?.y).toBe('number');
  });

  it('基础图生成 FlowGram JSON 时显示名称回退到节点类型默认名', () => {
    const graph: WorkflowGraph = {
      nodes: {
        loop_1: { type: 'loop', config: { script: '[payload]' } },
      },
      edges: [],
    } as WorkflowGraph;

    const result = toFlowgramWorkflowJson(graph);
    const node = result.nodes.find((item) => item.id === 'loop_1');

    expect((node?.data as { label?: string } | undefined)?.label).toBe('Loop Node');
  });

  it('未知运行时节点显示为原始 nodeType 而不是 Native', () => {
    const graph: WorkflowGraph = {
      nodes: {
        detector: { type: 'opencv/detect', config: { model: 'surface-defect-v1' } },
      },
      edges: [],
    } as WorkflowGraph;

    const result = toFlowgramWorkflowJson(graph);
    const node = result.nodes.find((item) => item.id === 'detector');
    const data = node?.data as { label?: string; nodeType?: string; displayType?: string } | undefined;

    expect(data?.label).toBe('opencv/detect');
    expect(data?.nodeType).toBe('opencv/detect');
    expect(data?.displayType).toBe('opencv/detect');
  });

  it('单节点无边图，边列表为空', () => {
    const graph: WorkflowGraph = {
      nodes: {
        solo: { type: 'native', config: {}, meta: { position: { x: 10, y: 20 } } },
      },
      edges: [],
    } as WorkflowGraph;
    const result = toFlowgramWorkflowJson(graph);
    expect(result.nodes).toHaveLength(1);
    expect(result.edges).toHaveLength(0);
  });

  it('保留 editor_graph 中的显示名称，同时用当前 graph 刷新运行字段', () => {
    const graph: WorkflowGraph = {
      nodes: {
        loop_1: {
          type: 'loop',
          config: { script: 'payload.map(|item| item)' },
          meta: { position: { x: 80, y: 120 } },
        },
      },
      edges: [],
      editor_graph: {
        nodes: [
          {
            id: 'loop_1',
            type: 'loop',
            meta: { position: { x: 10, y: 20 } },
            data: {
              label: '逐项处理',
              nodeType: 'loop',
              config: { script: '[payload]' },
            },
          },
        ],
        edges: [],
      },
    } as WorkflowGraph;

    const result = toFlowgramWorkflowJson(graph);
    const node = result.nodes.find((item) => item.id === 'loop_1');
    const data = node?.data as { label?: string; config?: { script?: string } } | undefined;

    expect(data?.label).toBe('逐项处理');
    expect(data?.config?.script).toBe('payload.map(|item| item)');
  });
});
