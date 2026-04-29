/**
 * ADR-0014 Phase 4 契约测试：EmptyPolicy 序列化矩阵（TS 侧）。
 *
 * 读取 `empty_policy_matrix.jsonc`，验证每个 case 的解析 + 重序列化
 * 与 `expected_serialized` 一致。与 `tests/empty_policy_contract.rs` 形成跨语言覆盖。
 */

import { describe, expect, it } from 'vitest';

// 与 Rust `EmptyPolicy` serde(tag = "kind", content = "value") 对齐
interface EmptyPolicyBlockUntilReady {
  kind: 'block_until_ready';
}
interface EmptyPolicyDefaultValue {
  kind: 'default_value';
  value: unknown;
}
interface EmptyPolicySkip {
  kind: 'skip';
}
type EmptyPolicy =
  | EmptyPolicyBlockUntilReady
  | EmptyPolicyDefaultValue
  | EmptyPolicySkip;

// 最小 JSONC strip（单行 // 注释）
function stripJsonc(raw: string): string {
  return raw
    .split('\n')
    .map((l) => {
      const idx = l.indexOf('//');
      return idx >= 0 ? l.slice(0, idx) : l;
    })
    .join('\n');
}

interface Case {
  name: string;
  policy: EmptyPolicy;
  expected_serialized: EmptyPolicy;
  is_default: boolean;
}

// eslint-disable-next-line @typescript-eslint/no-require-imports
const fs = require('node:fs');
// eslint-disable-next-line @typescript-eslint/no-require-imports
const path = require('node:path');

const fixtureRaw = fs.readFileSync(
  path.resolve(
    __dirname,
    '../../../../tests/fixtures/empty_policy_matrix.jsonc',
  ),
  'utf-8',
);
const cases: Case[] = JSON.parse(stripJsonc(fixtureRaw));

describe('EmptyPolicy 序列化契约', () => {
  it.each(cases)('case $name', ({ name, policy, expected_serialized, is_default }) => {
    // kind 存在性
    expect(policy).toHaveProperty('kind');

    // 默认性检查
    const isDefaultPolicy = policy.kind === 'block_until_ready';
    expect(isDefaultPolicy).toBe(is_default);

    // 重序列化 roundtrip
    const serialized = JSON.parse(JSON.stringify(policy));
    expect(serialized).toEqual(expected_serialized);
  });

  it('fixture 包含 5 case', () => {
    expect(cases.length).toBe(5);
  });
});
