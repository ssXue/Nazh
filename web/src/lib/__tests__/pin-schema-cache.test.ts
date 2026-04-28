// pin schema 缓存的单元测试。
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
  formatPinType,
  getCachedPinSchema,
  getPortTooltip,
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
          kind: 'exec',
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

  it('相同 (nodeType, config) 第二次刷新跳过 IPC（change-detection 守卫）', async () => {
    vi.mocked(describeNodePins).mockResolvedValue({
      inputPins: [
        { id: 'in', label: 'in', pin_type: { kind: 'json' }, direction: 'input', required: true, kind: 'exec' },
      ],
      outputPins: [],
    });
    await refreshNodePinSchema('node-1', 'sqlWriter', { table: 'logs' });
    await refreshNodePinSchema('node-1', 'sqlWriter', { table: 'logs' });
    await refreshNodePinSchema('node-1', 'sqlWriter', { table: 'logs' });
    expect(vi.mocked(describeNodePins)).toHaveBeenCalledTimes(1);
  });

  it('相同 nodeId 但 config 变化会重新 IPC（mqttClient 改 mode 场景）', async () => {
    // 先注入 publish 模式的 schema
    vi.mocked(describeNodePins).mockResolvedValueOnce({
      inputPins: [
        {
          id: 'in',
          label: 'in',
          pin_type: { kind: 'json' },
          direction: 'input',
          required: true,
          kind: 'exec',
        },
      ],
      outputPins: [
        { id: 'out', label: 'out', pin_type: { kind: 'any' }, direction: 'output', required: false, kind: 'exec' },
      ],
    });
    await refreshNodePinSchema('mqtt-1', 'mqttClient', { mode: 'publish' });
    expect(findPin('mqtt-1', 'in', 'input')?.pin_type).toEqual({ kind: 'json' });
    expect(findPin('mqtt-1', 'out', 'output')?.pin_type).toEqual({ kind: 'any' });

    // 再注入 subscribe 模式的 schema（pin 镜像翻转）
    vi.mocked(describeNodePins).mockResolvedValueOnce({
      inputPins: [
        { id: 'in', label: 'in', pin_type: { kind: 'any' }, direction: 'input', required: true, kind: 'exec' },
      ],
      outputPins: [
        {
          id: 'out',
          label: 'out',
          pin_type: { kind: 'json' },
          direction: 'output',
          required: false,
          kind: 'exec',
        },
      ],
    });
    await refreshNodePinSchema('mqtt-1', 'mqttClient', { mode: 'subscribe' });
    expect(findPin('mqtt-1', 'in', 'input')?.pin_type).toEqual({ kind: 'any' });
    expect(findPin('mqtt-1', 'out', 'output')?.pin_type).toEqual({ kind: 'json' });
  });

  it('clearPinSchemaCache 清空整张缓存（跨 workflow 切换）', async () => {
    vi.mocked(describeNodePins).mockResolvedValueOnce({ inputPins: [], outputPins: [] });
    vi.mocked(describeNodePins).mockResolvedValueOnce({ inputPins: [], outputPins: [] });
    await refreshNodePinSchema('a', 'foo', {});
    await refreshNodePinSchema('b', 'foo', {});
    const { clearPinSchemaCache } = await import('../pin-schema-cache');
    clearPinSchemaCache();
    expect(getCachedPinSchema('a')).toBeUndefined();
    expect(getCachedPinSchema('b')).toBeUndefined();
  });

  it('查不存在的端口返回 undefined（不抛错）', async () => {
    vi.mocked(describeNodePins).mockResolvedValueOnce({
      inputPins: [
        { id: 'in', label: 'in', pin_type: { kind: 'json' }, direction: 'input', required: true, kind: 'exec' },
      ],
      outputPins: [],
    });
    await refreshNodePinSchema('node-1', 'foo', {});
    expect(findPin('node-1', 'ghost', 'input')).toBeUndefined();
    expect(findPin('未存在', 'in', 'input')).toBeUndefined();
  });
});

describe('formatPinType', () => {
  it('标量 kind 直接返回', () => {
    expect(formatPinType({ kind: 'json' })).toBe('json');
    expect(formatPinType({ kind: 'any' })).toBe('any');
    expect(formatPinType({ kind: 'bool' })).toBe('bool');
  });

  it('array 嵌套递归展开', () => {
    expect(formatPinType({ kind: 'array', inner: { kind: 'json' } })).toBe('array<json>');
    expect(
      formatPinType({ kind: 'array', inner: { kind: 'array', inner: { kind: 'bool' } } }),
    ).toBe('array<array<bool>>');
  });

  it('custom 带 name', () => {
    expect(formatPinType({ kind: 'custom', name: 'modbus-register' })).toBe(
      'custom(modbus-register)',
    );
  });
});

describe('getPortTooltip', () => {
  beforeEach(() => {
    _resetCacheForTests();
    vi.mocked(describeNodePins).mockReset();
  });

  it('缓存未命中返回 undefined（不挡 hover）', () => {
    expect(getPortTooltip('未知节点', 'in', 'input')).toBeUndefined();
  });

  it('完整 schema 含方向 / 类型 / required / description', async () => {
    vi.mocked(describeNodePins).mockResolvedValueOnce({
      inputPins: [
        {
          id: 'in',
          label: 'in',
          pin_type: { kind: 'json' },
          direction: 'input',
          required: true,
          kind: 'exec',
          description: 'JSON 行数据',
        },
      ],
      outputPins: [],
    });
    await refreshNodePinSchema('sql-1', 'sqlWriter', {});
    const tooltip = getPortTooltip('sql-1', 'in', 'input');
    expect(tooltip).toContain('输入');
    expect(tooltip).toContain('json');
    expect(tooltip).toContain('必需');
    expect(tooltip).toContain('JSON 行数据');
  });

  it('output 不渲染必需标记', async () => {
    vi.mocked(describeNodePins).mockResolvedValueOnce({
      inputPins: [],
      outputPins: [
        {
          id: 'out',
          label: 'out',
          pin_type: { kind: 'any' },
          direction: 'output',
          required: false,
          kind: 'exec',
        },
      ],
    });
    await refreshNodePinSchema('node-1', 'foo', {});
    const tooltip = getPortTooltip('node-1', 'out', 'output');
    expect(tooltip).toContain('输出');
    expect(tooltip).not.toContain('必需');
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
