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
        type: 'rhai',
        config: {
          script: 'payload["reply"] = ai_complete("hello"); payload',
          ai: {
            providerId: 'deepseek',
            model: 'deepseek-chat',
            systemPrompt: '你是测试助手',
            temperature: 0.2,
            maxTokens: 256,
            topP: 0.9,
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
    expect((nodeB?.data as { nodeType?: string })?.nodeType).toBe('rhai');
  });

  it('脚本节点的 AI 配置会被映射到 FlowGram data.config', () => {
    const result = toFlowgramWorkflowJson(baseGraph());
    const nodeB = result.nodes.find((n) => n.id === 'b');
    expect((nodeB?.data as { config?: { ai?: unknown } })?.config?.ai).toEqual({
      providerId: 'deepseek',
      model: 'deepseek-chat',
      systemPrompt: '你是测试助手',
      temperature: 0.2,
      maxTokens: 256,
      topP: 0.9,
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
});
