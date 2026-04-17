import { describe, expect, it } from 'vitest';
import { buildScriptGenerationPrompt, type NodeContext } from '../script-generation';

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
