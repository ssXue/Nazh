import type { FlowNodeEntity } from '@flowgram.ai/free-layout-editor';
import { describe, expect, it, vi } from 'vitest';
import {
  buildScriptGenerationPrompt,
  generateScript,
  getNodeContext,
  type NodeContext,
} from '../script-generation';

vi.mock('../../lib/tauri', () => ({
  copilotComplete: vi.fn(),
}));

describe('buildScriptGenerationPrompt', () => {
  it('生成包含 system 和 user 两条消息', () => {
    const context: NodeContext = {
      current: { nodeType: 'code', label: '数据转换', aiDescription: '将温度值转为华氏度' },
      upstream: [{ nodeType: 'native', label: '传感器输入', aiDescription: '读取 Modbus 温度' }],
      downstream: [{ nodeType: 'httpClient', label: '上报数据', aiDescription: '' }],
    };
    const messages = buildScriptGenerationPrompt('将摄氏温度转为华氏温度', context);
    expect(messages).toHaveLength(2);
    expect(messages[0].role).toBe('system');
    expect(messages[0].content).toContain('Rhai');
    expect(messages[0].content).toContain('payload');
    expect(messages[0].content).not.toContain('ctx.payload');
    expect(messages[1].role).toBe('user');
    expect(messages[1].content).toContain('数据转换');
    expect(messages[1].content).toContain('传感器输入');
    expect(messages[1].content).toContain('上报数据');
    expect(messages[1].content).toContain('将摄氏温度转为华氏温度');
  });

  it('无上下游时输出"无"', () => {
    const context: NodeContext = {
      current: { nodeType: 'code', label: '独立节点', aiDescription: '' },
      upstream: [],
      downstream: [],
    };
    const messages = buildScriptGenerationPrompt('空脚本', context);
    expect(messages[1].content).toContain('上游节点：\n  无');
    expect(messages[1].content).toContain('下游节点：\n  无');
  });

  it('节点描述为空时显示"无"', () => {
    const context: NodeContext = {
      current: { nodeType: 'rhai', label: '测试节点', aiDescription: '' },
      upstream: [],
      downstream: [],
    };
    const messages = buildScriptGenerationPrompt('测试需求', context);
    expect(messages[1].content).toContain('节点描述：无');
  });

  it('上游节点含 aiDescription 时包含描述信息', () => {
    const context: NodeContext = {
      current: { nodeType: 'code', label: '处理节点', aiDescription: '处理数据' },
      upstream: [{ nodeType: 'native', label: '输入', aiDescription: '读取传感器' }],
      downstream: [],
    };
    const messages = buildScriptGenerationPrompt('需求', context);
    expect(messages[1].content).toContain('描述: 读取传感器');
  });

  it('上游节点无 aiDescription 时不包含描述字段', () => {
    const context: NodeContext = {
      current: { nodeType: 'code', label: '处理节点', aiDescription: '' },
      upstream: [{ nodeType: 'native', label: '输入', aiDescription: '' }],
      downstream: [],
    };
    const messages = buildScriptGenerationPrompt('需求', context);
    expect(messages[1].content).not.toContain('描述:');
  });
});

describe('getNodeContext', () => {
  function createNode(
    id: string,
    nodeType: string,
    overrides?: { label?: string; aiDescription?: string | null },
  ): FlowNodeEntity {
    return {
      id,
      flowNodeType: nodeType,
      getExtInfo: () => ({
        nodeType,
        label: overrides?.label,
        aiDescription: overrides?.aiDescription,
      }),
      lines: {
        inputNodes: [],
        outputNodes: [],
      },
    } as unknown as FlowNodeEntity;
  }

  it('提取当前节点及上下游节点信息', () => {
    const upstream = createNode('upstream', 'native', {
      label: '采集输入',
      aiDescription: '读取现场数据',
    });
    const downstream = createNode('downstream', 'httpClient', {
      label: '告警发送',
      aiDescription: '发送 webhook',
    });
    const current = createNode('current', 'code', {
      label: '数据清洗',
      aiDescription: '归一化字段',
    });
    (current.lines as { inputNodes: FlowNodeEntity[]; outputNodes: FlowNodeEntity[] }).inputNodes = [
      upstream,
    ];
    (current.lines as { inputNodes: FlowNodeEntity[]; outputNodes: FlowNodeEntity[] }).outputNodes = [
      downstream,
    ];

    expect(getNodeContext(current)).toEqual({
      current: {
        nodeType: 'code',
        label: '数据清洗',
        aiDescription: '归一化字段',
      },
      upstream: [
        {
          nodeType: 'native',
          label: '采集输入',
          aiDescription: '读取现场数据',
        },
      ],
      downstream: [
        {
          nodeType: 'httpClient',
          label: '告警发送',
          aiDescription: '发送 webhook',
        },
      ],
    });
  });
});

describe('generateScript', () => {
  const mockContext: NodeContext = {
    current: { nodeType: 'code', label: '测试', aiDescription: '' },
    upstream: [],
    downstream: [],
  };

  it('调用 copilotComplete 并返回修剪后的内容', async () => {
    const { copilotComplete } = await import('../../lib/tauri');
    const mocked = vi.mocked(copilotComplete);
    mocked.mockResolvedValueOnce({
      content: '```rhai\npayload["value"] = 1;\npayload\n```',
      model: 'test',
      usage: undefined,
    });

    const result = await generateScript('需求', mockContext, {
      providerId: 'test-provider',
      model: 'deepseek-chat',
      params: {
        temperature: 0.2,
        maxTokens: 512,
        topP: 0.9,
      },
    });

    expect(result).toBe('payload["value"] = 1;\npayload');
    expect(mocked).toHaveBeenCalledTimes(1);
    const request = mocked.mock.calls[0][0];
    expect(request.providerId).toBe('test-provider');
    expect(request.model).toBe('deepseek-chat');
    expect(request.messages).toHaveLength(2);
    expect(request.params.temperature).toBe(0.2);
    expect(request.params.maxTokens).toBe(512);
    expect(request.params.topP).toBe(0.9);
    expect(request.timeoutMs).toBe(BigInt(60000));
  });

  it('未显式传参时回退到默认 copilot 参数', async () => {
    const { copilotComplete } = await import('../../lib/tauri');
    const mocked = vi.mocked(copilotComplete);
    mocked.mockResolvedValueOnce({ content: 'payload', model: 'test', usage: undefined });

    await generateScript('需求', mockContext, { providerId: 'test-provider' });

    const lastCall = mocked.mock.calls[mocked.mock.calls.length - 1];
    const request = lastCall?.[0];
    expect(request?.params).toEqual({
      temperature: 0.7,
      maxTokens: 2048,
      topP: 1,
    });
  });

  it('抛出异常时向上传播', async () => {
    const { copilotComplete } = await import('../../lib/tauri');
    const mocked = vi.mocked(copilotComplete);
    mocked.mockRejectedValueOnce(new Error('连接失败'));

    await expect(
      generateScript('需求', mockContext, { providerId: 'p' }),
    ).rejects.toThrow('连接失败');
  });
});
