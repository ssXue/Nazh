import { beforeEach, describe, expect, it, vi } from 'vitest';

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

import { loadAiAssetContext } from '../tauri';

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
