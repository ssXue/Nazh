// ADR-0010 Phase 2：pin schema 缓存的单元测试。
//
// 覆盖 cache hit / IPC 失败 fallback / invalidate 三类路径。
// IPC 调用通过 vi.mock 隔离，不依赖真实 Tauri 环境。

import { beforeEach, describe, expect, it, vi } from 'vitest';

vi.mock('../tauri', () => ({
  describeNodePins: vi.fn(),
}));

import { describeNodePins } from '../tauri';
import {
  _resetCacheForTests,
  configToRecord,
  findPin,
  getCachedPinSchema,
  invalidateNodePinSchema,
  refreshNodePinSchema,
} from '../pin-schema-cache';

describe('pin-schema-cache', () => {
  beforeEach(() => {
    _resetCacheForTests();
    vi.mocked(describeNodePins).mockReset();
  });

  it('成功获取后能按 portId 查 pin', async () => {
    vi.mocked(describeNodePins).mockResolvedValueOnce({
      inputPins: [
        {
          id: 'in',
          label: 'in',
          pin_type: { kind: 'json' },
          direction: 'input',
          required: true,
        },
      ],
      outputPins: [],
    });
    await refreshNodePinSchema('sql-1', 'sqlWriter', {});
    expect(findPin('sql-1', 'in', 'input')?.pin_type).toEqual({ kind: 'json' });
  });

  it('IPC 失败时走 fallback（in/out 都是 any）', async () => {
    vi.mocked(describeNodePins).mockRejectedValueOnce(new Error('boom'));
    await refreshNodePinSchema('node-1', 'unknown', {});
    expect(findPin('node-1', 'in', 'input')?.pin_type).toEqual({ kind: 'any' });
    expect(findPin('node-1', 'out', 'output')?.pin_type).toEqual({ kind: 'any' });
  });

  it('invalidate 后查不到缓存', async () => {
    vi.mocked(describeNodePins).mockResolvedValueOnce({ inputPins: [], outputPins: [] });
    await refreshNodePinSchema('node-1', 'foo', {});
    expect(getCachedPinSchema('node-1')).toBeDefined();
    invalidateNodePinSchema('node-1');
    expect(getCachedPinSchema('node-1')).toBeUndefined();
    expect(findPin('node-1', 'in', 'input')).toBeUndefined();
  });

  it('refresh 同一节点会覆盖缓存（mqttClient 改 mode 场景）', async () => {
    // 先注入 publish 模式的 schema
    vi.mocked(describeNodePins).mockResolvedValueOnce({
      inputPins: [
        {
          id: 'in',
          label: 'in',
          pin_type: { kind: 'json' },
          direction: 'input',
          required: true,
        },
      ],
      outputPins: [
        { id: 'out', label: 'out', pin_type: { kind: 'any' }, direction: 'output', required: false },
      ],
    });
    await refreshNodePinSchema('mqtt-1', 'mqttClient', { mode: 'publish' });
    expect(findPin('mqtt-1', 'in', 'input')?.pin_type).toEqual({ kind: 'json' });
    expect(findPin('mqtt-1', 'out', 'output')?.pin_type).toEqual({ kind: 'any' });

    // 再注入 subscribe 模式的 schema（pin 镜像翻转）
    vi.mocked(describeNodePins).mockResolvedValueOnce({
      inputPins: [
        { id: 'in', label: 'in', pin_type: { kind: 'any' }, direction: 'input', required: true },
      ],
      outputPins: [
        {
          id: 'out',
          label: 'out',
          pin_type: { kind: 'json' },
          direction: 'output',
          required: false,
        },
      ],
    });
    await refreshNodePinSchema('mqtt-1', 'mqttClient', { mode: 'subscribe' });
    expect(findPin('mqtt-1', 'in', 'input')?.pin_type).toEqual({ kind: 'any' });
    expect(findPin('mqtt-1', 'out', 'output')?.pin_type).toEqual({ kind: 'json' });
  });

  it('查不存在的端口返回 undefined（不抛错）', async () => {
    vi.mocked(describeNodePins).mockResolvedValueOnce({
      inputPins: [
        { id: 'in', label: 'in', pin_type: { kind: 'json' }, direction: 'input', required: true },
      ],
      outputPins: [],
    });
    await refreshNodePinSchema('node-1', 'foo', {});
    expect(findPin('node-1', 'ghost', 'input')).toBeUndefined();
    expect(findPin('未存在', 'in', 'input')).toBeUndefined();
  });
});

describe('configToRecord', () => {
  it('对象 JsonValue 直接返回', () => {
    expect(configToRecord({ mode: 'publish', qos: 1 })).toEqual({ mode: 'publish', qos: 1 });
  });

  it('null/undefined/数组/标量 都退化为空对象', () => {
    expect(configToRecord(null)).toEqual({});
    expect(configToRecord(undefined)).toEqual({});
    expect(configToRecord([1, 2])).toEqual({});
    // 标量 JsonValue 也退化为空对象——TS 的 JsonValue 联合包含 string/number 字面
    expect(configToRecord('string' satisfies string)).toEqual({});
    expect(configToRecord(42)).toEqual({});
  });
});
