// TS 端 PinType 兼容性判断。
//
// 必须与 Rust 的 PinType::is_compatible_with（在 crates/core/src/pin.rs）
// 严格一致。任意一方漂移由 tests/fixtures/pin_compat_matrix.jsonc 合约
// 测试在 CI 抓——前端在 `__tests__/pin-compat.test.ts` 跑全表断言。

import type { PinKind, PinType } from '../types';

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

/**
 * 判断"上游引脚 Kind `from` → 下游引脚 Kind `to`"是否可连。
 *
 * 兼容矩阵（ADR-0015 Phase 1）：
 * - 同种互连：Exec↔Exec、Data↔Data、Reactive↔Reactive
 * - Reactive 输出 → 可连 Exec / Data / Reactive 输入（订阅式驱动纯推 / 纯拉下游）
 * - Exec / Data 输出 → 不可连 Reactive 输入
 * - Exec ↔ Data 互不兼容（ADR-0014 保持不变）
 *
 * 必须与 Rust 端 `PinKind::is_compatible_with`（在 crates/core/src/pin.rs）严格
 * 一致——由 tests/fixtures/pin_kind_matrix.jsonc 合约保证（前后端共享 fixture）。
 */
export function isKindCompatible(from: PinKind, to: PinKind): boolean {
  if (from === to) return true;
  // Reactive 输出可连 Exec / Data 输入（Reactive 是 Exec+Data 超集）
  if (from === 'reactive' && (to === 'exec' || to === 'data')) return true;
  return false;
}

/**
 * 判定节点是否为 pure-form（无 Exec / Reactive 引脚）。
 *
 * Reactive 引脚参与触发链（行为是 Exec+Data 并集），因此排除出 pure-form。
 * 与 Rust `nazh_core::is_pure_form` 同语义——任一端有 `kind: 'exec'` 或
 * `kind: 'reactive'` 引脚即非 pure-form。空输入 / 空输出 + 全 Data 仍算
 * pure-form（典型如"设备表"常量节点）。
 *
 * 跨语言契约 fixture：`tests/fixtures/pure_form_matrix.jsonc`（仓库根）。
 */
export function isPureForm(
  inputPins: ReadonlyArray<{ kind?: string }>,
  outputPins: ReadonlyArray<{ kind?: string }>,
): boolean {
  const hasExecIn = inputPins.some(
    (p) => { const k = p.kind ?? 'exec'; return k === 'exec' || k === 'reactive'; },
  );
  const hasExecOut = outputPins.some(
    (p) => { const k = p.kind ?? 'exec'; return k === 'exec' || k === 'reactive'; },
  );
  return !hasExecIn && !hasExecOut;
}
