// TS isCompatibleWith 合约测试。
//
// 消费 tests/fixtures/pin_compat_matrix.jsonc 作为单一真值源——同一份
// fixture 也被 Rust（crates/core/tests/pin_compat_contract.rs）消费。
// 任意一方与 fixture 漂移即触发 CI 红，杜绝"两份兼容判断悄悄走偏"的
// 隐藏 bug。
//
// 修改 PinType 兼容矩阵时同步：
// 1. 改 fixture
// 2. 改 Rust PinType::is_compatible_with
// 3. 改 TS isCompatibleWith
// 4. 跑 cargo test -p nazh-core --test pin_compat_contract
// 5. 跑 npm --prefix web run test pin-compat

import * as fs from 'node:fs';
import * as path from 'node:path';

import { parse as parseJsonc } from 'jsonc-parser';
import { describe, expect, it } from 'vitest';

import { isCompatibleWith } from '../pin-compat';
import type { PinType } from '../../types';

interface CompatPair {
  from: PinType;
  to: PinType;
  compatible: boolean;
}

interface Fixture {
  pairs: CompatPair[];
}

// fixture 在 workspace 根 tests/fixtures/——前后端共享。
// 路径：web/src/lib/__tests__/ → 4 层上 → tests/fixtures/...
const fixturePath = path.resolve(
  __dirname,
  '../../../../tests/fixtures/pin_compat_matrix.jsonc',
);

function loadFixture(): Fixture {
  const raw = fs.readFileSync(fixturePath, 'utf-8');
  const parsed = parseJsonc(raw) as Fixture | undefined;
  if (!parsed?.pairs?.length) {
    throw new Error(
      `pin_compat_matrix.jsonc 反序列化为空 / 缺 pairs 数组（路径: ${fixturePath}）`,
    );
  }
  return parsed;
}

describe('isCompatibleWith vs Rust 合约 fixture', () => {
  const fixture = loadFixture();

  it.each(
    fixture.pairs.map((pair, index) => ({
      ...pair,
      _index: index,
    })),
  )(
    'pair #$_index: $from.kind → $to.kind 期望 compatible=$compatible',
    ({ from, to, compatible }) => {
      expect(isCompatibleWith(from, to)).toBe(compatible);
    },
  );

  it('每个 PinType 变体至少出现一次（与 Rust 端覆盖纪律一致）', () => {
    const seen = new Set<string>();
    const collect = (pin: PinType): void => {
      seen.add(pin.kind);
      if (pin.kind === 'array') {
        collect(pin.inner);
      }
    };
    fixture.pairs.forEach((pair) => {
      collect(pair.from);
      collect(pair.to);
    });
    [
      'any',
      'bool',
      'integer',
      'float',
      'string',
      'json',
      'binary',
      'array',
      'custom',
    ].forEach((kind) => {
      expect(seen).toContain(kind);
    });
  });
});
