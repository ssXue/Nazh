// parseWorkflowEventPayload 单元测试
import { describe, expect, it } from 'vitest';
import { parseWorkflowEventPayload } from '../workflow-events';

describe('parseWorkflowEventPayload', () => {
  it('解析 Started 事件，返回 kind=started', () => {
    const payload = { Started: { stage: 'node-a', trace_id: 'trace-001' } };
    const result = parseWorkflowEventPayload(payload);
    expect(result).toEqual({ kind: 'started', nodeId: 'node-a', traceId: 'trace-001' });
  });

  it('解析 Completed 事件，返回 kind=completed 及 metadata', () => {
    const payload = { Completed: { stage: 'node-b', trace_id: 'trace-002' } };
    const result = parseWorkflowEventPayload(payload);
    expect(result).toEqual({ kind: 'completed', nodeId: 'node-b', traceId: 'trace-002', metadata: null });
  });

  it('解析 Completed 事件，透传 metadata', () => {
    const payload = {
      Completed: {
        stage: 'debug-1',
        trace_id: 'trace-debug',
        metadata: {
          debug_console: { label: '测试', pretty: true, rendered_payload: '{"x":1}' },
        },
      },
    };
    const result = parseWorkflowEventPayload(payload);
    expect(result).toEqual({
      kind: 'completed',
      nodeId: 'debug-1',
      traceId: 'trace-debug',
      metadata: payload.Completed.metadata,
    });
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

  it('解析 Finished 事件，返回 kind=finished', () => {
    const payload = { Finished: { trace_id: 'trace-005' } };
    const result = parseWorkflowEventPayload(payload);
    expect(result).toEqual({ kind: 'finished', nodeId: '', traceId: 'trace-005' });
  });

  it('解析 EdgeTransmitSummary 事件，透传边汇总载荷', () => {
    const payload = {
      EdgeTransmitSummary: {
        from_node: 'source',
        from_pin: 'out',
        to_node: 'sink',
        to_pin: 'in',
        edge_kind: 'exec',
        transmit_count: 3,
        max_queue_depth: 1,
        window_started_at: '2026-05-13T00:00:00Z',
        window_ended_at: '2026-05-13T00:00:00.100Z',
      },
    };
    const result = parseWorkflowEventPayload(payload);
    expect(result).toEqual({
      kind: 'edge-transmit-summary',
      nodeId: 'source',
      traceId: '',
      edgeTransmitSummary: payload.EdgeTransmitSummary,
    });
  });

  it('解析 BackpressureDetected 事件，透传背压载荷', () => {
    const payload = {
      BackpressureDetected: {
        at_node: 'sink',
        incoming_pin: 'in',
        channel_capacity: 64,
        channel_depth: 52,
        policy: 'block',
        dropped_since_last_report: 0,
        detected_at: '2026-05-13T00:00:00Z',
      },
    };
    const result = parseWorkflowEventPayload(payload);
    expect(result).toEqual({
      kind: 'backpressure-detected',
      nodeId: 'sink',
      traceId: '',
      backpressureDetected: payload.BackpressureDetected,
    });
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
