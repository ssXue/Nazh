import { beforeEach, describe, expect, it, vi } from 'vitest';

import type { AiCompletionRequest } from '../../types';

const { invokeMock, listenMock } = vi.hoisted(() => ({
  invokeMock: vi.fn(),
  listenMock: vi.fn(),
}));

vi.mock('@tauri-apps/api/core', () => ({
  invoke: invokeMock,
}));

vi.mock('@tauri-apps/api/event', () => ({
  listen: listenMock,
}));

vi.mock('@tauri-apps/api/window', () => ({
  currentMonitor: vi.fn(),
  getCurrentWindow: vi.fn(() => ({})),
  LogicalSize: class LogicalSize {
    constructor(public width: number, public height: number) {}
  },
}));

import { copilotCompleteStream, loadAiAssetContext } from '../tauri';

describe('loadAiAssetContext', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it('按当前工作路径读取 AI 资产上下文', async () => {
    const response = {
      devices: [],
      capabilities: [],
    };
    invokeMock.mockResolvedValueOnce(response);

    await expect(loadAiAssetContext('/tmp/nazh-workspace')).resolves.toEqual(response);
    expect(invokeMock).toHaveBeenCalledWith('load_ai_asset_context', {
      workspacePath: '/tmp/nazh-workspace',
    });
  });
});

describe('copilotCompleteStream', () => {
  const request: AiCompletionRequest = {
    providerId: 'test-provider',
    model: 'test-model',
    messages: [{ role: 'user', content: 'hello' }],
    params: {
      temperature: 0.3,
      maxTokens: 256,
      topP: 0.9,
    },
    timeoutMs: 5_000,
  };

  beforeEach(() => {
    vi.clearAllMocks();
  });

  it('先监听再启动流，并在完成后释放监听器', async () => {
    const callOrder: string[] = [];
    const unlisten = vi.fn();
    let handler:
      | ((event: {
          payload: { delta?: string; thinking?: string; done?: boolean; finishReason?: string };
        }) => void)
      | undefined;

    listenMock.mockImplementationOnce(async (eventName, callback) => {
      callOrder.push('listen');
      expect(String(eventName)).toContain('copilot://stream/');
      handler = callback;
      return unlisten;
    });

    invokeMock.mockImplementationOnce(async (command, payload) => {
      callOrder.push('invoke');
      expect(command).toBe('copilot_complete_stream');
      expect(payload.request).toEqual(request);
      expect(typeof payload.streamId).toBe('string');
      handler?.({ payload: { thinking: '思', delta: 'A' } });
      handler?.({ payload: { thinking: '考', delta: 'B', done: true, finishReason: 'stop' } });
    });

    const onDelta = vi.fn();
    const onThinking = vi.fn();
    const result = await copilotCompleteStream(request, onDelta, onThinking);

    expect(callOrder).toEqual(['listen', 'invoke']);
    expect(result).toEqual({
      text: 'AB',
      finishReason: 'stop',
    });
    expect(onThinking.mock.calls.map(([value]) => value)).toEqual(['思', '思考']);
    expect(onDelta.mock.calls.map(([value]) => value)).toEqual(['A', 'AB']);
    expect(unlisten).toHaveBeenCalledTimes(1);
  });

  it('启动失败时也会释放监听器', async () => {
    const unlisten = vi.fn();

    listenMock.mockResolvedValueOnce(unlisten);
    invokeMock.mockRejectedValueOnce(new Error('启动失败'));

    await expect(copilotCompleteStream(request, vi.fn())).rejects.toThrow('启动失败');
    expect(unlisten).toHaveBeenCalledTimes(1);
  });

  it('可恢复的流中断会自动重试一次', async () => {
    const unlistenFirst = vi.fn();
    const unlistenSecond = vi.fn();
    let firstHandler:
      | ((event: { payload: { error?: string } }) => void)
      | undefined;
    let secondHandler:
      | ((event: { payload: { delta?: string; done?: boolean; finishReason?: string } }) => void)
      | undefined;

    listenMock
      .mockImplementationOnce(async (_eventName, callback) => {
        firstHandler = callback;
        return unlistenFirst;
      })
      .mockImplementationOnce(async (_eventName, callback) => {
        secondHandler = callback;
        return unlistenSecond;
      });

    invokeMock
      .mockImplementationOnce(async () => {
        firstHandler?.({
          payload: {
            error: 'AI 网络错误: error decoding response body',
          },
        });
      })
      .mockImplementationOnce(async () => {
        secondHandler?.({
          payload: {
            delta: 'retry-ok',
            done: true,
            finishReason: 'stop',
          },
        });
      });

    const onDelta = vi.fn();
    const onRetryStart = vi.fn();
    const result = await copilotCompleteStream(request, onDelta, undefined, {
      maxRetries: 1,
      onRetryStart,
    });

    expect(onRetryStart).toHaveBeenCalledTimes(1);
    expect(onDelta.mock.calls.map(([value]) => value)).toEqual(['', 'retry-ok']);
    expect(result).toEqual({
      text: 'retry-ok',
      finishReason: 'stop',
    });
    expect(unlistenFirst).toHaveBeenCalledTimes(1);
    expect(unlistenSecond).toHaveBeenCalledTimes(1);
  });
});
