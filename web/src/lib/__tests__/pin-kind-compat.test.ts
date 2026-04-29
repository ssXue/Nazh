// TS isKindCompatible 合约测试。
//
// 消费 tests/fixtures/pin_kind_matrix.jsonc 作为单一真值源——同一份 fixture
// 也被 Rust（crates/core/tests/pin_kind_contract.rs）消费。任意一方与 fixture
// 漂移即触发 CI 红，杜绝"两份 PinKind 判断悄悄走偏"的隐藏 bug。
//
// 修改 PinKind 兼容矩阵时同步：
// 1. 改 fixture
// 2. 改 Rust PinKind::is_compatible_with
// 3. 改 TS isKindCompatible
// 4. 跑 cargo test -p nazh-core --test pin_kind_contract
// 5. 跑 npm --prefix web run test pin-kind-compat

import { describe, expect, it } from 'vitest';

import { isKindCompatible } from '../pin-compat';
import type { PinKind } from '../../types';
import { fixturePath, loadJsoncFixture } from './_fixture-helpers';

interface KindPair {
  from: PinKind;
  to: PinKind;
  compatible: boolean;
}

interface Fixture {
  pairs: KindPair[];
}

const loadFixture = (): Fixture =>
  loadJsoncFixture<Fixture>(fixturePath(__dirname, 'pin_kind_matrix.jsonc'));

describe('PinKind 兼容矩阵合约（与 Rust 端共享 fixture）', () => {
  const fixture = loadFixture();

  it.each(fixture.pairs)(
    'isKindCompatible($from, $to) === $compatible',
    ({ from, to, compatible }) => {
      expect(isKindCompatible(from, to)).toBe(compatible);
    },
  );

  it('fixture 穷尽 PinKind × PinKind 笛卡儿积', () => {
    const variants: PinKind[] = ['exec', 'data', 'reactive'];
    for (const from of variants) {
      for (const to of variants) {
        expect(
          fixture.pairs.some((p) => p.from === from && p.to === to),
        ).toBe(true);
      }
    }
  });
});
