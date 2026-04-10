# 前端测试体系 + App.tsx 重构实施计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 将 App.tsx 从 1436 行拆分至 ~550 行，建立 Vitest 单元测试 + Playwright E2E 测试体系。

**Architecture:** 先提取 5 个纯函数模块和 2 个自定义 hooks，再为提取后的模块编写 Vitest 单元测试，最后搭建 Playwright E2E 测试完整 Tauri 桌面链路。每个提取步骤后验证构建通过，保证重构不破坏功能。

**Tech Stack:** Vitest, Playwright, React hooks, TypeScript

---

## File Structure

### New Files

| File | Responsibility |
|------|------|
| `web/vitest.config.ts` | Vitest 配置 |
| `web/src/lib/workflow-events.ts` | 事件解析 + 状态机 + 日志/错误构建 |
| `web/src/lib/workflow-status.ts` | 工作流状态派生 + 标签 + 样式类 |
| `web/src/lib/settings.ts` | localStorage 偏好读取 |
| `web/src/lib/demo-data.ts` | 示例 AST 构建 |
| `web/src/lib/sidebar.ts` | 侧栏导航配置 |
| `web/src/hooks/use-settings.ts` | 偏好 state + localStorage 同步 hook |
| `web/src/hooks/use-workflow-engine.ts` | 工作流部署/事件/状态管理 hook |
| `web/src/lib/__tests__/parse-event.test.ts` | parseWorkflowEventPayload 测试 |
| `web/src/lib/__tests__/reduce-state.test.ts` | reduceRuntimeState 测试 |
| `web/src/lib/__tests__/workflow-status.test.ts` | deriveWorkflowStatus 测试 |
| `web/src/lib/__tests__/settings.test.ts` | getInitial* 测试 |
| `web/src/lib/__tests__/parse-graph.test.ts` | parseWorkflowGraph 测试 |
| `web/src/lib/__tests__/layout-graph.test.ts` | layoutGraph 测试 |
| `web/src/lib/__tests__/nazh-to-flowgram.test.ts` | toFlowgramWorkflowJson 测试 |
| `web/src/lib/__tests__/flowgram-to-nazh.test.ts` | toNazhWorkflowGraph 测试 |
| `web/e2e/playwright.config.ts` | Playwright 配置 |
| `web/e2e/deploy-and-dispatch.spec.ts` | 核心链路 E2E |
| `web/e2e/lifecycle.spec.ts` | 生命周期 E2E |
| `web/e2e/error-handling.spec.ts` | 错误处理 E2E |

### Modified Files

| File | Changes |
|------|---------|
| `web/package.json` | 新增 vitest / playwright 依赖和 scripts |
| `web/vite.config.ts` | 新增 vitest 配置（或独立 vitest.config.ts） |
| `web/tsconfig.json` | include 加入 e2e/ |
| `web/src/App.tsx` | 删除已提取代码，改为 import 调用 hooks |
| `web/src/components/app/SidebarNav.tsx` | 添加 data-testid |
| `web/src/components/app/SourcePanel.tsx` | 添加 data-testid |
| `web/src/components/app/PayloadPanel.tsx` | 添加 data-testid |
| `web/src/components/app/RuntimeDock.tsx` | 添加 data-testid |
| `CLAUDE.md` | Build Commands + Testing + Frontend Key Files |
| `README.md` | 当前完成度 + 已验证状态 |
| `AI-Context.md` | 路线图补充 |

---

### Task 1: 安装 Vitest 并配置

**Files:**
- Modify: `web/package.json`
- Create: `web/vitest.config.ts`

- [ ] **Step 1: 安装 vitest**

```bash
npm --prefix web install -D vitest
```

- [ ] **Step 2: 创建 `web/vitest.config.ts`**

```typescript
import { defineConfig } from 'vitest/config';
import react from '@vitejs/plugin-react';

export default defineConfig({
  plugins: [react()],
  test: {
    environment: 'node',
    include: ['src/**/__tests__/**/*.test.ts'],
  },
});
```

- [ ] **Step 3: 在 `web/package.json` 添加 scripts**

在 `"scripts"` 中添加：

```json
"test": "vitest run",
"test:watch": "vitest"
```

- [ ] **Step 4: 验证 vitest 可运行**

```bash
npm --prefix web run test
```

Expected: "No test files found"（还没有测试文件），但 vitest 本身正常启动退出。

- [ ] **Step 5: 提交**

```bash
git add web/package.json web/package-lock.json web/vitest.config.ts
git commit -s -m "chore: 安装并配置 Vitest 前端单元测试框架"
```

---

### Task 2: 提取 `workflow-events.ts`

**Files:**
- Create: `web/src/lib/workflow-events.ts`
- Modify: `web/src/App.tsx`

- [ ] **Step 1: 创建 `web/src/lib/workflow-events.ts`**

从 `App.tsx` 移出以下内容（行 59-64, 71-81, 425-584），加上 export 和必要的 import：

```typescript
//! 工作流运行时事件解析与状态机。
//!
//! 从 Tauri 事件 payload 解析 ExecutionEvent 联合类型，
//! 通过 reducer 模式维护工作流运行时状态。

import type {
  AppErrorRecord,
  ExecutionEvent,
  RuntimeLogEntry,
  WorkflowRuntimeState,
} from '../types';

export interface ParsedWorkflowEvent {
  kind: 'started' | 'completed' | 'failed' | 'output';
  nodeId: string;
  traceId: string;
  error?: string;
}

export const EMPTY_RUNTIME_STATE: WorkflowRuntimeState = {
  traceId: null,
  lastEventType: null,
  lastNodeId: null,
  lastError: null,
  lastUpdatedAt: null,
  activeNodeIds: [],
  completedNodeIds: [],
  failedNodeIds: [],
  outputNodeIds: [],
};

export function pushUnique(items: string[], item: string): string[] {
  return items.includes(item) ? items : [...items, item];
}

export function removeItem(items: string[], item: string): string[] {
  return items.filter((current) => current !== item);
}

export function createClientEntryId(prefix: string): string {
  return `${prefix}-${Date.now()}-${Math.random().toString(16).slice(2, 8)}`;
}

export function describeUnknownError(error: unknown): { message: string; detail?: string | null } {
  if (error instanceof Error) {
    return {
      message: error.message || '未知错误',
      detail: error.stack ?? null,
    };
  }

  if (typeof error === 'string') {
    return { message: error };
  }

  if (error && typeof error === 'object') {
    try {
      return {
        message: JSON.stringify(error),
      };
    } catch {
      return {
        message: '发生了无法序列化的异常对象',
      };
    }
  }

  return { message: '未知错误' };
}

export function buildRuntimeLogEntry(
  source: string,
  level: RuntimeLogEntry['level'],
  message: string,
  detail?: string | null,
): RuntimeLogEntry {
  return {
    id: createClientEntryId('log'),
    timestamp: Date.now(),
    level,
    source,
    message,
    detail: detail ?? null,
  };
}

export function buildAppErrorRecord(
  scope: AppErrorRecord['scope'],
  title: string,
  detail?: string | null,
): AppErrorRecord {
  return {
    id: createClientEntryId('error'),
    timestamp: Date.now(),
    scope,
    title,
    detail: detail ?? null,
  };
}

export function parseWorkflowEventPayload(payload: unknown): ParsedWorkflowEvent | null {
  if (!payload || typeof payload !== 'object') {
    return null;
  }

  const event = payload as ExecutionEvent;

  if ('Started' in event) {
    return {
      kind: 'started',
      nodeId: event.Started.stage,
      traceId: event.Started.trace_id,
    };
  }

  if ('Completed' in event) {
    return {
      kind: 'completed',
      nodeId: event.Completed.stage,
      traceId: event.Completed.trace_id,
    };
  }

  if ('Failed' in event) {
    return {
      kind: 'failed',
      nodeId: event.Failed.stage,
      traceId: event.Failed.trace_id,
      error: event.Failed.error,
    };
  }

  if ('Output' in event) {
    return {
      kind: 'output',
      nodeId: event.Output.stage,
      traceId: event.Output.trace_id,
    };
  }

  return null;
}

export function reduceRuntimeState(
  current: WorkflowRuntimeState,
  event: ParsedWorkflowEvent,
): WorkflowRuntimeState {
  const baseState =
    current.traceId === event.traceId
      ? current
      : {
          ...EMPTY_RUNTIME_STATE,
          traceId: event.traceId,
        };

  const nextState: WorkflowRuntimeState = {
    ...baseState,
    traceId: event.traceId,
    lastEventType: event.kind,
    lastNodeId: event.nodeId,
    lastError: event.kind === 'failed' ? event.error ?? null : null,
    lastUpdatedAt: Date.now(),
  };

  switch (event.kind) {
    case 'started':
      nextState.activeNodeIds = pushUnique(baseState.activeNodeIds, event.nodeId);
      nextState.completedNodeIds = removeItem(baseState.completedNodeIds, event.nodeId);
      nextState.failedNodeIds = removeItem(baseState.failedNodeIds, event.nodeId);
      nextState.outputNodeIds = removeItem(baseState.outputNodeIds, event.nodeId);
      return nextState;
    case 'completed':
      nextState.activeNodeIds = removeItem(baseState.activeNodeIds, event.nodeId);
      nextState.completedNodeIds = pushUnique(baseState.completedNodeIds, event.nodeId);
      nextState.failedNodeIds = removeItem(baseState.failedNodeIds, event.nodeId);
      nextState.outputNodeIds = baseState.outputNodeIds;
      return nextState;
    case 'failed':
      nextState.activeNodeIds = removeItem(baseState.activeNodeIds, event.nodeId);
      nextState.completedNodeIds = removeItem(baseState.completedNodeIds, event.nodeId);
      nextState.failedNodeIds = pushUnique(baseState.failedNodeIds, event.nodeId);
      nextState.outputNodeIds = removeItem(baseState.outputNodeIds, event.nodeId);
      return nextState;
    case 'output':
      nextState.activeNodeIds = removeItem(baseState.activeNodeIds, event.nodeId);
      nextState.completedNodeIds = pushUnique(baseState.completedNodeIds, event.nodeId);
      nextState.failedNodeIds = removeItem(baseState.failedNodeIds, event.nodeId);
      nextState.outputNodeIds = pushUnique(baseState.outputNodeIds, event.nodeId);
      return nextState;
  }
}
```

- [ ] **Step 2: 更新 `App.tsx` — 删除已移出代码，添加 import**

在 `App.tsx` 顶部添加：

```typescript
import {
  buildAppErrorRecord,
  buildRuntimeLogEntry,
  describeUnknownError,
  EMPTY_RUNTIME_STATE,
  parseWorkflowEventPayload,
  reduceRuntimeState,
  type ParsedWorkflowEvent,
} from './lib/workflow-events';
```

删除 `App.tsx` 中的以下代码块：
- 行 59-64: `interface ParsedWorkflowEvent`
- 行 71-81: `const EMPTY_RUNTIME_STATE`
- 行 425-584: `pushUnique` 到 `reduceRuntimeState` 的全部函数

- [ ] **Step 3: 验证构建**

```bash
npm --prefix web run build
```

Expected: tsc + vite build 通过，无类型错误。

- [ ] **Step 4: 提交**

```bash
git add web/src/lib/workflow-events.ts web/src/App.tsx
git commit -s -m "refactor: 提取 workflow-events.ts——事件解析与状态机"
```

---

### Task 3: 编写 workflow-events 单元测试

**Files:**
- Create: `web/src/lib/__tests__/parse-event.test.ts`
- Create: `web/src/lib/__tests__/reduce-state.test.ts`

- [ ] **Step 1: 创建 `web/src/lib/__tests__/parse-event.test.ts`**

```typescript
import { describe, expect, it } from 'vitest';

import { parseWorkflowEventPayload } from '../workflow-events';

describe('parseWorkflowEventPayload', () => {
  it('解析 Started 事件', () => {
    const result = parseWorkflowEventPayload({
      Started: { stage: 'node-a', trace_id: 'trace-1' },
    });
    expect(result).toEqual({ kind: 'started', nodeId: 'node-a', traceId: 'trace-1' });
  });

  it('解析 Completed 事件', () => {
    const result = parseWorkflowEventPayload({
      Completed: { stage: 'node-b', trace_id: 'trace-2' },
    });
    expect(result).toEqual({ kind: 'completed', nodeId: 'node-b', traceId: 'trace-2' });
  });

  it('解析 Failed 事件', () => {
    const result = parseWorkflowEventPayload({
      Failed: { stage: 'node-c', trace_id: 'trace-3', error: '超时' },
    });
    expect(result).toEqual({
      kind: 'failed',
      nodeId: 'node-c',
      traceId: 'trace-3',
      error: '超时',
    });
  });

  it('解析 Output 事件', () => {
    const result = parseWorkflowEventPayload({
      Output: { stage: 'node-d', trace_id: 'trace-4' },
    });
    expect(result).toEqual({ kind: 'output', nodeId: 'node-d', traceId: 'trace-4' });
  });

  it('null 输入返回 null', () => {
    expect(parseWorkflowEventPayload(null)).toBeNull();
  });

  it('非 object 输入返回 null', () => {
    expect(parseWorkflowEventPayload('string')).toBeNull();
    expect(parseWorkflowEventPayload(42)).toBeNull();
  });

  it('空 object 返回 null', () => {
    expect(parseWorkflowEventPayload({})).toBeNull();
  });

  it('未知变体返回 null', () => {
    expect(parseWorkflowEventPayload({ Unknown: { stage: 'x' } })).toBeNull();
  });
});
```

- [ ] **Step 2: 创建 `web/src/lib/__tests__/reduce-state.test.ts`**

```typescript
import { describe, expect, it } from 'vitest';

import {
  EMPTY_RUNTIME_STATE,
  reduceRuntimeState,
  type ParsedWorkflowEvent,
} from '../workflow-events';

function event(
  kind: ParsedWorkflowEvent['kind'],
  nodeId: string,
  traceId = 'trace-1',
  error?: string,
): ParsedWorkflowEvent {
  return { kind, nodeId, traceId, error };
}

describe('reduceRuntimeState', () => {
  it('started 将节点加入 activeNodeIds', () => {
    const next = reduceRuntimeState(EMPTY_RUNTIME_STATE, event('started', 'a'));
    expect(next.activeNodeIds).toContain('a');
    expect(next.lastEventType).toBe('started');
    expect(next.traceId).toBe('trace-1');
  });

  it('completed 将节点从 active 移至 completed', () => {
    const after_start = reduceRuntimeState(EMPTY_RUNTIME_STATE, event('started', 'a'));
    const next = reduceRuntimeState(after_start, event('completed', 'a'));
    expect(next.activeNodeIds).not.toContain('a');
    expect(next.completedNodeIds).toContain('a');
  });

  it('failed 记录错误并移至 failedNodeIds', () => {
    const after_start = reduceRuntimeState(EMPTY_RUNTIME_STATE, event('started', 'a'));
    const next = reduceRuntimeState(after_start, event('failed', 'a', 'trace-1', '超时'));
    expect(next.failedNodeIds).toContain('a');
    expect(next.activeNodeIds).not.toContain('a');
    expect(next.lastError).toBe('超时');
  });

  it('output 将节点加入 outputNodeIds 和 completedNodeIds', () => {
    const after_start = reduceRuntimeState(EMPTY_RUNTIME_STATE, event('started', 'a'));
    const next = reduceRuntimeState(after_start, event('output', 'a'));
    expect(next.outputNodeIds).toContain('a');
    expect(next.completedNodeIds).toContain('a');
    expect(next.activeNodeIds).not.toContain('a');
  });

  it('trace_id 切换时重置状态', () => {
    const s1 = reduceRuntimeState(EMPTY_RUNTIME_STATE, event('started', 'a', 'trace-1'));
    expect(s1.activeNodeIds).toContain('a');

    const s2 = reduceRuntimeState(s1, event('started', 'b', 'trace-2'));
    expect(s2.traceId).toBe('trace-2');
    expect(s2.activeNodeIds).toEqual(['b']);
    expect(s2.completedNodeIds).toEqual([]);
  });

  it('多节点并发', () => {
    let state = reduceRuntimeState(EMPTY_RUNTIME_STATE, event('started', 'a'));
    state = reduceRuntimeState(state, event('started', 'b'));
    expect(state.activeNodeIds).toEqual(['a', 'b']);

    state = reduceRuntimeState(state, event('completed', 'a'));
    expect(state.activeNodeIds).toEqual(['b']);
    expect(state.completedNodeIds).toEqual(['a']);
  });

  it('重复 started 不重复加入', () => {
    let state = reduceRuntimeState(EMPTY_RUNTIME_STATE, event('started', 'a'));
    state = reduceRuntimeState(state, event('started', 'a'));
    expect(state.activeNodeIds).toEqual(['a']);
  });
});
```

- [ ] **Step 3: 运行测试**

```bash
npm --prefix web run test
```

Expected: 所有测试通过。

- [ ] **Step 4: 提交**

```bash
git add web/src/lib/__tests__/
git commit -s -m "test: parseWorkflowEventPayload + reduceRuntimeState 单元测试"
```

---

### Task 4: 提取 `workflow-status.ts` + 测试

**Files:**
- Create: `web/src/lib/workflow-status.ts`
- Create: `web/src/lib/__tests__/workflow-status.test.ts`
- Modify: `web/src/App.tsx`

- [ ] **Step 1: 创建 `web/src/lib/workflow-status.ts`**

从 `App.tsx` 行 586-652 移出：

```typescript
//! 工作流状态派生与展示。

import type { DeployResponse, WorkflowRuntimeState, WorkflowWindowStatus } from '../types';

export function deriveWorkflowStatus(
  tauriRuntime: boolean,
  hasActiveBoard: boolean,
  deployInfo: DeployResponse | null,
  runtimeState: WorkflowRuntimeState,
): WorkflowWindowStatus {
  if (!tauriRuntime) {
    return 'preview';
  }

  if (!hasActiveBoard || !deployInfo) {
    return 'idle';
  }

  if (runtimeState.lastEventType === 'failed' || runtimeState.failedNodeIds.length > 0) {
    return 'failed';
  }

  if (runtimeState.lastEventType === 'started' || runtimeState.activeNodeIds.length > 0) {
    return 'running';
  }

  if (
    runtimeState.traceId &&
    (runtimeState.lastEventType === 'output' ||
      runtimeState.outputNodeIds.length > 0 ||
      (runtimeState.lastEventType === 'completed' &&
        runtimeState.completedNodeIds.length > 0 &&
        runtimeState.activeNodeIds.length === 0))
  ) {
    return 'completed';
  }

  return 'deployed';
}

export function getWorkflowStatusLabel(status: WorkflowWindowStatus): string {
  switch (status) {
    case 'preview':
      return '浏览器预览';
    case 'idle':
      return '未部署';
    case 'deployed':
      return '已部署待运行';
    case 'running':
      return '运行中';
    case 'completed':
      return '执行完成';
    case 'failed':
      return '执行失败';
  }
}

export function getWorkflowStatusPillClass(status: WorkflowWindowStatus): string {
  switch (status) {
    case 'running':
      return 'runtime-pill--running';
    case 'failed':
      return 'runtime-pill--failed';
    case 'completed':
    case 'deployed':
      return 'runtime-pill--ready';
    case 'idle':
    case 'preview':
      return 'runtime-pill--idle';
  }
}
```

- [ ] **Step 2: 更新 `App.tsx`**

添加 import：
```typescript
import { deriveWorkflowStatus, getWorkflowStatusLabel, getWorkflowStatusPillClass } from './lib/workflow-status';
```

删除 `App.tsx` 行 586-652 的三个函数。

- [ ] **Step 3: 创建 `web/src/lib/__tests__/workflow-status.test.ts`**

```typescript
import { describe, expect, it } from 'vitest';

import { EMPTY_RUNTIME_STATE } from '../workflow-events';
import {
  deriveWorkflowStatus,
  getWorkflowStatusLabel,
  getWorkflowStatusPillClass,
} from '../workflow-status';

const deployed = { nodeCount: 3, edgeCount: 2, rootNodes: ['a'] };

describe('deriveWorkflowStatus', () => {
  it('非 Tauri 运行时 → preview', () => {
    expect(deriveWorkflowStatus(false, true, deployed, EMPTY_RUNTIME_STATE)).toBe('preview');
  });

  it('无活跃看板 → idle', () => {
    expect(deriveWorkflowStatus(true, false, deployed, EMPTY_RUNTIME_STATE)).toBe('idle');
  });

  it('无部署信息 → idle', () => {
    expect(deriveWorkflowStatus(true, true, null, EMPTY_RUNTIME_STATE)).toBe('idle');
  });

  it('已部署无事件 → deployed', () => {
    expect(deriveWorkflowStatus(true, true, deployed, EMPTY_RUNTIME_STATE)).toBe('deployed');
  });

  it('有 active 节点 → running', () => {
    const state = { ...EMPTY_RUNTIME_STATE, lastEventType: 'started' as const, activeNodeIds: ['a'] };
    expect(deriveWorkflowStatus(true, true, deployed, state)).toBe('running');
  });

  it('有 failed 节点 → failed', () => {
    const state = { ...EMPTY_RUNTIME_STATE, lastEventType: 'failed' as const, failedNodeIds: ['a'] };
    expect(deriveWorkflowStatus(true, true, deployed, state)).toBe('failed');
  });

  it('有 output 节点 → completed', () => {
    const state = { ...EMPTY_RUNTIME_STATE, traceId: 't', lastEventType: 'output' as const, outputNodeIds: ['a'] };
    expect(deriveWorkflowStatus(true, true, deployed, state)).toBe('completed');
  });
});

describe('getWorkflowStatusLabel', () => {
  it.each([
    ['preview', '浏览器预览'],
    ['idle', '未部署'],
    ['deployed', '已部署待运行'],
    ['running', '运行中'],
    ['completed', '执行完成'],
    ['failed', '执行失败'],
  ] as const)('%s → %s', (status, label) => {
    expect(getWorkflowStatusLabel(status)).toBe(label);
  });
});

describe('getWorkflowStatusPillClass', () => {
  it('running → runtime-pill--running', () => {
    expect(getWorkflowStatusPillClass('running')).toBe('runtime-pill--running');
  });

  it('idle → runtime-pill--idle', () => {
    expect(getWorkflowStatusPillClass('idle')).toBe('runtime-pill--idle');
  });
});
```

- [ ] **Step 4: 验证构建和测试**

```bash
npm --prefix web run build && npm --prefix web run test
```

- [ ] **Step 5: 提交**

```bash
git add web/src/lib/workflow-status.ts web/src/lib/__tests__/workflow-status.test.ts web/src/App.tsx
git commit -s -m "refactor: 提取 workflow-status.ts + 单元测试"
```

---

### Task 5: 提取 `settings.ts` + 测试

**Files:**
- Create: `web/src/lib/settings.ts`
- Create: `web/src/lib/__tests__/settings.test.ts`
- Modify: `web/src/App.tsx`

- [ ] **Step 1: 创建 `web/src/lib/settings.ts`**

从 `App.tsx` 行 83-195 移出（6 个 storage key 常量 + 6 个 `getInitial*` 函数）。添加必要 import 和 export：

```typescript
//! 桌面偏好设置的 localStorage 持久化读取。

import type { MotionMode, StartupPage, ThemeMode, UiDensity } from '../components/app/types';
import {
  ACCENT_PRESET_OPTIONS,
  normalizeCustomAccentHex,
  type AccentPreset,
} from './theme';

export const THEME_STORAGE_KEY = 'nazh.theme';
export const ACCENT_PRESET_STORAGE_KEY = 'nazh.accent-preset';
export const CUSTOM_ACCENT_STORAGE_KEY = 'nazh.custom-accent';
export const UI_DENSITY_STORAGE_KEY = 'nazh.ui-density';
export const MOTION_MODE_STORAGE_KEY = 'nazh.motion-mode';
export const STARTUP_PAGE_STORAGE_KEY = 'nazh.startup-page';

export function getInitialThemeMode(): ThemeMode {
  if (typeof window === 'undefined') {
    return 'light';
  }

  try {
    const storedTheme = window.localStorage.getItem(THEME_STORAGE_KEY);
    if (storedTheme === 'light' || storedTheme === 'dark') {
      return storedTheme;
    }
  } catch {
    // 忽略 localStorage 访问失败，降级到系统偏好。
  }

  return window.matchMedia('(prefers-color-scheme: dark)').matches ? 'dark' : 'light';
}

export function getInitialAccentPreset(): AccentPreset {
  if (typeof window === 'undefined') {
    return ACCENT_PRESET_OPTIONS[0].key;
  }

  try {
    const storedPreset = window.localStorage.getItem(ACCENT_PRESET_STORAGE_KEY);
    if (
      storedPreset === 'custom' ||
      ACCENT_PRESET_OPTIONS.some((option) => option.key === storedPreset)
    ) {
      return storedPreset as AccentPreset;
    }
  } catch {
    // 忽略 localStorage 访问失败。
  }

  return ACCENT_PRESET_OPTIONS[0].key;
}

export function getInitialCustomAccentHex(): string {
  if (typeof window === 'undefined') {
    return normalizeCustomAccentHex(ACCENT_PRESET_OPTIONS[0].hex);
  }

  try {
    const storedHex = window.localStorage.getItem(CUSTOM_ACCENT_STORAGE_KEY);
    if (storedHex) {
      return normalizeCustomAccentHex(storedHex);
    }
  } catch {
    // 忽略 localStorage 访问失败。
  }

  return normalizeCustomAccentHex(ACCENT_PRESET_OPTIONS[0].hex);
}

export function getInitialUiDensity(): UiDensity {
  if (typeof window === 'undefined') {
    return 'comfortable';
  }

  try {
    const storedDensity = window.localStorage.getItem(UI_DENSITY_STORAGE_KEY);
    if (storedDensity === 'comfortable' || storedDensity === 'compact') {
      return storedDensity;
    }
  } catch {
    // 忽略 localStorage 访问失败。
  }

  return 'comfortable';
}

export function getInitialMotionMode(): MotionMode {
  if (typeof window === 'undefined') {
    return 'full';
  }

  try {
    const storedMotionMode = window.localStorage.getItem(MOTION_MODE_STORAGE_KEY);
    if (storedMotionMode === 'full' || storedMotionMode === 'reduced') {
      return storedMotionMode;
    }
  } catch {
    // 忽略 localStorage 访问失败。
  }

  return window.matchMedia('(prefers-reduced-motion: reduce)').matches ? 'reduced' : 'full';
}

export function getInitialStartupPage(): StartupPage {
  if (typeof window === 'undefined') {
    return 'dashboard';
  }

  try {
    const storedPage = window.localStorage.getItem(STARTUP_PAGE_STORAGE_KEY);
    if (storedPage === 'dashboard' || storedPage === 'boards') {
      return storedPage;
    }
  } catch {
    // 忽略 localStorage 访问失败。
  }

  return 'dashboard';
}
```

- [ ] **Step 2: 更新 `App.tsx`**

添加 import：
```typescript
import {
  THEME_STORAGE_KEY,
  ACCENT_PRESET_STORAGE_KEY,
  CUSTOM_ACCENT_STORAGE_KEY,
  UI_DENSITY_STORAGE_KEY,
  MOTION_MODE_STORAGE_KEY,
  STARTUP_PAGE_STORAGE_KEY,
  getInitialThemeMode,
  getInitialAccentPreset,
  getInitialCustomAccentHex,
  getInitialUiDensity,
  getInitialMotionMode,
  getInitialStartupPage,
} from './lib/settings';
```

删除 `App.tsx` 行 83-195 的常量和函数。

- [ ] **Step 3: 创建 `web/src/lib/__tests__/settings.test.ts`**

```typescript
import { afterEach, describe, expect, it, vi } from 'vitest';

import { getInitialThemeMode, getInitialUiDensity, getInitialStartupPage } from '../settings';

describe('getInitialThemeMode', () => {
  afterEach(() => {
    localStorage.clear();
  });

  it('localStorage 有 dark → 返回 dark', () => {
    localStorage.setItem('nazh.theme', 'dark');
    expect(getInitialThemeMode()).toBe('dark');
  });

  it('localStorage 无值 → 返回默认值', () => {
    expect(getInitialThemeMode()).toBe('light');
  });

  it('localStorage 有非法值 → 返回默认值', () => {
    localStorage.setItem('nazh.theme', 'invalid');
    expect(getInitialThemeMode()).toBe('light');
  });
});

describe('getInitialUiDensity', () => {
  afterEach(() => {
    localStorage.clear();
  });

  it('localStorage 有 compact → 返回 compact', () => {
    localStorage.setItem('nazh.ui-density', 'compact');
    expect(getInitialUiDensity()).toBe('compact');
  });

  it('localStorage 无值 → 返回 comfortable', () => {
    expect(getInitialUiDensity()).toBe('comfortable');
  });
});

describe('getInitialStartupPage', () => {
  afterEach(() => {
    localStorage.clear();
  });

  it('localStorage 有 boards → 返回 boards', () => {
    localStorage.setItem('nazh.startup-page', 'boards');
    expect(getInitialStartupPage()).toBe('boards');
  });

  it('localStorage 无值 → 返回 dashboard', () => {
    expect(getInitialStartupPage()).toBe('dashboard');
  });
});
```

**注意**：settings.test.ts 需要 localStorage，将 `vitest.config.ts` 的 `environment` 改为 `'jsdom'`，或对本文件单独标注 `// @vitest-environment jsdom`。推荐在测试文件头部加注释：

```typescript
// @vitest-environment jsdom
```

- [ ] **Step 4: 验证构建和测试**

```bash
npm --prefix web run build && npm --prefix web run test
```

如果 settings.test.ts 因缺少 `jsdom` 环境报错，安装 jsdom：

```bash
npm --prefix web install -D jsdom
```

- [ ] **Step 5: 提交**

```bash
git add web/src/lib/settings.ts web/src/lib/__tests__/settings.test.ts web/src/App.tsx web/vitest.config.ts
git commit -s -m "refactor: 提取 settings.ts + 单元测试"
```

---

### Task 6: 提取 `demo-data.ts` 和 `sidebar.ts`

**Files:**
- Create: `web/src/lib/demo-data.ts`
- Create: `web/src/lib/sidebar.ts`
- Modify: `web/src/App.tsx`

- [ ] **Step 1: 创建 `web/src/lib/demo-data.ts`**

从 `App.tsx` 行 197-370 移出 `buildIndustrialAlarmExample`、`buildProjectAst`、`buildInitialProjectDrafts`。需要导入 `BOARD_LIBRARY`、`WorkflowGraph`、`JsonValue`、`SAMPLE_AST`、`SAMPLE_PAYLOAD`，同时定义并导出 `ProjectDraft` 接口（原 `App.tsx` 行 66-69）和 `CURRENT_USER_NAME` 常量。

文件顶部：
```typescript
//! 示例工作流 AST 和项目草稿构建。

import type { BoardItem } from '../components/app/BoardsPanel';
import { BOARD_LIBRARY } from '../components/app/BoardsPanel';
import type { JsonValue, WorkflowGraph } from '../types';
import { SAMPLE_AST, SAMPLE_PAYLOAD } from '../types';

export interface ProjectDraft {
  astText: string;
  payloadText: string;
}

export const CURRENT_USER_NAME = 'ssxue';
export const DEFAULT_BOARD_ID = BOARD_LIBRARY[0]?.id ?? 'default';

export function buildIndustrialAlarmExample(boardName: string): WorkflowGraph {
  // ... 完整移入，与原文一致
}

export function buildProjectAst(boardId: string, boardName: string): string {
  // ... 完整移入
}

export function buildInitialProjectDrafts(): Record<string, ProjectDraft> {
  // ... 完整移入
}
```

函数体与 `App.tsx` 原文完全一致，只加 `export`。

- [ ] **Step 2: 创建 `web/src/lib/sidebar.ts`**

从 `App.tsx` 行 372-423 移出：

```typescript
//! 侧栏导航配置构建。

import type { SidebarSectionConfig } from '../components/app/types';
import type { DeployResponse } from '../types';
import { BOARD_LIBRARY } from '../components/app/BoardsPanel';
import { hasTauriRuntime } from './tauri';

export function buildSidebarSections(
  workflowStatusLabel: string,
  graphError: string | null,
  graphConnectionCount: number,
  deployInfo: DeployResponse | null,
  activeBoardName: string | null,
): SidebarSectionConfig[] {
  // ... 与原文完全一致，加 export
}
```

- [ ] **Step 3: 更新 `App.tsx`**

添加 import：
```typescript
import { buildInitialProjectDrafts, buildProjectAst, CURRENT_USER_NAME, DEFAULT_BOARD_ID, type ProjectDraft } from './lib/demo-data';
import { buildSidebarSections } from './lib/sidebar';
```

删除 `App.tsx` 中的 `ProjectDraft` 接口、`CURRENT_USER_NAME`、`DEFAULT_BOARD_ID` 常量，以及行 197-423 的所有函数。

- [ ] **Step 4: 验证构建**

```bash
npm --prefix web run build
```

- [ ] **Step 5: 提交**

```bash
git add web/src/lib/demo-data.ts web/src/lib/sidebar.ts web/src/App.tsx
git commit -s -m "refactor: 提取 demo-data.ts 和 sidebar.ts"
```

---

### Task 7: 提取 `use-settings.ts` hook

**Files:**
- Create: `web/src/hooks/use-settings.ts`
- Modify: `web/src/App.tsx`

- [ ] **Step 1: 创建 `web/src/hooks/use-settings.ts`**

将 `App.tsx` 中主题/密度/动效/启动页相关的 useState + useEffect 提取为自定义 hook：

```typescript
//! 桌面偏好设置管理 hook。
//!
//! 管理主题、强调色、UI 密度、动效和启动页的状态，
//! 负责将偏好写入 localStorage 并同步 CSS 变量/data 属性。

import { useEffect, useMemo, useState } from 'react';

import type { MotionMode, StartupPage, ThemeMode, UiDensity } from '../components/app/types';
import type { AccentPreset } from '../lib/theme';
import {
  ACCENT_PRESET_OPTIONS,
  buildAccentThemeVariables,
  getAccentHex,
  normalizeCustomAccentHex,
} from '../lib/theme';
import {
  ACCENT_PRESET_STORAGE_KEY,
  CUSTOM_ACCENT_STORAGE_KEY,
  MOTION_MODE_STORAGE_KEY,
  STARTUP_PAGE_STORAGE_KEY,
  THEME_STORAGE_KEY,
  UI_DENSITY_STORAGE_KEY,
  getInitialAccentPreset,
  getInitialCustomAccentHex,
  getInitialMotionMode,
  getInitialStartupPage,
  getInitialThemeMode,
  getInitialUiDensity,
} from '../lib/settings';

export interface SettingsState {
  themeMode: ThemeMode;
  accentPreset: AccentPreset;
  customAccentHex: string;
  accentHex: string;
  densityMode: UiDensity;
  motionMode: MotionMode;
  startupPage: StartupPage;
}

export interface SettingsActions {
  setThemeMode: (mode: ThemeMode) => void;
  setAccentPreset: (preset: AccentPreset) => void;
  setCustomAccentHex: (hex: string) => void;
  setDensityMode: (mode: UiDensity) => void;
  setMotionMode: (mode: MotionMode) => void;
  setStartupPage: (page: StartupPage) => void;
  toggleTheme: () => void;
}

export function useSettings(): SettingsState & SettingsActions {
  const [themeMode, setThemeMode] = useState<ThemeMode>(getInitialThemeMode);
  const [accentPreset, setAccentPreset] = useState<AccentPreset>(getInitialAccentPreset);
  const [customAccentHex, setCustomAccentHexRaw] = useState<string>(getInitialCustomAccentHex);
  const [densityMode, setDensityMode] = useState<UiDensity>(getInitialUiDensity);
  const [motionMode, setMotionMode] = useState<MotionMode>(getInitialMotionMode);
  const [startupPage, setStartupPage] = useState<StartupPage>(getInitialStartupPage);

  const accentHex = useMemo(
    () => getAccentHex(accentPreset, customAccentHex),
    [accentPreset, customAccentHex],
  );
  const accentThemeVariables = useMemo(
    () => buildAccentThemeVariables(accentHex, themeMode),
    [accentHex, themeMode],
  );

  useEffect(() => {
    document.documentElement.dataset.theme = themeMode;
    try {
      window.localStorage.setItem(THEME_STORAGE_KEY, themeMode);
    } catch { /* 忽略 */ }
  }, [themeMode]);

  useEffect(() => {
    Object.entries(accentThemeVariables).forEach(([key, value]) => {
      document.documentElement.style.setProperty(key, value);
    });
    try {
      window.localStorage.setItem(ACCENT_PRESET_STORAGE_KEY, accentPreset);
      window.localStorage.setItem(CUSTOM_ACCENT_STORAGE_KEY, customAccentHex);
    } catch { /* 忽略 */ }
  }, [accentPreset, accentThemeVariables, customAccentHex]);

  useEffect(() => {
    document.documentElement.dataset.uiDensity = densityMode;
    try {
      window.localStorage.setItem(UI_DENSITY_STORAGE_KEY, densityMode);
    } catch { /* 忽略 */ }
  }, [densityMode]);

  useEffect(() => {
    document.documentElement.dataset.motionMode = motionMode;
    try {
      window.localStorage.setItem(MOTION_MODE_STORAGE_KEY, motionMode);
    } catch { /* 忽略 */ }
  }, [motionMode]);

  useEffect(() => {
    try {
      window.localStorage.setItem(STARTUP_PAGE_STORAGE_KEY, startupPage);
    } catch { /* 忽略 */ }
  }, [startupPage]);

  function setCustomAccentHex(hex: string) {
    setAccentPreset('custom');
    setCustomAccentHexRaw(normalizeCustomAccentHex(hex));
  }

  function toggleTheme() {
    setThemeMode((current) => (current === 'dark' ? 'light' : 'dark'));
  }

  return {
    themeMode, accentPreset, customAccentHex, accentHex,
    densityMode, motionMode, startupPage,
    setThemeMode, setAccentPreset, setCustomAccentHex,
    setDensityMode, setMotionMode, setStartupPage, toggleTheme,
  };
}
```

- [ ] **Step 2: 更新 `App.tsx`**

添加 import：
```typescript
import { useSettings } from './hooks/use-settings';
```

在 `App()` 函数开头替换所有主题/设置相关的 useState 和 useEffect 为：
```typescript
const settings = useSettings();
```

将 `themeMode`、`setThemeMode`、`accentPreset`、`setAccentPreset` 等变量改为 `settings.themeMode`、`settings.setThemeMode` 等（或解构）。删除已移入 hook 的 useEffect 块。

- [ ] **Step 3: 验证构建**

```bash
npm --prefix web run build
```

- [ ] **Step 4: 提交**

```bash
git add web/src/hooks/use-settings.ts web/src/App.tsx
git commit -s -m "refactor: 提取 use-settings hook——偏好设置状态管理"
```

---

### Task 8: 提取 `use-workflow-engine.ts` hook

**Files:**
- Create: `web/src/hooks/use-workflow-engine.ts`
- Modify: `web/src/App.tsx`

- [ ] **Step 1: 创建 `web/src/hooks/use-workflow-engine.ts`**

将 `App.tsx` 中工作流生命周期相关的状态和逻辑提取为自定义 hook。包括：
- `deployInfo`、`results`、`eventFeed`、`appErrors`、`connections`、`runtimeState` 状态
- `appendRuntimeLog`、`appendAppError` 辅助函数
- Tauri 事件监听 useEffect（`onWorkflowEvent`/`onWorkflowResult`/`onWorkflowDeployed`/`onWorkflowUndeployed`）
- 全局错误捕获 useEffect（`window.addEventListener('error')`）
- `resetWorkspaceRuntime`、`refreshConnections` 函数

Hook 返回接口：

```typescript
export interface WorkflowEngineState {
  statusMessage: string;
  deployInfo: DeployResponse | null;
  results: WorkflowResult[];
  eventFeed: RuntimeLogEntry[];
  appErrors: AppErrorRecord[];
  connections: ConnectionRecord[];
  runtimeState: WorkflowRuntimeState;
}

export interface WorkflowEngineActions {
  setStatusMessage: (message: string) => void;
  appendRuntimeLog: (source: string, level: RuntimeLogEntry['level'], message: string, detail?: string | null) => void;
  appendAppError: (scope: AppErrorRecord['scope'], title: string, detail?: string | null) => void;
  resetWorkspaceRuntime: (nextMessage: string) => void;
  refreshConnections: () => Promise<void>;
}
```

完整 hook 实现从 `App.tsx` 中移出对应的 useState 和 useEffect 块。其中事件监听 useEffect（原行 838-948）和全局错误捕获 useEffect（原行 805-836）完整移入。

- [ ] **Step 2: 更新 `App.tsx`**

```typescript
import { useWorkflowEngine } from './hooks/use-workflow-engine';
```

在 `App()` 中：
```typescript
const engine = useWorkflowEngine();
```

将 `deployInfo`、`results`、`eventFeed` 等改为从 `engine` 解构。删除已移入的 useState 和 useEffect 块。

- [ ] **Step 3: 验证构建**

```bash
npm --prefix web run build
```

- [ ] **Step 4: 运行全部现有测试确保未破坏**

```bash
npm --prefix web run test
```

- [ ] **Step 5: 提交**

```bash
git add web/src/hooks/use-workflow-engine.ts web/src/App.tsx
git commit -s -m "refactor: 提取 use-workflow-engine hook——工作流生命周期管理"
```

---

### Task 9: 编写 graph.ts 单元测试

**Files:**
- Create: `web/src/lib/__tests__/parse-graph.test.ts`
- Create: `web/src/lib/__tests__/layout-graph.test.ts`

- [ ] **Step 1: 创建 `web/src/lib/__tests__/parse-graph.test.ts`**

```typescript
import { describe, expect, it } from 'vitest';

import { parseWorkflowGraph } from '../graph';

describe('parseWorkflowGraph', () => {
  it('合法 JSON 返回 graph', () => {
    const source = JSON.stringify({
      nodes: { a: { type: 'native' } },
      edges: [],
    });
    const result = parseWorkflowGraph(source);
    expect(result.error).toBeNull();
    expect(result.graph).not.toBeNull();
    expect(result.graph?.nodes).toHaveProperty('a');
  });

  it('缺少 nodes 返回错误', () => {
    const result = parseWorkflowGraph(JSON.stringify({ edges: [] }));
    expect(result.graph).toBeNull();
    expect(result.error).toContain('nodes');
  });

  it('非法 JSON 返回解析错误', () => {
    const result = parseWorkflowGraph('{invalid');
    expect(result.graph).toBeNull();
    expect(result.error).toBeTruthy();
  });

  it('空字符串返回错误', () => {
    const result = parseWorkflowGraph('');
    expect(result.graph).toBeNull();
    expect(result.error).toBeTruthy();
  });
});
```

- [ ] **Step 2: 创建 `web/src/lib/__tests__/layout-graph.test.ts`**

```typescript
import { describe, expect, it } from 'vitest';

import { layoutGraph } from '../graph';
import type { WorkflowGraph } from '../../types';

function graph(nodes: Record<string, { type: string }>, edges: { from: string; to: string }[]): WorkflowGraph {
  return { nodes, edges } as WorkflowGraph;
}

describe('layoutGraph', () => {
  it('线性链层级递增', () => {
    const result = layoutGraph(graph(
      { a: { type: 'native' }, b: { type: 'native' }, c: { type: 'native' } },
      [{ from: 'a', to: 'b' }, { from: 'b', to: 'c' }],
    ));
    const layers = Object.fromEntries(result.map((n) => [n.id, n.layer]));
    expect(layers.a).toBe(0);
    expect(layers.b).toBe(1);
    expect(layers.c).toBe(2);
  });

  it('分叉 DAG 同层', () => {
    const result = layoutGraph(graph(
      { a: { type: 'native' }, b: { type: 'native' }, c: { type: 'native' } },
      [{ from: 'a', to: 'b' }, { from: 'a', to: 'c' }],
    ));
    const layers = Object.fromEntries(result.map((n) => [n.id, n.layer]));
    expect(layers.b).toBe(layers.c);
    expect(layers.a).toBe(0);
  });

  it('孤立节点 layer 为 0', () => {
    const result = layoutGraph(graph(
      { x: { type: 'native' } },
      [],
    ));
    expect(result[0].layer).toBe(0);
  });

  it('返回正确的 type', () => {
    const result = layoutGraph(graph(
      { a: { type: 'timer' } },
      [],
    ));
    expect(result[0].type).toBe('timer');
  });
});
```

- [ ] **Step 3: 运行测试**

```bash
npm --prefix web run test
```

- [ ] **Step 4: 提交**

```bash
git add web/src/lib/__tests__/parse-graph.test.ts web/src/lib/__tests__/layout-graph.test.ts
git commit -s -m "test: parseWorkflowGraph + layoutGraph 单元测试"
```

---

### Task 10: 编写 flowgram 转换单元测试

**Files:**
- Create: `web/src/lib/__tests__/nazh-to-flowgram.test.ts`
- Create: `web/src/lib/__tests__/flowgram-to-nazh.test.ts`

- [ ] **Step 1: 创建 `web/src/lib/__tests__/nazh-to-flowgram.test.ts`**

```typescript
import { describe, expect, it } from 'vitest';

import { toFlowgramWorkflowJson } from '../flowgram';
import type { WorkflowGraph } from '../../types';

function simpleGraph(): WorkflowGraph {
  return {
    nodes: {
      a: { type: 'native', meta: { position: { x: 0, y: 0 } } },
      b: { type: 'rhai', meta: { position: { x: 300, y: 0 } } },
    },
    edges: [{ from: 'a', to: 'b' }],
  } as WorkflowGraph;
}

describe('toFlowgramWorkflowJson', () => {
  it('节点数量一致', () => {
    const result = toFlowgramWorkflowJson(simpleGraph());
    expect(result.nodes).toHaveLength(2);
  });

  it('边数量一致', () => {
    const result = toFlowgramWorkflowJson(simpleGraph());
    expect(result.edges).toHaveLength(1);
  });

  it('节点 data 包含 nodeType', () => {
    const result = toFlowgramWorkflowJson(simpleGraph());
    const nodeA = result.nodes.find((n) => n.id === 'a');
    expect((nodeA?.data as Record<string, unknown>)?.nodeType).toBe('native');
  });

  it('边映射 from/to → sourceNodeID/targetNodeID', () => {
    const result = toFlowgramWorkflowJson(simpleGraph());
    expect(result.edges[0].sourceNodeID).toBe('a');
    expect(result.edges[0].targetNodeID).toBe('b');
  });

  it('保留 meta.position 坐标', () => {
    const result = toFlowgramWorkflowJson(simpleGraph());
    const nodeA = result.nodes.find((n) => n.id === 'a');
    expect(nodeA?.meta?.position).toEqual({ x: 0, y: 0 });
  });
});
```

- [ ] **Step 2: 创建 `web/src/lib/__tests__/flowgram-to-nazh.test.ts`**

```typescript
import { describe, expect, it } from 'vitest';

import { toFlowgramWorkflowJson, toNazhWorkflowGraph } from '../flowgram';
import type { WorkflowGraph } from '../../types';

function baseGraph(): WorkflowGraph {
  return {
    name: 'test-flow',
    connections: [{ id: 'conn-1', type: 'modbus', metadata: {} }],
    nodes: {
      a: { type: 'native', config: { message: 'hello' } },
      b: { type: 'rhai', config: { script: 'payload' } },
    },
    edges: [{ from: 'a', to: 'b' }],
  } as WorkflowGraph;
}

describe('toNazhWorkflowGraph', () => {
  it('往返保留核心字段', () => {
    const original = baseGraph();
    const flowgram = toFlowgramWorkflowJson(original);
    const roundtripped = toNazhWorkflowGraph(flowgram, original);

    expect(Object.keys(roundtripped.nodes)).toHaveLength(2);
    expect(roundtripped.edges).toHaveLength(1);
    expect(roundtripped.nodes.a?.type).toBe('native');
    expect(roundtripped.nodes.b?.type).toBe('rhai');
  });

  it('继承 previousGraph 的 name 和 connections', () => {
    const original = baseGraph();
    const flowgram = toFlowgramWorkflowJson(original);
    const result = toNazhWorkflowGraph(flowgram, original);

    expect(result.name).toBe('test-flow');
    expect(result.connections).toEqual(original.connections);
  });

  it('保留 editor_graph 引用', () => {
    const original = baseGraph();
    const flowgram = toFlowgramWorkflowJson(original);
    const result = toNazhWorkflowGraph(flowgram, original);

    expect(result.editor_graph).toBe(flowgram);
  });

  it('继承 previousGraph 的 config', () => {
    const original = baseGraph();
    const flowgram = toFlowgramWorkflowJson(original);
    const result = toNazhWorkflowGraph(flowgram, original);

    expect(result.nodes.a?.config).toEqual({ message: 'hello' });
  });
});
```

- [ ] **Step 3: 运行测试**

```bash
npm --prefix web run test
```

- [ ] **Step 4: 提交**

```bash
git add web/src/lib/__tests__/nazh-to-flowgram.test.ts web/src/lib/__tests__/flowgram-to-nazh.test.ts
git commit -s -m "test: flowgram 双向转换单元测试"
```

---

### Task 11: 安装 Playwright 并配置

**Files:**
- Modify: `web/package.json`
- Create: `web/e2e/playwright.config.ts`

- [ ] **Step 1: 安装 Playwright**

```bash
npm --prefix web install -D @playwright/test
npx --prefix web playwright install --with-deps chromium
```

- [ ] **Step 2: 创建 `web/e2e/playwright.config.ts`**

```typescript
import { defineConfig } from '@playwright/test';

export default defineConfig({
  testDir: '.',
  timeout: 60_000,
  retries: 0,
  use: {
    baseURL: 'http://localhost:1420',
    screenshot: 'only-on-failure',
  },
  webServer: {
    command: 'cd ../src-tauri && ../web/node_modules/.bin/tauri dev --no-watch',
    url: 'http://localhost:1420',
    timeout: 120_000,
    reuseExistingServer: true,
  },
});
```

- [ ] **Step 3: 在 `web/package.json` 添加 script**

```json
"test:e2e": "playwright test --config e2e/playwright.config.ts"
```

- [ ] **Step 4: 提交**

```bash
git add web/package.json web/package-lock.json web/e2e/playwright.config.ts
git commit -s -m "chore: 安装并配置 Playwright E2E 测试框架"
```

---

### Task 12: 添加 data-testid 属性

**Files:**
- Modify: `web/src/App.tsx`
- Modify: `web/src/components/app/RuntimeDock.tsx`

在以下关键元素上添加 `data-testid`：

- [ ] **Step 1: 在 App.tsx 和子组件中添加 data-testid**

在 `App.tsx` 的 JSX 中，为以下元素添加 `data-testid`：
- 状态标签区域：`data-testid="workflow-status"`
- 错误显示区域：`data-testid="error-display"`

在 `SidebarNav` 渲染的各侧栏项上，通过 `key` 属性匹配添加 `data-testid={`sidebar-${section.key}`}`。

在 `SourcePanel` 中：
- AST 编辑器 textarea：`data-testid="ast-editor"`

在 `PayloadPanel` 中：
- 发送按钮：`data-testid="dispatch-button"`

在 `RuntimeDock` 中：
- 事件日志容器：`data-testid="event-feed"`
- 结果列表容器：`data-testid="result-list"`

在工具栏中：
- 部署按钮：`data-testid="deploy-button"`
- 卸载按钮：`data-testid="undeploy-button"`

具体添加位置需根据各组件 JSX 结构确定——在对应元素上加 `data-testid="..."` 属性即可。

- [ ] **Step 2: 验证构建**

```bash
npm --prefix web run build
```

- [ ] **Step 3: 提交**

```bash
git add web/src/
git commit -s -m "chore: 添加 E2E 测试所需的 data-testid 属性"
```

---

### Task 13: 编写 E2E 测试

**Files:**
- Create: `web/e2e/deploy-and-dispatch.spec.ts`
- Create: `web/e2e/lifecycle.spec.ts`
- Create: `web/e2e/error-handling.spec.ts`

- [ ] **Step 1: 创建 `web/e2e/deploy-and-dispatch.spec.ts`**

```typescript
import { expect, test } from '@playwright/test';

test('部署 AST 并发送 Payload 后收到事件和结果', async ({ page }) => {
  await page.goto('/');
  await page.waitForLoadState('networkidle');

  // 进入默认工程
  const boardEntry = page.locator('[data-testid="board-entry"]').first();
  if (await boardEntry.isVisible()) {
    await boardEntry.click();
  }

  // 部署
  const deployButton = page.locator('[data-testid="deploy-button"]');
  await expect(deployButton).toBeVisible({ timeout: 10_000 });
  await deployButton.click();

  // 验证状态变为已部署
  await expect(page.locator('[data-testid="workflow-status"]')).toContainText('已部署', {
    timeout: 15_000,
  });

  // 发送 Payload
  const dispatchButton = page.locator('[data-testid="dispatch-button"]');
  await expect(dispatchButton).toBeVisible();
  await dispatchButton.click();

  // 验证事件日志出现
  await expect(page.locator('[data-testid="event-feed"]')).toContainText('节点', {
    timeout: 10_000,
  });
});
```

- [ ] **Step 2: 创建 `web/e2e/lifecycle.spec.ts`**

```typescript
import { expect, test } from '@playwright/test';

test('部署 → 卸载 → 重新部署', async ({ page }) => {
  await page.goto('/');
  await page.waitForLoadState('networkidle');

  // 进入工程
  const boardEntry = page.locator('[data-testid="board-entry"]').first();
  if (await boardEntry.isVisible()) {
    await boardEntry.click();
  }

  // 第一次部署
  const deployButton = page.locator('[data-testid="deploy-button"]');
  await expect(deployButton).toBeVisible({ timeout: 10_000 });
  await deployButton.click();
  await expect(page.locator('[data-testid="workflow-status"]')).toContainText('已部署', {
    timeout: 15_000,
  });

  // 卸载
  const undeployButton = page.locator('[data-testid="undeploy-button"]');
  await undeployButton.click();
  await expect(page.locator('[data-testid="workflow-status"]')).toContainText('未部署', {
    timeout: 10_000,
  });

  // 重新部署
  await deployButton.click();
  await expect(page.locator('[data-testid="workflow-status"]')).toContainText('已部署', {
    timeout: 15_000,
  });
});
```

- [ ] **Step 3: 创建 `web/e2e/error-handling.spec.ts`**

```typescript
import { expect, test } from '@playwright/test';

test('部署非法 JSON 显示错误', async ({ page }) => {
  await page.goto('/');
  await page.waitForLoadState('networkidle');

  // 进入工程
  const boardEntry = page.locator('[data-testid="board-entry"]').first();
  if (await boardEntry.isVisible()) {
    await boardEntry.click();
  }

  // 导航到 Source 面板
  await page.locator('[data-testid="sidebar-source"]').click();

  // 清空并输入非法 JSON
  const editor = page.locator('[data-testid="ast-editor"]');
  await expect(editor).toBeVisible({ timeout: 5_000 });
  await editor.fill('{invalid json!!!');

  // 尝试部署
  const deployButton = page.locator('[data-testid="deploy-button"]');
  await deployButton.click();

  // 验证错误提示
  await expect(page.locator('[data-testid="error-display"]')).toBeVisible({ timeout: 5_000 });
});
```

- [ ] **Step 4: 提交**

```bash
git add web/e2e/
git commit -s -m "test: Playwright E2E 测试——核心链路、生命周期、错误处理"
```

---

### Task 14: 更新文档

**Files:**
- Modify: `CLAUDE.md`
- Modify: `README.md`
- Modify: `AI-Context.md`

- [ ] **Step 1: 更新 `CLAUDE.md`**

在 Build & Dev Commands 中添加：
```
# Run frontend unit tests
npm --prefix web run test

# Run frontend E2E tests (requires compiled Tauri app)
npm --prefix web run test:e2e
```

在 Testing 章节补充：
```
Frontend unit tests live in `web/src/lib/__tests__/` (Vitest). E2E tests in `web/e2e/` (Playwright, full Tauri desktop window).
```

在 Frontend Key Files 中补充 `hooks/` 和新增 lib 文件。

- [ ] **Step 2: 更新 `README.md`**

在"当前完成度"列表添加：
```
- 已建立前端 Vitest 单元测试 + Playwright E2E 测试体系。
```

在"已验证状态"中添加：
```
- `npm --prefix web run test` 通过（Vitest 单元测试）。
```

- [ ] **Step 3: 更新 `AI-Context.md`**

在路线图中添加：
```
* **Phase 5.6: 前端测试体系** -> Vitest 单元测试覆盖核心转换逻辑，Playwright E2E 覆盖完整 Tauri 桌面链路。（已完成）
```

- [ ] **Step 4: 提交**

```bash
git add CLAUDE.md README.md AI-Context.md
git commit -s -m "docs: 更新文档——前端测试体系 + App.tsx 重构"
```
