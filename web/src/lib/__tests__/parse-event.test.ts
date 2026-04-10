// parseWorkflowEventPayload 单元测试
import { describe, expect, it } from 'vitest';
import { parseWorkflowEventPayload } from '../workflow-events';

describe('parseWorkflowEventPayload', () => {
  it('解析 Started 事件，返回 kind=started', () => {
    const payload = { Started: { stage: 'node-a', trace_id: 'trace-001' } };
    const result = parseWorkflowEventPayload(payload);
    expect(result).toEqual({ kind: 'started', nodeId: 'node-a', traceId: 'trace-001' });
  });

  it('解析 Completed 事件，返回 kind=completed', () => {
    const payload = { Completed: { stage: 'node-b', trace_id: 'trace-002' } };
    const result = parseWorkflowEventPayload(payload);
    expect(result).toEqual({ kind: 'completed', nodeId: 'node-b', traceId: 'trace-002' });
  });

  it('解析 Failed 事件，返回 kind=failed 及 error 字段', () => {
    const payload = { Failed: { stage: 'node-c', trace_id: 'trace-003', error: '脚本超时' } };
    const result = parseWorkflowEventPayload(payload);
    expect(result).toEqual({
      kind: 'failed',
      nodeId: 'node-c',
      traceId: 'trace-003',
      error: '脚本超时',
    });
  });

  it('解析 Output 事件，返回 kind=output', () => {
    const payload = { Output: { stage: 'node-d', trace_id: 'trace-004' } };
    const result = parseWorkflowEventPayload(payload);
    expect(result).toEqual({ kind: 'output', nodeId: 'node-d', traceId: 'trace-004' });
  });

  it('输入 null 时返回 null', () => {
    expect(parseWorkflowEventPayload(null)).toBeNull();
  });

  it('输入字符串时返回 null', () => {
    expect(parseWorkflowEventPayload('Started')).toBeNull();
  });

  it('输入数字时返回 null', () => {
    expect(parseWorkflowEventPayload(42)).toBeNull();
  });

  it('输入空对象时返回 null', () => {
    expect(parseWorkflowEventPayload({})).toBeNull();
  });

  it('输入未知变体时返回 null', () => {
    const payload = { Unknown: { stage: 'node-x', trace_id: 'trace-999' } };
    expect(parseWorkflowEventPayload(payload)).toBeNull();
  });
});
