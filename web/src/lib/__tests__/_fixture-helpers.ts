// 共享：从工作空间根 `tests/fixtures/*.jsonc` 加载合约 fixture。
//
// `pin-compat.test.ts`（PinType 矩阵）与 `pin-kind-compat.test.ts`（PinKind 矩阵）
// 都用同一份 fixture pattern——本模块抽出加载逻辑，让前后端共享 fixture 的
// 路径约定与非空守卫只有一处定义。
//
// 序列化形态：jsonc-parser 解析（容忍 `// ...` 注释）+ 顶层 `{ pairs: [...] }`。

import { readFileSync } from 'node:fs';
import path from 'node:path';

import { parse as parseJsonc } from 'jsonc-parser';

/**
 * 解析 fixture 在工作空间根 `tests/fixtures/<fileName>` 的绝对路径。
 *
 * 调用方传入 `__dirname`（来自 `web/src/lib/__tests__/`）+ fixture 文件名；
 * 4 层向上即工作空间根。
 */
export function fixturePath(testDirname: string, fileName: string): string {
  return path.resolve(testDirname, '../../../../tests/fixtures', fileName);
}

/**
 * 读 + parseJsonc + 非空守卫；任一步失败抛带绝对路径的中文错。
 *
 * 类型参数 `T` 必须满足 `{ pairs: unknown[] }`——所有合约 fixture 共享
 * "顶层是 pairs 数组"的约定（PinType / PinKind 矩阵都是）。
 */
export function loadJsoncFixture<T extends { pairs: unknown[] }>(absPath: string): T {
  const raw = readFileSync(absPath, 'utf-8');
  const parsed = parseJsonc(raw) as T | undefined;
  if (!parsed?.pairs?.length) {
    throw new Error(
      `合约 fixture 反序列化为空 / 缺 pairs 数组（路径: ${absPath}）`,
    );
  }
  return parsed;
}
