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
      current: { nodeId: 'cur', nodeType: 'code', label: '数据转换' },
      upstream: [{ nodeId: 'up', nodeType: 'native', label: '传感器输入' }],
      downstream: [{ nodeId: 'down', nodeType: 'httpClient', label: '上报数据' }],
    };
    const messages = buildScriptGenerationPrompt('将摄氏温度转为华氏温度', context);
    expect(messages).toHaveLength(2);
    expect(messages[0].role).toBe('system');
    expect(messages[0].content).toContain('Rhai');
    expect(messages[0].content).toContain('payload');
    expect(messages[0].content).toContain('自动解析');
    expect(messages[0].content).toContain('rand(min, max)');
    expect(messages[0].content).toContain('now_ms()');
    expect(messages[0].content).toContain('from_json(text)');
    expect(messages[0].content).toContain('to_json(value)');
    expect(messages[0].content).toContain('is_blank(text)');
    expect(messages[0].content).toContain('Math.random()');
    expect(messages[0].content).not.toContain('ctx.payload');
    expect(messages[1].role).toBe('user');
    expect(messages[1].content).toContain('数据转换');
    expect(messages[1].content).toContain('传感器输入');
    expect(messages[1].content).toContain('上报数据');
    expect(messages[1].content).toContain('将摄氏温度转为华氏温度');
  });

  it('无上下游时输出"无"', () => {
    const context: NodeContext = {
      current: { nodeId: 'solo', nodeType: 'code', label: '独立节点' },
      upstream: [],
      downstream: [],
    };
    const messages = buildScriptGenerationPrompt('空脚本', context);
    expect(messages[1].content).toContain('上游节点：\n  无');
    expect(messages[1].content).toContain('下游节点：\n  无');
  });

  it('pin schema 摘要出现在 prompt 让 LLM 拿到类型锚点', () => {
    const context: NodeContext = {
      current: {
        nodeId: 'cur',
        nodeType: 'code',
        label: '数据清洗',
        inputPins: [{ id: 'in', typeLabel: 'json', required: true, kind: 'exec' }],
        outputPins: [{ id: 'out', typeLabel: 'any', required: false, kind: 'exec' }],
      },
      upstream: [
        {
          nodeId: 'modbus',
          nodeType: 'modbusRead',
          label: '寄存器',
          outputPins: [{ id: 'out', typeLabel: 'json', required: false, kind: 'data' }],
        },
      ],
      downstream: [
        {
          nodeId: 'sql',
          nodeType: 'sqlWriter',
          label: '入库',
          inputPins: [{ id: 'in', typeLabel: 'json', required: true, kind: 'data' }],
        },
      ],
    };
    const messages = buildScriptGenerationPrompt('转换数据', context);
    const userText = messages[1].content;
    // 当前节点 pin（与上下游同样 inline 形态：端口：输入 [...] 输出 [...]）
    expect(userText).toContain('端口：输入 [in: exec/json (required)] 输出 [out: exec/any]');
    // 上游 / 下游 pin 内联展示
    expect(userText).toContain('类型: modbusRead）');
    expect(userText).toContain('out: data/json');
    expect(userText).toContain('类型: sqlWriter）');
    expect(userText).toContain('in: data/json (required)');
  });

  it('缺失 pin schema 时不输出端口信息（graceful degradation）', () => {
    const context: NodeContext = {
      current: { nodeId: 'cur', nodeType: 'code', label: '裸节点' },
      upstream: [],
      downstream: [],
    };
    const messages = buildScriptGenerationPrompt('需求', context);
    const userText = messages[1].content;
    expect(userText).not.toContain('端口：');
  });

  it('系统 prompt 含 PinKind 语义', () => {
    const messages = buildScriptGenerationPrompt('test', {
      current: {
        nodeId: 'code1',
        nodeType: 'code',
        label: '脚本节点',
        inputPins: [{ id: 'in', typeLabel: 'json', required: true, kind: 'exec' }],
        outputPins: [{ id: 'out', typeLabel: 'json', required: true, kind: 'exec' }],
      },
      upstream: [],
      downstream: [],
    });
    const system = messages[0].content;
    expect(system).toContain('求值语义');
    expect(system).toContain('exec');
    expect(system).toContain('data');
  });

  it('pin 描述含 PinKind 标记', () => {
    const messages = buildScriptGenerationPrompt('test', {
      current: {
        nodeId: 'code1',
        nodeType: 'code',
        label: '脚本节点',
        inputPins: [
          { id: 'in', typeLabel: 'json', required: true, kind: 'exec' },
          { id: 'sensor', typeLabel: 'float', required: false, kind: 'data' },
        ],
        outputPins: [
          { id: 'out', typeLabel: 'json', required: true, kind: 'exec' },
        ],
      },
      upstream: [],
      downstream: [],
    });
    const user = messages[1].content;
    expect(user).toContain('exec/json');
    expect(user).toContain('data/float');
  });
});

describe('getNodeContext', () => {
  function createNode(
    id: string,
    nodeType: string,
    overrides?: { label?: string },
  ): FlowNodeEntity {
    return {
      id,
      flowNodeType: nodeType,
      getExtInfo: () => ({
        nodeType,
        label: overrides?.label,
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
    });
    const downstream = createNode('downstream', 'httpClient', {
      label: '告警发送',
    });
    const current = createNode('current', 'code', {
      label: '数据清洗',
    });
    (current.lines as { inputNodes: FlowNodeEntity[]; outputNodes: FlowNodeEntity[] }).inputNodes = [
      upstream,
    ];
    (current.lines as { inputNodes: FlowNodeEntity[]; outputNodes: FlowNodeEntity[] }).outputNodes = [
      downstream,
    ];

    expect(getNodeContext(current)).toEqual({
      current: {
        nodeId: 'current',
        nodeType: 'code',
        label: '数据清洗',
        inputPins: undefined,
        outputPins: undefined,
      },
      upstream: [
        {
          nodeId: 'upstream',
          nodeType: 'native',
          label: '采集输入',
          inputPins: undefined,
          outputPins: undefined,
        },
      ],
      downstream: [
        {
          nodeId: 'downstream',
          nodeType: 'httpClient',
          label: '告警发送',
          inputPins: undefined,
          outputPins: undefined,
        },
      ],
    });
  });
});

describe('generateScript', () => {
  const mockContext: NodeContext = {
    current: { nodeId: 'test', nodeType: 'code', label: '测试' },
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
      model: 'deepseek-v4-flash',
      params: {
        temperature: 0.2,
        maxTokens: 512,
        topP: 0.9,
        thinking: { type: 'disabled' },
        reasoningEffort: 'high',
      },
    });

    expect(result).toBe('payload["value"] = 1;\npayload');
    expect(mocked).toHaveBeenCalledTimes(1);
    const request = mocked.mock.calls[0][0];
    expect(request.providerId).toBe('test-provider');
    expect(request.model).toBe('deepseek-v4-flash');
    expect(request.messages).toHaveLength(2);
    expect(request.params.temperature).toBe(0.2);
    expect(request.params.maxTokens).toBe(512);
    expect(request.params.topP).toBe(0.9);
    expect(request.params.thinking).toEqual({ type: 'disabled' });
    expect(request.params.reasoningEffort).toBe('high');
    expect(request.timeoutMs).toBe(60000);
    expect(() => JSON.stringify(request)).not.toThrow();
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
