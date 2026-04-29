/**
 * ADR-0014 Phase 3b：与 Rust `src/graph/pull.rs::merge_payload` 等价的 TS 实现。
 *
 * 用途：AI prompt 生成预览、前端调试视图模拟"transform 期看到的 payload"。
 * 不是运行期路径——运行期 payload 合并由 Rust Runner 完成，前端只在解释/预览
 * 场景使用本函数。
 *
 * 合约 fixture：`tests/fixtures/mixed_input_merge.jsonc`（仓库根，与 Rust 共享）。
 */
export function mergePullPayload(
  execPayload: unknown,
  dataValues: Record<string, unknown>,
): unknown {
  if (
    execPayload !== null &&
    typeof execPayload === 'object' &&
    !Array.isArray(execPayload)
  ) {
    return { ...(execPayload as Record<string, unknown>), ...dataValues };
  }
  return { in: execPayload, ...dataValues };
}
