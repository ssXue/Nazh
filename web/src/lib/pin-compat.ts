// TS 端 PinType 兼容性判断。
//
// 必须与 Rust 的 PinType::is_compatible_with（在 crates/core/src/pin.rs）
// 严格一致。任意一方漂移由 tests/fixtures/pin_compat_matrix.jsonc 合约
// 测试在 CI 抓——前端在 `__tests__/pin-compat.test.ts` 跑全表断言。

import type { PinType } from '../types';

/**
 * 判断"上游产出 `from` → 下游期望 `to`"是否兼容。
 *
 * 兼容矩阵（与部署期校验规则代码化形态一致）：
 * - 任一端是 `any` → 通过
 * - 标量类型精确相等 → 通过
 * - `array` → 嵌套递归 + 内层各自兼容
 * - `custom` → name 精确相等
 * - 跨类（`string` ↔ `integer`、`json` ↔ `bool` 等）→ 不通过
 *
 * 注意：`json → json` 通过、`json → any` 通过、`any → json` 通过；
 * 但 `json → integer` 拒绝——`json` 是结构上的"任意"，类型上仍是独立类。
 */
export function isCompatibleWith(from: PinType, to: PinType): boolean {
  // Any 双向吃一切——匹配 Rust 矩阵的前两行
  if (from.kind === 'any' || to.kind === 'any') return true;

  // 标量精确相等
  if (from.kind === to.kind) {
    if (
      from.kind === 'bool' ||
      from.kind === 'integer' ||
      from.kind === 'float' ||
      from.kind === 'string' ||
      from.kind === 'json' ||
      from.kind === 'binary'
    ) {
      return true;
    }
    if (from.kind === 'array' && to.kind === 'array') {
      return isCompatibleWith(from.inner, to.inner);
    }
    if (from.kind === 'custom' && to.kind === 'custom') {
      return from.name === to.name;
    }
  }

  return false;
}
