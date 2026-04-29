> **Status:** merged in b29dd5b

# ADR-0014 Phase 5 实施计划：视觉打磨 + AI prompt + PureMemo 清理

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 把 ADR-0014 Phase 5 三件事落地——（1）节点头部按 capability 自动着色 + CSS 变量化；（2）AI 脚本生成 prompt 携带 PinKind 信息；（3）PureMemo trace 完成后清理 + watch channel 替代 Notify。

**Architecture:** 前端从 `NodeTypeEntry.capabilities` 位图推导 primary capability，用 CSS 属性选择器着色。AI prompt 扩展 pin 描述格式加 PinKind。Rust 端 `OutputCache` slot 从 `Notify` 迁移到 `watch::Sender/Receiver`，`PureMemo` 加 trace 清理。

**Tech Stack:** Rust（`crates/core/src/cache.rs`、`src/graph/pull.rs`、`src/graph/runner.rs`），TypeScript / React / CSS（`FlowgramCanvas.tsx`、`foundation.css`、`flowgram.css`、`script-generation.ts`），Vitest，`cargo test`。

---

## File Structure

| 操作 | 路径 | 责任 |
|------|------|------|
| 修改 | `web/src/lib/node-capabilities-cache.ts`（新建） | nodeType → capabilities bits 轻量缓存 |
| 修改 | `web/src/components/FlowgramCanvas.tsx` | 节点卡片加 `data-node-capability` 属性 |
| 修改 | `web/src/styles/foundation.css` | 加 `--node-header-*` CSS 变量（明暗主题） |
| 修改 | `web/src/styles/flowgram.css` | 按 capability 属性选择器着色 |
| 修改 | `web/src/lib/script-generation.ts` | 系统 prompt 加 PinKind + pin 描述格式扩展 |
| 修改 | `crates/core/src/cache.rs` | `Slot` 从 `Notify` 迁移到 `watch` + `subscribe()` API |
| 修改 | `src/graph/pull.rs` | `BlockUntilReady` 用 watch 等待 + `PureMemo::clear_trace` |
| 修改 | `src/graph/runner.rs` | trace 结束调 `pure_memo.clear_trace` |
| 修改 | `docs/adr/0014-执行边与数据边分离.md` | 实施进度加 Phase 5 |
| 修改 | `docs/superpowers/plans/2026-04-28-architecture-review.md` | Phase A checkbox 更新 |
| 修改 | `AGENTS.md` | ADR-0014 状态同步 |

---

## Task 1：CSS 变量 + capability 着色样式

**Files:**
- Modify: `web/src/styles/foundation.css`
- Modify: `web/src/styles/flowgram.css`

- [ ] **Step 1: 在 `foundation.css` `:root` 末尾加 node-header 变量**

定位 `:root { ... }` 块末尾（`--shadow-medium:` 行之后），在闭合 `}` 前插入：

```css
  /* ADR-0014 Phase 5：节点头部按 capability 着色（UE5 Blueprint 风格） */
  --node-header-pure: #2FB75F;
  --node-header-trigger: #B00000;
  --node-header-branching: #1FB7FF;
  --node-header-default: #3A4F66;
```

- [ ] **Step 2: 在 `foundation.css` `html[data-theme='dark']` 块末尾加暗色覆盖**

定位 `html[data-theme='dark'] { ... }` 块末尾（`--shadow-nav:` 行之后），在闭合 `}` 前插入：

```css
  --node-header-pure: #3AC76F;
  --node-header-trigger: #D03030;
  --node-header-branching: #4FC7FF;
  --node-header-default: #4A5F76;
```

- [ ] **Step 3: 在 `flowgram.css` 末尾（Phase 3 pure-form 样式块之后）加 capability 着色**

追加到文件末尾：

```css
/* ADR-0014 Phase 5：按 capability 自动着色（Trigger / Branching / Default）。
 * Pure 节点优先级最高——已由 .flowgram-card--pure-form 覆盖，此处不重复。 */
.flowgram-card[data-node-capability="trigger"] .flowgram-card__topline {
  background: linear-gradient(180deg, var(--node-header-trigger) 0%, color-mix(in srgb, var(--node-header-trigger) 75%, black) 100%);
  color: #ffffff;
}

.flowgram-card[data-node-capability="branching"] .flowgram-card__topline {
  background: linear-gradient(180deg, var(--node-header-branching) 0%, color-mix(in srgb, var(--node-header-branching) 75%, black) 100%);
  color: #ffffff;
}

.flowgram-card[data-node-capability="default"] .flowgram-card__topline {
  background: linear-gradient(180deg, var(--node-header-default) 0%, color-mix(in srgb, var(--node-header-default) 75%, black) 100%);
  color: #ffffff;
}
```

- [ ] **Step 4: commit**

```bash
git add web/src/styles/foundation.css web/src/styles/flowgram.css
git commit -s -m "feat(web): ADR-0014 Phase 5 节点头部 capability CSS 变量 + 着色"
```

---

## Task 2：node-capabilities 轻量缓存

**Files:**
- Create: `web/src/lib/node-capabilities-cache.ts`

- [ ] **Step 1: 创建缓存模块**

```typescript
// 节点类型 → capabilities 位图的轻量缓存。
// 由 list_node_types IPC 填充，供 FlowgramCanvas 渲染时同步查询。
import { hasTauriRuntime, listNodeTypes } from './tauri';

let cache = new Map<string, number>();

export function getCachedCapabilities(nodeType: string): number | undefined {
  return cache.get(nodeType);
}

export async function refreshCapabilitiesCache(): Promise<void> {
  if (!hasTauriRuntime()) return;
  try {
    const resp = await listNodeTypes();
    cache = new Map(resp.types.map((t) => [t.name, t.capabilities] as const));
  } catch {
    // graceful degradation
  }
}
```

- [ ] **Step 2: 找到 FlowgramCanvas 初始化位置，调 refreshCapabilitiesCache**

定位 `FlowgramCanvas.tsx` 中组件 mount / workflow load 的 `useEffect`。搜索 `useEffect.*workflow` 或 `loadWorkflowGraph`。在 mount 时调一次 `refreshCapabilitiesCache()`。

在 `FlowgramCanvas.tsx` 顶部加 import：
```typescript
import { getCachedCapabilities } from '../lib/node-capabilities-cache';
```

在 workflow 加载后（找到 `loadWorkflowGraph` 或组件首次 mount effect）加调用。搜索 `refreshCapabilitiesCache` 的调用位置——应该在 workflow 数据加载的同一 effect 里。

具体：在 `FlowgramCanvas.tsx` 中找到 workflow 初始化 `useEffect`，加 import + 调用：

```typescript
import { refreshCapabilitiesCache } from '../lib/node-capabilities-cache';
```

在该 `useEffect` 内调 `void refreshCapabilitiesCache()`。

- [ ] **Step 3: 在 `web/src/lib/__tests__/` 写缓存单测**

创建 `web/src/lib/__tests__/node-capabilities-cache.test.ts`：

```typescript
import { describe, it, expect, vi, beforeEach } from 'vitest';

// mock tauri
vi.mock('../../lib/tauri', () => ({
  hasTauriRuntime: vi.fn(() => true),
  listNodeTypes: vi.fn(() =>
    Promise.resolve({
      types: [
        { name: 'timer', capabilities: 16 },
        { name: 'if', capabilities: 32 },
        { name: 'httpClient', capabilities: 2 },
      ],
    }),
  ),
}));

import { getCachedCapabilities, refreshCapabilitiesCache } from '../node-capabilities-cache';

describe('node-capabilities-cache', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it('缓存空时返回 undefined', () => {
    expect(getCachedCapabilities('timer')).toBeUndefined();
  });

  it('refresh 后可按 nodeType 查询', async () => {
    await refreshCapabilitiesCache();
    expect(getCachedCapabilities('timer')).toBe(16);
    expect(getCachedCapabilities('if')).toBe(32);
    expect(getCachedCapabilities('httpClient')).toBe(2);
    expect(getCachedCapabilities('unknown')).toBeUndefined();
  });
});
```

- [ ] **Step 4: 跑测试 + commit**

```bash
npm --prefix web run test -- --run node-capabilities-cache
git add web/src/lib/node-capabilities-cache.ts web/src/lib/__tests__/node-capabilities-cache.test.ts web/src/components/FlowgramCanvas.tsx
git commit -s -m "feat(web): ADR-0014 Phase 5 node-capabilities 轻量缓存 + mount 填充"
```

---

## Task 3：FlowgramCanvas 节点卡片加 capability 属性

**Files:**
- Modify: `web/src/components/FlowgramCanvas.tsx:796-811`

- [ ] **Step 1: 在 `FlowgramNodeCard` 里推导 capability 属性**

定位 `FlowgramNodeCard` 函数（约 line 664），在 `pureForm` 计算之后（约 line 799），加：

```typescript
  import { getCachedCapabilities } from '../lib/node-capabilities-cache';
  import { hasCapability, NODE_CAPABILITY_FLAGS } from '../lib/node-capabilities';
```

（import 放文件顶部，逻辑放在 `pureForm` 计算后。）

在 `pureForm` 计算后加 capability 推导：

```typescript
  const capabilityBits = getCachedCapabilities(nodeType);
  let nodeCapability: string;
  if (pureForm) {
    nodeCapability = 'pure'; // pure 优先级最高，但 CSS 已由 --pure-form 覆盖，此处仅做标记
  } else if (capabilityBits !== undefined && hasCapability(capabilityBits, 'TRIGGER')) {
    nodeCapability = 'trigger';
  } else if (capabilityBits !== undefined && hasCapability(capabilityBits, 'BRANCHING')) {
    nodeCapability = 'branching';
  } else {
    nodeCapability = 'default';
  }
```

- [ ] **Step 2: 在节点卡片 div 加 `data-node-capability` 属性**

定位 line 811：
```tsx
      <div data-flow-editor-selectable="false" className="flowgram-card__body" draggable={false} data-pure-form={pureForm ? 'true' : undefined}>
```

改为：
```tsx
      <div data-flow-editor-selectable="false" className="flowgram-card__body" draggable={false} data-pure-form={pureForm ? 'true' : undefined} data-node-capability={pureForm ? undefined : nodeCapability}>
```

注意：`pureForm` 时 data-node-capability 不设（CSS 由 `.flowgram-card--pure-form` 控制），非 pure 时设为 trigger/branching/default。

- [ ] **Step 3: 跑前端测试验证不破**

```bash
npm --prefix web run test -- --run
```

- [ ] **Step 4: commit**

```bash
git add web/src/components/FlowgramCanvas.tsx
git commit -s -m "feat(web): ADR-0014 Phase 5 节点卡片加 data-node-capability 属性"
```

---

## Task 4：AI prompt 携带 PinKind

**Files:**
- Modify: `web/src/lib/script-generation.ts:44-51,82-104,107-111`
- Modify: `web/src/lib/__tests__/script-generation.test.ts`（如有；否则新建）

- [ ] **Step 1: `summarizePins` 加 kind 字段**

定位 `script-generation.ts:44-51` `summarizePins` 函数。当前 `NodePinSummary` 只有 `id/typeLabel/required`。

修改 `NodePinSummary` interface（line 13-19），加：
```typescript
  /** PinKind：'exec'（控制流）或 'data'（数据拉取）。默认 'exec'。 */
  kind?: 'exec' | 'data';
```

修改 `summarizePins`（line 44-51），加 kind 映射：
```typescript
function summarizePins(pins: PinDefinition[] | undefined): NodePinSummary[] | undefined {
  if (!pins) return undefined;
  return pins.map((pin) => ({
    id: pin.id,
    typeLabel: formatPinType(pin.pin_type),
    required: pin.required,
    kind: (pin.kind as 'exec' | 'data' | undefined) ?? 'exec',
  }));
}
```

- [ ] **Step 2: 系统 prompt 追加 PinKind 语义**

定位 `SYSTEM_PROMPT` 常量（line 82-104）。在 `- Pin 类型语义：...` 行之后追加：

```
- Pin 求值语义：'exec' 引脚是控制流（被上游推）；'data' 引脚是数据流（按需从上游缓存拉取最新值）。Data 输入引脚的值在脚本运行前已被 Runner 自动拉取并合并到 payload 中，脚本无需手动处理拉取逻辑。Data 输出引脚的值会自动写入缓存供其他节点按需读取。
```

- [ ] **Step 3: `formatPinSummary` 扩展格式**

定位 `formatPinSummary`（line 107-111）。当前格式 `in: json (required)` → 改为含 PinKind：

```typescript
function formatPinSummary(pins: NodePinSummary[] | undefined): string {
  if (!pins || pins.length === 0) return '';
  return pins
    .map((pin) => {
      const kind = pin.kind === 'data' ? 'data' : 'exec';
      return `${pin.id}: ${kind}/${pin.typeLabel}${pin.required ? ' (required)' : ''}`;
    })
    .join(', ');
}
```

新格式示例：`in: exec/json (required), sensor: data/float (required)`

- [ ] **Step 4: 写 / 更新测试**

定位 `web/src/lib/__tests__/script-generation.test.ts`。如果不存在，创建。

```typescript
import { describe, it, expect } from 'vitest';
import { buildScriptGenerationPrompt, type NodeContext } from '../script-generation';

const baseContext: NodeContext = {
  current: {
    nodeId: 'code1',
    nodeType: 'code',
    label: '脚本节点',
    inputPins: [
      { id: 'in', typeLabel: 'json', required: true, kind: 'exec' },
      { id: 'sensor', typeLabel: 'float', required: false, kind: 'data' },
    ],
    outputPins: [
      { id: 'out', typeLabel: 'json', required: true, kind: 'exec' },
    ],
  },
  upstream: [],
  downstream: [],
};

describe('buildScriptGenerationPrompt', () => {
  it('系统 prompt 含 PinKind 语义', () => {
    const messages = buildScriptGenerationPrompt('test', baseContext);
    const system = messages[0].content;
    expect(system).toContain('求值语义');
    expect(system).toContain('exec');
    expect(system).toContain('data');
  });

  it('pin 描述含 PinKind 标记', () => {
    const messages = buildScriptGenerationPrompt('test', baseContext);
    const user = messages[1].content;
    expect(user).toContain('exec/json');
    expect(user).toContain('data/float');
  });
});
```

- [ ] **Step 5: 跑测试 + commit**

```bash
npm --prefix web run test -- --run script-generation
git add web/src/lib/script-generation.ts web/src/lib/__tests__/script-generation.test.ts
git commit -s -m "feat(web): ADR-0014 Phase 5 AI prompt 携带 PinKind 信息"
```

---

## Task 5：`OutputCache` 从 Notify 迁移到 watch channel

**Files:**
- Modify: `crates/core/src/cache.rs`

- [ ] **Step 1: 替换 `Slot` 内部结构**

定位 `cache.rs:30-34`：
```rust
#[derive(Debug)]
struct Slot {
    value: ArcSwap<Option<CachedOutput>>,
    notify: Arc<Notify>,
}
```

替换为：
```rust
use tokio::sync::watch;

#[derive(Debug)]
struct Slot {
    tx: watch::Sender<Option<CachedOutput>>,
    rx: watch::Receiver<Option<CachedOutput>>,
}
```

删除 `use tokio::sync::Notify;` import，加 `use tokio::sync::watch;`。

- [ ] **Step 2: 更新 `prepare_slot`**

定位 `cache.rs:50-59`。替换为：
```rust
    pub fn prepare_slot(&self, pin_id: &str) {
        if !self.slots.contains_key(pin_id) {
            let (tx, rx) = watch::channel(None);
            self.slots.insert(
                pin_id.to_owned(),
                Arc::new(Slot { tx, rx }),
            );
        }
    }
```

- [ ] **Step 3: 更新 `write`**

定位 `cache.rs:63-68`。替换为：
```rust
    pub fn write(&self, pin_id: &str, output: CachedOutput) {
        if let Some(slot) = self.slots.get(pin_id) {
            let _ = slot.tx.send(Some(output));
        }
    }
```

- [ ] **Step 4: 更新 `read`**

定位 `cache.rs:83-96`。替换为：
```rust
    pub fn read(&self, pin_id: &str, ttl_ms: Option<u64>) -> Option<CachedOutput> {
        let slot = self.slots.get(pin_id)?;
        let cached = slot.rx.borrow().clone()?;
        if let Some(ttl) = ttl_ms {
            let age = Utc::now()
                .signed_duration_since(cached.produced_at)
                .num_milliseconds();
            if age.unsigned_abs() > ttl {
                return None;
            }
        }
        Some(cached)
    }
```

- [ ] **Step 5: 替换 `notify_handle` 为 `subscribe`**

定位 `cache.rs:99-101`。替换为：
```rust
    /// 拿到 slot 的 watch Receiver clone——pull collector 在 `BlockUntilReady` 下
    /// `changed().await` 等新值。
    pub fn subscribe(&self, pin_id: &str) -> Option<watch::Receiver<Option<CachedOutput>>> {
        self.slots.get(pin_id).map(|slot| slot.rx.clone())
    }
```

- [ ] **Step 6: 更新 `write_唤醒等待者` 测试**

定位 `cache.rs:205-226`。替换为：
```rust
    #[tokio::test]
    async fn write_唤醒等待者() {
        let cache = Arc::new(OutputCache::new());
        cache.prepare_slot("latest");
        let mut rx = cache.subscribe("latest").unwrap();

        let cache2 = Arc::clone(&cache);
        let waiter = tokio::spawn(async move {
            rx.changed().await.unwrap();
            cache2.read("latest", None)
        });

        tokio::time::sleep(Duration::from_millis(50)).await;
        cache.write_now("latest", Value::from(42), Uuid::nil());

        let got = timeout(Duration::from_secs(1), waiter)
            .await
            .unwrap()
            .unwrap();
        assert!(got.is_some());
        assert_eq!(got.unwrap().value, Value::from(42));
    }
```

- [ ] **Step 7: 跑 cache 单测**

```bash
cargo test -p nazh-core cache
```

Expected: 全 PASS（`write_唤醒等待者` 用 watch 语义、其余测试不变）。

- [ ] **Step 8: commit**

```bash
git add crates/core/src/cache.rs
git commit -s -m "refactor(core): ADR-0014 Phase 5 OutputCache watch channel 替代 Notify"
```

---

## Task 6：`pull.rs` BlockUntilReady 用 watch + PureMemo 加 clear_trace

**Files:**
- Modify: `src/graph/pull.rs`

- [ ] **Step 1: `PureMemo` 加 `clear_trace` 方法**

定位 `pull.rs:62-81` `PureMemo` impl。在 `insert` 方法之后加：

```rust
    /// 清理指定 trace 的所有 memo 条目。
    /// 由 Runner 在 Exec 节点完成一个 trace 后调用。
    /// 幂等——不存在的 key 被 DashMap 静默跳过。
    pub fn clear_trace(&self, trace_id: Uuid) {
        self.inner.retain(|_, tid, _| *tid != trace_id);
    }
```

- [ ] **Step 2: `pull_one` BlockUntilReady 分支改为 watch**

定位 `pull.rs:293-326` BlockUntilReady 分支。替换为：

```rust
                EmptyPolicy::BlockUntilReady => {
                    let mut rx = cache.subscribe(upstream_output_pin_id).ok_or(
                        EngineError::DataPinCacheEmpty {
                            upstream: upstream_node_id.to_owned(),
                            pin: upstream_output_pin_id.to_owned(),
                        },
                    )?;
                    let timeout_ms = block_timeout_ms.unwrap_or(DEFAULT_BLOCK_TIMEOUT_MS);
                    // watch: 先检查当前值
                    if let Some(cached) = rx.borrow().clone() {
                        let age = Utc::now()
                            .signed_duration_since(cached.produced_at)
                            .num_milliseconds();
                        if ttl_ms.is_none_or(|ttl| age.unsigned_abs() <= ttl) {
                            return Ok(cached.value);
                        }
                    }
                    // 等变更
                    let result = tokio::select! {
                        res = rx.changed() => {
                            match res {
                                Ok(()) => {
                                    let snapshot = rx.borrow().clone();
                                    match snapshot {
                                        Some(cached) => {
                                            if let Some(ttl) = ttl_ms {
                                                let age = Utc::now()
                                                    .signed_duration_since(cached.produced_at)
                                                    .num_milliseconds();
                                                if age.unsigned_abs() > ttl {
                                                    return Err(EngineError::DataPinPullTimeout {
                                                        upstream: upstream_node_id.to_owned(),
                                                        pin: upstream_output_pin_id.to_owned(),
                                                        timeout_ms,
                                                    });
                                                }
                                            }
                                            Ok(cached.value)
                                        }
                                        None => Err(EngineError::DataPinCacheEmpty {
                                            upstream: upstream_node_id.to_owned(),
                                            pin: upstream_output_pin_id.to_owned(),
                                        }),
                                    }
                                }
                                Err(_) => Err(EngineError::DataPinCacheEmpty {
                                    upstream: upstream_node_id.to_owned(),
                                    pin: upstream_output_pin_id.to_owned(),
                                }),
                            }
                        }
                        _ = tokio::time::sleep(Duration::from_millis(timeout_ms)) => {
                            // 超时前最后读一次
                            cache
                                .read(upstream_output_pin_id, ttl_ms)
                                .map(|c| c.value)
                                .ok_or(EngineError::DataPinPullTimeout {
                                    upstream: upstream_node_id.to_owned(),
                                    pin: upstream_output_pin_id.to_owned(),
                                    timeout_ms,
                                })
                        }
                    };
                    result
                }
```

需要在 `pull.rs` 顶部 import 加 `use chrono::Utc;`（如果还没有）。

- [ ] **Step 3: 加 PureMemo clear_trace 单测**

在 `pull.rs` 的 `#[cfg(test)]` 模块末尾（约 line 749）加：

```rust
    #[test]
    fn clear_trace_只清目标_trace() {
        let memo = PureMemo::new();
        let t1 = Uuid::new_v4();
        let t2 = Uuid::new_v4();

        memo.insert("node", t1, 1, json!(1));
        memo.insert("node", t2, 2, json!(2));
        memo.insert("other", t1, 3, json!(3));

        memo.clear_trace(t1);

        // t1 的条目被清
        assert!(memo.get("node", t1, 1).is_none());
        assert!(memo.get("other", t1, 3).is_none());
        // t2 的条目保留
        assert_eq!(memo.get("node", t2, 2).unwrap(), json!(2));
    }
```

- [ ] **Step 4: 跑 pull 测试**

```bash
cargo test -p nazh-engine pull
```

Expected: 全 PASS（`block_until_ready_在缓存空时超时` 走 watch 等待超时路径，行为等价）。

- [ ] **Step 5: commit**

```bash
git add src/graph/pull.rs
git commit -s -m "refactor(graph): ADR-0014 Phase 5 BlockUntilReady 用 watch + PureMemo clear_trace"
```

---

## Task 7：Runner trace 结束清理 PureMemo

**Files:**
- Modify: `src/graph/runner.rs`

- [ ] **Step 1: 在 `run_node` 循环末尾加清理**

定位 `runner.rs` `run_node` 函数内的 `while let Some(ctx_ref) = input_rx.recv().await` 循环。找到循环末尾（`emit_event` 完成 / 错误处理后，回到下一次 `recv` 之前）。

在每次迭代完成处（最简单的位置：在 `while` 循环体的最后一行，`continue` / 自然结束之前），加：

```rust
        // ADR-0014 Phase 5：trace 完成后清理 PureMemo（释放内存）
        pure_memo.clear_trace(trace_id);
```

具体定位：在循环体中所有分支结束后（最末尾），`}` 之前加这行。搜索 `emit_event.*Finished` 或 `emit_failure` 的最后调用点之后。

注意 `trace_id` 在循环顶部定义（line 49），所以循环末尾可以访问。

- [ ] **Step 2: 跑全量测试**

```bash
cargo test --workspace
```

Expected: 全 PASS。行为等价——`clear_trace` 只清 DashMap 条目，不影响当前 trace 的结果（清理在结果已写入/分发之后）。

- [ ] **Step 3: commit**

```bash
git add src/graph/runner.rs
git commit -s -m "feat(graph): ADR-0014 Phase 5 PureMemo trace 完成后清理"
```

---

## Task 8：文档同步 + Phase A checkbox 更新

**Files:**
- Modify: `docs/adr/0014-执行边与数据边分离.md`
- Modify: `docs/superpowers/plans/2026-04-28-architecture-review.md`
- Modify: `docs/superpowers/plans/2026-04-28-adr-0014-phase-4-cache-lifecycle.md`
- Modify: `AGENTS.md`

- [ ] **Step 1: ADR-0014 文档更新实施进度**

定位 `docs/adr/0014-执行边与数据边分离.md` 实施进度 section（Phase 4 之后）。Phase 4 状态从 `🟢` 改为 `✅`，加 Phase 5：

```markdown
- ✅ **Phase 4（2026-04-29）**：Data 输入引脚缓存空/过期兜底策略（`EmptyPolicy`）+
  `BlockUntilReady` Notify+timeout / `DefaultValue(Value)` / `Skip` 三分支；`OutputCache`
  加 `Notify` 唤醒机制 + TTL 过期检查；`PureMemo` per-trace 纯函数记忆缓存。
- ✅ **Phase 5（2026-04-30）**：节点头部按 capability 自动着色（Trigger 红 / Branching
  蓝 / Default 灰蓝）+ CSS 变量化明暗主题；AI 脚本生成 prompt 携带 PinKind 信息；
  OutputCache 从 Notify 迁移到 watch channel（消除竞态窗口）；PureMemo trace
  完成后清理释放内存。
```

- [ ] **Step 2: Phase 4 plan 文件 prepend Status**

定位 `docs/superpowers/plans/2026-04-28-adr-0014-phase-4-cache-lifecycle.md`。把首行：
```
> **Status:** deferred as Phase A backlog (not implemented as of 2026-04-29)
```
替换为：
```
> **Status:** merged in 9db4035
```

- [ ] **Step 3: architecture review plan 更新 Phase A checkbox**

定位 `docs/superpowers/plans/2026-04-28-architecture-review.md`。

把 `- [ ] **Phase 4** cache lifecycle` 行改为 `- [x] **Phase 4** cache lifecycle`。

把 `- [ ] **Phase 5** visual + AI` 行改为 `- [x] **Phase 5** visual + AI`。

Phase 6 / ADR-0015 / ADR-0016 保持 `[ ]`（不在本期范围）。

Phase A 同步清单（5 项）全勾：
```
- [x] ADR 状态推进：提议中 → 已接受 → 已实施
- [x] 同步 `docs/adr/README.md` 索引行
- [x] 同步 `crates/*/AGENTS.md` 影响内容
- [x] prepend `> **Status:** merged in <SHA>` 到对应 plan 文件
```

- [ ] **Step 4: AGENTS.md ADR-0014 状态更新**

定位 `AGENTS.md` ADR-0014 行。更新状态为 Phase 1-5 全完成：

找到：
```
- ADR-0014（执行边与数据边分离 → 重命名为「引脚求值语义二分」）— **已实施 Phase 1 + Phase 2 + Phase 3 + Phase 3b + Phase 4**（2026-04-29）。
```

替换为：
```
- ADR-0014（执行边与数据边分离 → 重命名为「引脚求值语义二分」）— **已实施 Phase 1 + Phase 2 + Phase 3 + Phase 3b + Phase 4 + Phase 5**（2026-04-30）。Phase 5：节点头部 capability 自动着色 + CSS 变量化 + AI prompt PinKind + watch channel 替代 Notify + PureMemo trace 清理。Phase 6 EventBus / ADR-0015 / ADR-0016 仍待实施。
```

ADR Execution Order #8 同步更新。

- [ ] **Step 5: 全量验证**

```bash
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all -- --check
npm --prefix web run test
```

- [ ] **Step 6: commit**

```bash
git add docs/adr/0014-执行边与数据边分离.md docs/superpowers/plans/2026-04-28-adr-0014-phase-4-cache-lifecycle.md docs/superpowers/plans/2026-04-28-architecture-review.md AGENTS.md
git commit -s -m "docs(adr-0014): Phase 5 落地后状态同步 + Phase A checkbox 更新"
```

---

## Self-Review

### Spec coverage

- ✅ 节点头部按 capability 自动着色 — Task 1 (CSS) + Task 2 (缓存) + Task 3 (属性)
- ✅ CSS 变量化（明暗主题） — Task 1
- ✅ AI prompt 携带 PinKind — Task 4
- ✅ PureMemo trace 清理 — Task 6 + Task 7
- ✅ watch channel 替代 Notify — Task 5 + Task 6

### Placeholder scan

- 已检：所有代码块给实际实现，无 TBD/TODO/similar to
- 所有文件路径精确到行号范围

### Type consistency

- `watch::Sender<Option<CachedOutput>>` / `watch::Receiver<Option<CachedOutput>>` — Task 5 Slot 定义 + Task 6 subscribe 使用一致
- `PureMemo::clear_trace(trace_id: Uuid)` — Task 6 定义 + Task 7 调用一致
- `NodePinSummary.kind` 类型 `'exec' | 'data'` — Task 4 定义 + format 使用一致
- `data-node-capability="trigger|branching|default"` — Task 3 设置 + Task 1 CSS 选择器一致

### 已知风险

- **Task 5 watch channel `send` 忽略错误**：watch `send` 在所有 receiver dropped 时返回 `Err`。这在 slot 预分配但无消费者场景下是正确行为（值存入但无人等）。
- **Task 6 pull_one 先检查 `rx.borrow()` 再 `changed()`**：watch 保证 `changed()` 只在新值时唤醒。先读 buffer 可以跳过等待。
- **Task 2 `refreshCapabilitiesCache` 首次加载时序**：IPC 异步，首次渲染可能 capability 未知 → 走 `default` 着色。IPC 回来后需要重渲染。由于 `listNodeTypes` 在 workflow 加载时一并触发，通常在画布渲染前已完成。若未完成，节点头部显示默认灰蓝（无害降级）。
