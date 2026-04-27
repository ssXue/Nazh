// reduceRuntimeState 单元测试
import { describe, expect, it } from 'vitest';
import {
  EMPTY_RUNTIME_STATE,
  reduceRuntimeState,
  type ParsedWorkflowEvent,
} from '../workflow-events';

/** 构造一个最小的 ParsedWorkflowEvent，仅覆盖需要的字段 */
function makeEvent(overrides: Partial<ParsedWorkflowEvent>): ParsedWorkflowEvent {
  return {
    kind: 'started',
    nodeId: 'node-a',
    traceId: 'trace-1',
    ...overrides,
  };
}

describe('reduceRuntimeState', () => {
  it('started 事件：将节点加入 activeNodeIds', () => {
    const event = makeEvent({ kind: 'started', nodeId: 'node-a' });
    const next = reduceRuntimeState(EMPTY_RUNTIME_STATE, event);
    expect(next.activeNodeIds).toContain('node-a');
    expect(next.completedNodeIds).not.toContain('node-a');
    expect(next.failedNodeIds).not.toContain('node-a');
  });

  it('completed 事件：将节点从 active 移至 completedNodeIds', () => {
    const started = reduceRuntimeState(EMPTY_RUNTIME_STATE, makeEvent({ kind: 'started', nodeId: 'node-a' }));
    const next = reduceRuntimeState(started, makeEvent({ kind: 'completed', nodeId: 'node-a' }));
    expect(next.activeNodeIds).not.toContain('node-a');
    expect(next.completedNodeIds).toContain('node-a');
  });

  it('failed 事件：记录 error 并将节点移至 failedNodeIds', () => {
    const started = reduceRuntimeState(EMPTY_RUNTIME_STATE, makeEvent({ kind: 'started', nodeId: 'node-b' }));
    const next = reduceRuntimeState(
      started,
      makeEvent({ kind: 'failed', nodeId: 'node-b', error: '连接超时' }),
    );
    expect(next.activeNodeIds).not.toContain('node-b');
    expect(next.failedNodeIds).toContain('node-b');
    expect(next.completedNodeIds).not.toContain('node-b');
    expect(next.lastError).toBe('连接超时');
  });

  it('output 事件：节点同时出现在 outputNodeIds 和 completedNodeIds', () => {
    const next = reduceRuntimeState(
      EMPTY_RUNTIME_STATE,
      makeEvent({ kind: 'output', nodeId: 'node-c' }),
    );
    expect(next.outputNodeIds).toContain('node-c');
    expect(next.completedNodeIds).toContain('node-c');
  });

  it('trace_id 切换：重置全部状态后处理新事件', () => {
    // 先在 trace-1 中 start node-a
    const stateTrace1 = reduceRuntimeState(
      EMPTY_RUNTIME_STATE,
      makeEvent({ kind: 'started', nodeId: 'node-a', traceId: 'trace-1' }),
    );
    expect(stateTrace1.activeNodeIds).toContain('node-a');

    // 新 trace-2 的事件触发重置
    const stateTrace2 = reduceRuntimeState(
      stateTrace1,
      makeEvent({ kind: 'started', nodeId: 'node-x', traceId: 'trace-2' }),
    );
    // trace-1 的 node-a 应已被清除
    expect(stateTrace2.activeNodeIds).not.toContain('node-a');
    expect(stateTrace2.activeNodeIds).toContain('node-x');
    expect(stateTrace2.traceId).toBe('trace-2');
  });

  it('多节点并发：依次 start a、start b、complete a', () => {
    let state = EMPTY_RUNTIME_STATE;
    state = reduceRuntimeState(state, makeEvent({ kind: 'started', nodeId: 'node-a' }));
    state = reduceRuntimeState(state, makeEvent({ kind: 'started', nodeId: 'node-b' }));
    state = reduceRuntimeState(state, makeEvent({ kind: 'completed', nodeId: 'node-a' }));

    expect(state.activeNodeIds).toEqual(['node-b']);
    expect(state.completedNodeIds).toContain('node-a');
    expect(state.activeNodeIds).not.toContain('node-a');
  });

  it('重复 started 不会重复添加到 activeNodeIds', () => {
    let state = reduceRuntimeState(EMPTY_RUNTIME_STATE, makeEvent({ kind: 'started', nodeId: 'node-a' }));
    state = reduceRuntimeState(state, makeEvent({ kind: 'started', nodeId: 'node-a' }));
    const count = state.activeNodeIds.filter((id) => id === 'node-a').length;
    expect(count).toBe(1);
  });

  it('finished 事件：清空 activeNodeIds，保留 completed/failed/output', () => {
    let state = reduceRuntimeState(EMPTY_RUNTIME_STATE, makeEvent({ kind: 'started', nodeId: 'node-a' }));
    state = reduceRuntimeState(state, makeEvent({ kind: 'started', nodeId: 'node-b' }));
    state = reduceRuntimeState(state, makeEvent({ kind: 'completed', nodeId: 'node-a' }));
    state = reduceRuntimeState(state, makeEvent({ kind: 'failed', nodeId: 'node-b', error: 'err' }));
    state = reduceRuntimeState(state, makeEvent({ kind: 'finished', nodeId: '' }));
    expect(state.activeNodeIds).toEqual([]);
    expect(state.completedNodeIds).toContain('node-a');
    expect(state.failedNodeIds).toContain('node-b');
    expect(state.lastEventType).toBe('finished');
  });
});
