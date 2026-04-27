import { describe, expect, it, vi, beforeEach } from 'vitest';

// Mock @tauri-apps/api/core invoke
const invokeMock = vi.fn();
vi.mock('@tauri-apps/api/core', () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));

const listenMock = vi.fn();
vi.mock('@tauri-apps/api/event', () => ({
  listen: (...args: unknown[]) => listenMock(...args),
}));

import {
  setWorkflowVariable,
  snapshotWorkflowVariables,
  onWorkflowVariableChanged,
} from '../workflow-variables';

describe('workflow-variables IPC wrappers', () => {
  beforeEach(() => {
    invokeMock.mockReset();
    listenMock.mockReset();
  });

  it('setWorkflowVariable 通过 invoke 调用 set_workflow_variable', async () => {
    const expected = {
      snapshot: {
        value: 25.0,
        variableType: { kind: 'float' },
        updatedAt: '2026-04-27T10:00:00Z',
        updatedBy: 'ipc',
      },
    };
    invokeMock.mockResolvedValue(expected);
    const result = await setWorkflowVariable({
      workflowId: 'wf-1',
      name: 'setpoint',
      value: 25.0,
    });
    expect(invokeMock).toHaveBeenCalledWith('set_workflow_variable', {
      request: { workflowId: 'wf-1', name: 'setpoint', value: 25.0 },
    });
    expect(result).toEqual(expected);
  });

  it('snapshotWorkflowVariables 通过 invoke 调用 snapshot_workflow_variables', async () => {
    invokeMock.mockResolvedValue({ variables: {} });
    const result = await snapshotWorkflowVariables('wf-1');
    expect(invokeMock).toHaveBeenCalledWith('snapshot_workflow_variables', {
      request: { workflowId: 'wf-1' },
    });
    expect(result).toEqual({ variables: {} });
  });

  it('onWorkflowVariableChanged 注册 listener 并返回 unlisten', async () => {
    const unlisten = vi.fn();
    listenMock.mockResolvedValue(unlisten);
    const handler = vi.fn();
    const result = await onWorkflowVariableChanged(handler);
    expect(listenMock).toHaveBeenCalledWith(
      'workflow://variable-changed',
      expect.any(Function),
    );
    expect(result).toBe(unlisten);
  });

  it('onWorkflowVariableChanged 调用 handler 时透传 payload', async () => {
    let registeredHandler: ((event: { payload: unknown }) => void) | undefined;
    listenMock.mockImplementation((_channel, h) => {
      registeredHandler = h;
      return Promise.resolve(() => {});
    });
    const handler = vi.fn();
    await onWorkflowVariableChanged(handler);
    const payload = {
      workflowId: 'wf-1',
      name: 'x',
      value: 1,
      updatedAt: '2026-04-27T10:00:00Z',
      updatedBy: 'node-A',
    };
    registeredHandler!({ payload });
    expect(handler).toHaveBeenCalledWith(payload);
  });
});
