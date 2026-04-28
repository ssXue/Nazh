// 连接期校验纯函数单测。
//
// 用 vi.mock 隔离 pin-schema-cache（实际生产路径上的 cache 由 IPC 写入，
// 此处直接 mock findPin 让测试聚焦在判断逻辑）。

import { beforeEach, describe, expect, it, vi } from 'vitest';

vi.mock('../pin-schema-cache', async () => {
  const actual = await vi.importActual<typeof import('../pin-schema-cache')>('../pin-schema-cache');
  return {
    ...actual,
    findPin: vi.fn(),
  };
});

import { findPin } from '../pin-schema-cache';
import { checkConnection, formatRejection } from '../pin-validator';

const mockedFindPin = vi.mocked(findPin);

function pin(id: string, kind: string, direction: 'input' | 'output', extra?: { name?: string }) {
  const pin_type =
    kind === 'custom' && extra?.name
      ? ({ kind: 'custom', name: extra.name } as const)
      : ({ kind } as const);
  return {
    id,
    label: id,
    pin_type: pin_type as never,
    direction,
    required: direction === 'input',
    kind: 'exec' as const,
  };
}

describe('checkConnection', () => {
  beforeEach(() => {
    mockedFindPin.mockReset();
  });

  it('两端都 Json → Json 时 allow=true', () => {
    mockedFindPin
      .mockReturnValueOnce(pin('out', 'json', 'output'))
      .mockReturnValueOnce(pin('in', 'json', 'input'));

    const result = checkConnection('a', 'out', 'b', 'in');
    expect(result.allow).toBe(true);
    expect(result.rejection).toBeNull();
  });

  it('Bool → Json 时 allow=false 且 rejection 含两端类型', () => {
    mockedFindPin
      .mockReturnValueOnce(pin('out', 'bool', 'output'))
      .mockReturnValueOnce(pin('in', 'json', 'input'));

    const result = checkConnection('src', 'out', 'sink', 'in');
    expect(result.allow).toBe(false);
    expect(result.rejection).toEqual({
      kind: 'incompatible-types',
      fromNodeId: 'src',
      fromPortId: 'out',
      toNodeId: 'sink',
      toPortId: 'in',
      fromType: { kind: 'bool' },
      toType: { kind: 'json' },
    });
  });

  it('Any → Json 时 allow=true（Any 双向兜底）', () => {
    mockedFindPin
      .mockReturnValueOnce(pin('out', 'any', 'output'))
      .mockReturnValueOnce(pin('in', 'json', 'input'));

    expect(checkConnection('a', 'out', 'b', 'in').allow).toBe(true);
  });

  it('上游缓存未命中时放行（fallback 给部署期）', () => {
    mockedFindPin
      .mockReturnValueOnce(undefined)
      .mockReturnValueOnce(pin('in', 'json', 'input'));

    expect(checkConnection('a', 'out', 'b', 'in').allow).toBe(true);
  });

  it('下游缓存未命中时放行', () => {
    mockedFindPin
      .mockReturnValueOnce(pin('out', 'json', 'output'))
      .mockReturnValueOnce(undefined);

    expect(checkConnection('a', 'out', 'b', 'in').allow).toBe(true);
  });

  it('Custom 同名通过 / 异名拒绝', () => {
    // 同名
    mockedFindPin
      .mockReturnValueOnce(pin('out', 'custom', 'output', { name: 'modbus-register' }))
      .mockReturnValueOnce(pin('in', 'custom', 'input', { name: 'modbus-register' }));
    expect(checkConnection('a', 'out', 'b', 'in').allow).toBe(true);

    // 异名
    mockedFindPin
      .mockReturnValueOnce(pin('out', 'custom', 'output', { name: 'modbus-register' }))
      .mockReturnValueOnce(pin('in', 'custom', 'input', { name: 'sql-row' }));
    expect(checkConnection('a', 'out', 'b', 'in').allow).toBe(false);
  });
});

describe('formatRejection', () => {
  it('incompatible-types 格式化含两端类型', () => {
    const message = formatRejection({
      kind: 'incompatible-types',
      fromNodeId: 'src',
      fromPortId: 'out',
      toNodeId: 'sink',
      toPortId: 'in',
      fromType: { kind: 'bool' },
      toType: { kind: 'json' },
    });
    expect(message).toContain('src.out');
    expect(message).toContain('sink.in');
    expect(message).toContain('bool');
    expect(message).toContain('json');
  });

  it('保留 array / custom 内层信息（不被 .kind 截断）', () => {
    const message = formatRejection({
      kind: 'incompatible-types',
      fromNodeId: 'src',
      fromPortId: 'out',
      toNodeId: 'sink',
      toPortId: 'in',
      fromType: { kind: 'array', inner: { kind: 'json' } },
      toType: { kind: 'custom', name: 'modbus-register' },
    });
    expect(message).toContain('array<json>');
    expect(message).toContain('custom(modbus-register)');
  });
});
