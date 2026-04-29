# ADR-0014 Phase 5 设计文档：视觉打磨 + AI prompt + PureMemo 清理

> **状态**：待用户审
> **日期**：2026-04-30
> **关联**：ADR-0014（引脚求值语义二分）、`docs/superpowers/specs/2026-04-28-pin-kind-exec-data-design.md` 第九章 Phase 5

---

## 范围

Phase 5 三项增量工作（Phase 3/4 已完成的 Pure 绿头 + pin tooltip 不重做）：

1. **节点头部按 capability 自动着色** + CSS 变量化（明暗主题）
2. **AI 脚本生成 prompt 携带 PinKind 信息**
3. **PureMemo trace 完成后清理** + **watch channel 替代 Notify**

Minimap / 调试视图不在本期。

---

## Block 1：Capability 自动着色 + CSS 变量化

### 现状

- `FlowgramCanvas.tsx:811` 已设 `data-pure-form="true"`，CSS 按此做绿头（`--pin-float`）
- Trigger / Branching / 默认节点未按 ADR-0014 spec 第五章着色
- `node-capabilities.ts` 已定义 `TRIGGER` / `BRANCHING` 等位图，但未用于视觉

### 设计

**数据层**：`FlowgramCanvas.tsx` 渲染节点卡片时，从 `NodeTypeEntry.capabilities`（IPC 已透传）取 bitmask，推导 primary capability：

```
优先级：pure-form > trigger > branching > default
```

- `pure-form`（无 Exec 引脚）→ 已做，不动
- `trigger`（`capabilities & TRIGGER`）→ 红 `#B00000`
- `branching`（`capabilities & BRANCHING`）→ 蓝 `#1FB7FF`
- 其他 → 默认灰蓝 `#3A4F66`

**属性层**：节点卡片 `<div>` 加 `data-node-capability="trigger|branching|default"` 属性（`pure-form` 保持现有 `data-pure-form`）。

**CSS 变量**：`foundation.css` 加：

```css
:root {
  --node-header-pure: #2FB75F;      /* 已有，通过 --pin-float 引用 */
  --node-header-trigger: #B00000;
  --node-header-branching: #1FB7FF;
  --node-header-default: #3A4F66;
}
html[data-theme='dark'] {
  --node-header-pure: #3AC76F;
  --node-header-trigger: #D03030;
  --node-header-branching: #4FC7FF;
  --node-header-default: #4A5F76;
}
```

**flowgram.css** 按属性选择器着色：

```css
.flowgram-card[data-node-capability="trigger"] .flowgram-card__topline {
  background: linear-gradient(180deg, var(--node-header-trigger) 0%, color-mix(in srgb, var(--node-header-trigger) 80%, black) 100%);
}
.flowgram-card[data-node-capability="branching"] .flowgram-card__topline {
  background: linear-gradient(180deg, var(--node-header-branching) 0%, color-mix(in srgb, var(--node-header-branching) 80%, black) 100%);
}
.flowgram-card[data-node-capability="default"] .flowgram-card__topline {
  background: linear-gradient(180deg, var(--node-header-default) 0%, color-mix(in srgb, var(--node-header-default) 80%, black) 100%);
}
```

**着色逻辑位置**：`FlowgramCanvas.tsx` 的节点卡片渲染函数内，紧邻 `pureForm` 计算。从现有 `nodeTypeData`（IPC `list_node_types` 返回的 `NodeTypeEntry`）取 `capabilities` 字段。

**不变**：
- Pure 绿头优先级最高，已做不动
- 现有 `data-pure-form` 属性保留
- 不改节点 body / 端口渲染

### 测试

- Vitest：`FlowgramCanvas` 渲染测试断言 `data-node-capability` 属性值
- 手动验证：dev server + 画布拖入 timer（trigger 红）/ if（branching 蓝）/ httpClient（default 灰蓝）/ c2f（pure 绿）

---

## Block 2：AI prompt 携带 PinKind

### 现状

- `web/src/lib/script-generation.ts:82-104` 系统 prompt 已含 PinType 语义解释
- `buildScriptGenerationPrompt` 已列 pin schema（id, type, direction, required），缺 PinKind

### 设计

**系统 prompt 追加**（`SYSTEM_PROMPT` 常量加一行）：

```
- Pin 求值语义：'exec' 引脚是控制流（被推/推下游）；'data' 引脚是数据流（按需拉取最新值）。Data 输入引脚在脚本运行时已被 Runner 自动从上游 OutputCache 拉取并合并到 payload 里，脚本无需显式处理拉取逻辑。
```

**pin 描述格式扩展**：当前格式 `in (json, required)` → 改为 `in (exec, json, required)` / `value (data, float, required)`。

即每个 pin 描述从 `(pinType, required)` 扩展为 `(pinKind, pinType, required)`。PinKind 在 `PinDefinition` ts-rs 类型里已存在（`kind` 字段，默认 `'exec'`）。

**Rust 端无需改动** — prompt 组装全在前端 `script-generation.ts`，数据源是 `describe_node_pins` IPC 已返回含 `kind` 的 `PinDefinition`。

### 测试

- Vitest：`script-generation.test.ts` 断言 prompt 文本含 PinKind 信息
- 手动：copilot 生成脚本，观察 prompt 里 pin 描述格式

---

## Block 3：PureMemo 清理 + watch 替代 Notify

### 现状

- `src/graph/pull.rs:54-81` `PureMemo` 用 `DashMap<(String, Uuid, u64), Value>` 无清理
- `crates/core/src/cache.rs` `OutputCache` slot 用 `ArcSwap<Option<CachedOutput>>` + `Arc<Notify>` 做一次性唤醒
- Runner 循环（`run_node`）无 trace 结束清理 hook

### 3a：PureMemo trace 完成后清理

**问题**：per-trace `DashMap` 随 trace 累积不释放。虽然 key 含 `trace_id`，不同 trace 的旧条目不会被命中（key 不匹配），但内存不回收。

**设计**：

- `src/graph/pull.rs` `PureMemo` 加 `clear_trace(trace_id: Uuid)` 方法：遍历 `DashMap`，移除 key.2 == trace_id 的条目
- `src/graph/runner.rs` `run_node` 循环末尾（Exec 节点 transform 完成后），调 `pure_memo.clear_trace(trace_id)`
- 清理时机：每次 Exec 节点完成一个 trace 后。由于同一 trace 可能还在其他 Exec 节点执行中，清理只移除**该 trace 的 memo**，不影响其他 trace

**边界**：纯 pure-form 节点不触发清理（它们不 spawn）。清理由参与 Exec 链的节点负责。同一 trace 多个 Exec 节点可能重复清理同一 trace — 幂等（DashMap remove 不存在 key 是 no-op）。

### 3b：watch channel 替代 Notify

**问题**：`Notify` 是一次性信号。`BlockUntilReady` 消费者等 `notified()` → 上游 `notify_waiters()` → 消费者被唤醒后再 `cache.read()`。如果消费者注册晚于 notify，会错过信号永远等。

**`watch::Sender/Receiver`** 优势：
- 新消费者 `Receiver::changed()` 立刻能感知最新值
- 值传递 + 通知合一，不需要"notify + read"两步
- 天然支持多消费者（clone Receiver）

**设计**：

`crates/core/src/cache.rs` Slot 重构：

```rust
struct Slot {
    tx: watch::Sender<Option<CachedOutput>>,
    rx: watch::Receiver<Option<CachedOutput>>,
}

impl OutputCache {
    pub fn prepare_slot(&self, pin_id: &str) {
        let (tx, rx) = watch::channel(None);
        self.slots.insert(pin_id.to_owned(), Arc::new(Slot { tx, rx }));
    }

    pub fn write(&self, pin_id: &str, output: CachedOutput) {
        if let Some(slot) = self.slots.get(pin_id) {
            let _ = slot.tx.send(Some(output));
        }
    }

    /// 读最新快照（不等）
    pub fn read(&self, pin_id: &str, ttl_ms: Option<u64>) -> Option<CachedOutput> {
        let slot = self.slots.get(pin_id)?;
        let cached = slot.rx.borrow().clone()?;
        check_ttl(&cached, ttl_ms)
    }

    /// 拿 Receiver clone（用于 BlockUntilReady 等待）
    pub fn subscribe(&self, pin_id: &str) -> Option<watch::Receiver<Option<CachedOutput>>> {
        self.slots.get(pin_id).map(|slot| slot.rx.clone())
    }
}
```

`src/graph/pull.rs` `BlockUntilReady` 分支重写：

```rust
EmptyPolicy::BlockUntilReady => {
    let mut rx = cache.subscribe(upstream_output_pin_id).ok_or(...)?;
    // 先检查当前值
    if let Some(v) = check_and_read(&rx, ttl_ms) {
        return Ok(v);
    }
    // 等 change
    let timeout_ms = block_timeout_ms.unwrap_or(DEFAULT_BLOCK_TIMEOUT_MS);
    match tokio::time::timeout(
        Duration::from_millis(timeout_ms),
        rx.changed(),
    ).await {
        Ok(Ok(())) => check_and_read(&rx, ttl_ms)
            .ok_or(EngineError::DataPinPullTimeout { ... }),
        _ => Err(EngineError::DataPinPullTimeout { ... }),
    }
}
```

**迁移注意**：
- `write` 不再返回 `notify_waiters()`，改 `tx.send()` — watch send 是有值语义
- `slot_ids()` 等 API 不变
- 所有 `cache.read(pin, None)` 调用点签名不变（`ttl_ms` 参数保留）
- `notify_handle()` 删 → 替换为 `subscribe()`

### 测试

- Rust 单测：`cache.rs` — `watch::Sender` 写后 `Receiver::changed()` 唤醒 + TTL 过期
- Rust 单测：`pull.rs` — `PureMemo::clear_trace` 只清目标 trace
- 集成测试：Phase 3/4 已有测试全部仍通过（行为等价替换）

---

## 不在范围

- Minimap / 调试视图 PinKind 形状
- 色盲友好替代配色（spec 第十一章决策留后）
- 节点头部圆角胶囊 vs 圆角矩形的具体 CSS 微调（Phase 3 已做圆角胶囊）
- 反应式引脚（ADR-0015）

---

## 文件影响清单

| 操作 | 文件 | 责任 |
|------|------|------|
| 修改 | `web/src/components/FlowgramCanvas.tsx` | 节点卡片加 `data-node-capability` 属性 |
| 修改 | `web/src/styles/foundation.css` | 加 `--node-header-*` CSS 变量（明暗主题） |
| 修改 | `web/src/styles/flowgram.css` | 按 capability 属性选择器着色 |
| 修改 | `web/src/lib/script-generation.ts` | 系统 prompt 加 PinKind 语义 + pin 描述格式扩展 |
| 修改 | `crates/core/src/cache.rs` | `Slot` 重构为 watch + `subscribe()` API |
| 修改 | `src/graph/pull.rs` | `BlockUntilReady` 用 watch 等待 + `PureMemo::clear_trace` |
| 修改 | `src/graph/runner.rs` | trace 结束调 `pure_memo.clear_trace` |
| 修改 | `docs/adr/0014-执行边与数据边分离.md` | 实施进度加 Phase 5 |
| 修改 | `docs/superpowers/plans/2026-04-28-architecture-review.md` | Phase A checkbox 更新 |
| 修改 | `AGENTS.md` | ADR-0014 状态同步 |
