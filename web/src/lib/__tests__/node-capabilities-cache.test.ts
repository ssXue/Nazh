import { describe, it, expect, vi, beforeEach } from 'vitest';

// mock tauri
vi.mock('../../lib/tauri', () => ({
  hasTauriRuntime: vi.fn(() => true),
  listNodeTypes: vi.fn(() =>
    Promise.resolve({
      types: [
        { name: 'timer', capabilities: 16 },
        { name: 'if', capabilities: 32 },
        { name: 'httpClient', capabilities: 2 },
      ],
    }),
  ),
}));

import { getCachedCapabilities, refreshCapabilitiesCache } from '../node-capabilities-cache';

describe('node-capabilities-cache', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it('缓存空时返回 undefined', () => {
    expect(getCachedCapabilities('timer')).toBeUndefined();
  });

  it('refresh 后可按 nodeType 查询', async () => {
    await refreshCapabilitiesCache();
    expect(getCachedCapabilities('timer')).toBe(16);
    expect(getCachedCapabilities('if')).toBe(32);
    expect(getCachedCapabilities('httpClient')).toBe(2);
    expect(getCachedCapabilities('unknown')).toBeUndefined();
  });
});