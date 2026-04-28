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

import { describe, expect, it } from 'vitest';

import { isCompatibleWith, isPureForm } from '../pin-compat';
import type { PinType } from '../../types';
import { fixturePath, loadJsoncFixture } from './_fixture-helpers';
import { readFileSync } from 'node:fs';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

interface CompatPair {
  from: PinType;
  to: PinType;
  compatible: boolean;
}

interface Fixture {
  pairs: CompatPair[];
}

const loadFixture = (): Fixture =>
  loadJsoncFixture<Fixture>(fixturePath(__dirname, 'pin_compat_matrix.jsonc'));

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

describe('isPureForm — fixture parity with Rust `nazh_core::is_pure_form`', () => {
  const __dirname = dirname(fileURLToPath(import.meta.url));
  const raw = readFileSync(
    resolve(__dirname, '../../../../tests/fixtures/pure_form_matrix.jsonc'),
    'utf8',
  );
  const stripped = raw
    .split('\n')
    .map((line) => {
      const idx = line.indexOf('//');
      return idx >= 0 ? line.slice(0, idx) : line;
    })
    .join('\n');
  const cases = JSON.parse(stripped) as Array<{
    name: string;
    input_pins: Array<{ kind: 'exec' | 'data' }>;
    output_pins: Array<{ kind: 'exec' | 'data' }>;
    expected_pure_form: boolean;
  }>;

  it.each(cases)('$name → $expected_pure_form', (c) => {
    expect(isPureForm(c.input_pins, c.output_pins)).toBe(c.expected_pure_form);
  });
});
