> **Status:** merged in 9a838b1

# ADR-0015 Phase 2+3: 变量 Reactive + IPC + 前端 UI Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 完成 ADR-0015 剩余工作：前端 PinKind 兼容矩阵同步 + Reactive 端口着色 + `subscribe_reactive_pin` IPC 命令 + WorkflowVariables watch channel。

**Architecture:** Phase 3 纯前端（TypeScript 兼容矩阵 + CSS 着色），Phase 2 跨前后端（Rust IPC 命令 + Tauri 事件 + 变量 watch）。前端 `isKindCompatible` 对齐 Rust 三分支矩阵；后端 IPC 持有 OutputCache watch receiver 推送变更到 Tauri 事件 channel。

**Tech Stack:** Rust / tokio watch / Tauri v2 IPC / TypeScript / CSS

**关联 spec:** `docs/specs/2026-04-30-adr-0015-reactive-data-pin-design.md`
**关联 Phase 1 plan:** `docs/plans/2026-04-30-adr-0015-phase1-reactive-edge.md`（已 merged）

---

## 文件变更清单

| 文件 | 动作 | 职责 |
|------|------|------|
| `web/src/lib/pin-compat.ts` | 修改 | isKindCompatible 三分支矩阵 |
| `web/src/lib/pin-compat.ts` | 修改 | isPureForm 覆盖 Reactive |
| `web/src/styles/flowgram.css` | 修改 | Reactive 端口着色 CSS |
| `src-tauri/src/commands/runtime.rs` | 修改 | 新增 subscribe_reactive_pin 命令 |
| `src-tauri/src/lib.rs` | 修改 | 注册新 IPC 命令 |
| `crates/tauri-bindings/src/lib.rs` | 修改 | ReactiveUpdatePayload 类型 |
| `crates/core/src/variables.rs` | 修改 | TypedVariable 加 watch sender |
| `web/src/lib/workflow-events.ts` | 修改 | reactive-update 事件解析 |

---

### Task 1: 前端 isKindCompatible 三分支

**Files:**
- Modify: `web/src/lib/pin-compat.ts:58-60`

当前 `isKindCompatible`（line 58-60）只做严格相等 `from === to`。Phase 1 在 Rust 端已更新为三分支矩阵，前端需要对齐。

- [ ] **Step 1: 更新 isKindCompatible**

```typescript
export function isKindCompatible(from: PinKind, to: PinKind): boolean {
  if (from === to) return true;
  // Reactive 输出可连 Exec / Data 输入（Reactive 是 Exec+Data 超集）
  if (from === 'reactive' && (to === 'exec' || to === 'data')) return true;
  return false;
}
```

- [ ] **Step 2: 更新 isPureForm**

当前 `isPureForm`（line 70-77）检测 "没有 exec pin" 判断纯形式。Reactive pin 有 Exec 行为（推 ContextRef），不应视为纯形式节点。无需改动——`kind !== 'exec'` 对 Reactive 返回 true（即 Reactive 输入输出也算"非 exec"）。确认：`isPureForm` 的语义是"不需要触发链"——Reactive 节点需要触发链（收到 ContextRef），所以应排除。

修改 line 72-73：

```typescript
export function isPureForm(
  inputPins: ReadonlyArray<{ kind?: string }>,
  outputPins: ReadonlyArray<{ kind?: string }>,
): boolean {
  const hasExecIn = inputPins.some(
    (p) => (p.kind ?? 'exec') === 'exec' || (p.kind ?? 'exec') === 'reactive',
  );
  const hasExecOut = outputPins.some(
    (p) => (p.kind ?? 'exec') === 'exec' || (p.kind ?? 'exec') === 'reactive',
  );
  return !hasExecIn && !hasExecOut;
}
```

- [ ] **Step 3: 前端测试**

Run: `npm --prefix web run test -- --run pin-compat`
Expected: PASS（如有既有测试可能需要更新）

- [ ] **Step 4: Commit**

```bash
git add web/src/lib/pin-compat.ts
git commit -s -m "feat(frontend): isKindCompatible 三分支矩阵 + isPureForm 覆盖 Reactive"
```

---

### Task 2: Reactive 端口 CSS 着色

**Files:**
- Modify: `web/src/styles/flowgram.css`（在 Data 着色规则后加 Reactive）

当前 Data pin 着色（~line 1342-1346）：

```css
.flowgram-card__branch-port[data-port-pin-kind='data'] {
  background: var(--surface-elevated);
  border: 2px solid var(--accent-cool, #6366f1);
}
```

- [ ] **Step 1: 添加 Reactive CSS 规则**

在 Data 规则后加：

```css
/* ADR-0015：Reactive 端口——实心填充 + 双色边框（蓝+紫）区分于 Data 的空心 */
.flowgram-card__branch-port[data-port-pin-kind='reactive'] {
  background: linear-gradient(135deg, var(--accent-cool, #6366f1), #a855f7);
  border: 2px solid var(--accent-cool, #6366f1);
}
```

Reactive 端口用渐变填充（蓝紫过渡）区分于 Data 的空心边框和 Exec 的默认样式。

- [ ] **Step 2: 验证样式加载**

Run: `npm --prefix web run build`
Expected: 构建成功，CSS 无错误

- [ ] **Step 3: Commit**

```bash
git add web/src/styles/flowgram.css
git commit -s -m "feat(frontend): Reactive 端口 CSS 着色——蓝紫渐变填充"
```

---

### Task 3: ReactiveUpdatePayload 类型定义 + ts-rs 导出

**Files:**
- Modify: `crates/tauri-bindings/src/lib.rs`

- [ ] **Step 1: 新增 ReactiveUpdatePayload**

在 `VariableChangedPayload`（~line 137-152）之后加：

```rust
/// Reactive 引脚值变更推送载荷（ADR-0015 Phase 2 IPC）。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-export", derive(TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct ReactiveUpdatePayload {
    pub workflow_id: String,
    pub node_id: String,
    pub pin_id: String,
    pub value: serde_json::Value,
    pub updated_at: String,
}
```

- [ ] **Step 2: 重新生成 TypeScript 类型**

Run: `cargo test -p tauri-bindings --features ts-export export_bindings`
Expected: `web/src/generated/ReactiveUpdatePayload.ts` 生成

- [ ] **Step 3: Commit**

```bash
git add crates/tauri-bindings/src/lib.rs web/src/generated/
git commit -s -m "feat(bindings): ReactiveUpdatePayload IPC 类型 + ts-rs 导出"
```

---

### Task 4: subscribe_reactive_pin IPC 命令

**Files:**
- Modify: `src-tauri/src/commands/runtime.rs`
- Modify: `src-tauri/src/lib.rs`

这个命令在前端订阅某个 Reactive 输出引脚的变更。后端持有 OutputCache watch receiver，值变化时通过 Tauri 事件推送到前端。

- [ ] **Step 1: 新增 IPC 命令**

在 `src-tauri/src/commands/runtime.rs` 末尾加：

```rust
/// 订阅指定工作流节点的 Reactive 输出引脚值变更（ADR-0015 Phase 2）。
///
/// 后台启动 watch task：OutputCache slot 值变化时通过
/// `workflow://reactive-update/{workflow_id}/{node_id}/{pin_id}` 推送到前端。
/// task 生命周期随 workflow undeploy 结束（CancellationToken 取消）。
#[tauri::command]
pub(crate) async fn subscribe_reactive_pin(
    state: State<'_, DesktopState>,
    app: tauri::AppHandle,
    workflow_id: String,
    node_id: String,
    pin_id: String,
) -> Result<(), String> {
    let workflows = state.workflows.lock().await;
    let deployment = workflows
        .get(&workflow_id)
        .ok_or_else(|| format!("工作流 `{workflow_id}` 未部署"))?;

    let cache = deployment
        .output_cache_for_node(&node_id)
        .ok_or_else(|| format!("节点 `{node_id}` 无 OutputCache"))?;

    let rx = cache
        .subscribe(&pin_id)
        .ok_or_else(|| format!("引脚 `{pin_id}` 无缓存槽位"))?;

    drop(workflows); // 释放锁

    let event_channel = format!(
        "workflow://reactive-update/{workflow_id}/{node_id}/{pin_id}"
    );

    tokio::spawn(async move {
        let mut rx = rx;
        while rx.changed().await.is_ok() {
            let snapshot = rx.borrow().clone();
            if let Some(cached) = snapshot {
                let payload = tauri_bindings::ReactiveUpdatePayload {
                    workflow_id: workflow_id.clone(),
                    node_id: node_id.clone(),
                    pin_id: pin_id.clone(),
                    value: cached.value,
                    updated_at: cached.produced_at.to_rfc3339(),
                };
                let _ = app.emit(&event_channel, payload);
            }
        }
    });

    Ok(())
}
```

- [ ] **Step 2: 在 DesktopState 暴露 output_cache_for_node**

检查 `src-tauri/src/lib.rs` 中 `WorkflowDeployment` struct 是否已有 `output_cache_for_node` 方法。若没有，需在 `src/graph/` 的 deployment 返回类型上暴露 OutputCache 索引。

查看 `deploy_workflow` 返回的 `WorkflowDeployment` 结构。它应包含 `output_caches: HashMap<String, Arc<OutputCache>>` 字段（Phase 1 deploy.rs 已构建）。

如果 `WorkflowDeployment` 未暴露此字段，需新增 pub method：

```rust
impl WorkflowDeployment {
    pub fn output_cache_for_node(&self, node_id: &str) -> Option<Arc<OutputCache>> {
        // 需要在结构体中存储 output_caches_index
    }
}
```

具体实现取决于 `WorkflowDeployment` 的当前结构。需要检查 `src/graph/deploy.rs` 的返回值和 `src-tauri/` 的 `DesktopState`。

- [ ] **Step 3: 注册 IPC 命令**

在 `src-tauri/src/lib.rs` 的 `invoke_handler` 中加 `subscribe_reactive_pin`：

```rust
.invoke_handler(tauri::generate_handler![
    // ... existing commands ...
    commands::runtime::subscribe_reactive_pin,
])
```

- [ ] **Step 4: 编译检查**

Run: `cargo check --manifest-path src-tauri/Cargo.toml`
Expected: 编译通过

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/commands/runtime.rs src-tauri/src/lib.rs src/graph/
git commit -s -m "feat(ipc): subscribe_reactive_pin 命令 + OutputCache 订阅推送"
```

---

### Task 5: WorkflowVariables watch channel

**Files:**
- Modify: `crates/core/src/variables.rs`

为每个变量加 watch sender，`set()` / `cas()` 写入时同时发送 watch 通知。为未来 Phase 2 变量 Reactive 铺路。

- [ ] **Step 1: TypedVariable 加 watch_tx**

修改 `TypedVariable` struct（line 50-56）：

```rust
pub struct TypedVariable {
    pub value: Value,
    pub variable_type: PinType,
    pub updated_at: DateTime<Utc>,
    pub updated_by: Option<String>,
    /// 变更通知 channel。`set()` / `cas()` 写入时发送 `(timestamp, value)`。
    /// 外部可通过 `subscribe()` 拿 receiver 监听变更。
    watch_tx: watch::Sender<Option<(DateTime<Utc>, Value)>>,
}
```

需要加 `use tokio::sync::watch;` import。

- [ ] **Step 2: 新增 subscribe 方法**

在 `TypedVariable` impl 中加：

```rust
/// 返回当前值的 watch receiver。`changed().await` 在值变更时唤醒。
pub fn subscribe(&self) -> watch::Receiver<Option<(DateTime<Utc>, Value)>> {
    self.watch_tx.subscribe()
}
```

- [ ] **Step 3: 修改 declare 构造 TypedVariable**

找到 `declare()` 或构造 `TypedVariable` 的位置，创建 watch channel：

```rust
let (watch_tx, _) = watch::channel(None);
TypedVariable {
    value,
    variable_type,
    updated_at: Utc::now(),
    updated_by: None,
    watch_tx,
}
```

- [ ] **Step 4: set() 和 cas() 中写 watch sender**

在 `set()` 的 `entry.value = value;` 之后（~line 247），加：

```rust
let _ = entry.watch_tx.send(Some((entry.updated_at, entry.value.clone())));
```

在 `cas()` 的 `entry.value = new;` 之后（~line 297），加相同行。

- [ ] **Step 5: 编译 + 测试**

Run: `cargo test -p nazh-core`
Expected: 全通过

- [ ] **Step 6: Commit**

```bash
git add crates/core/src/variables.rs
git commit -s -m "feat(core): WorkflowVariables watch channel——变量变更通知"
```

---

### Task 6: 前端 reactive-update 事件解析

**Files:**
- Modify: `web/src/lib/workflow-events.ts`

- [ ] **Step 1: 添加 reactive-update 事件处理**

在 `ParsedWorkflowEvent` type 中加 Reactive variant（如果需要集成到统一事件流）。或单独导出 `parseReactiveUpdate` 函数供 dashboard 组件使用。

如果 `workflow-events.ts` 只处理 `ExecutionEvent`（Rust 枚举），则 reactive-update 是独立事件 channel（`workflow://reactive-update/*`），不走 ExecutionEvent 解析。

在文件末尾加：

```typescript
/** Reactive 引脚值变更事件（ADR-0015 Phase 2，独立事件 channel）。 */
export interface ReactiveUpdateEvent {
  workflowId: string;
  nodeId: string;
  pinId: string;
  value: unknown;
  updatedAt: string;
}

/** 从 Tauri 事件 payload 解析 ReactiveUpdate。 */
export function parseReactiveUpdate(payload: unknown): ReactiveUpdateEvent | null {
  if (!payload || typeof payload !== 'object') return null;
  const p = payload as Record<string, unknown>;
  if (
    typeof p.workflowId === 'string' &&
    typeof p.nodeId === 'string' &&
    typeof p.pinId === 'string' &&
    'value' in p &&
    typeof p.updatedAt === 'string'
  ) {
    return {
      workflowId: p.workflowId,
      nodeId: p.nodeId,
      pinId: p.pinId,
      value: p.value,
      updatedAt: p.updatedAt,
    };
  }
  return null;
}
```

- [ ] **Step 2: 前端测试**

Run: `npm --prefix web run test -- --run workflow-events`
Expected: PASS

- [ ] **Step 3: Commit**

```bash
git add web/src/lib/workflow-events.ts
git commit -s -m "feat(frontend): ReactiveUpdate 事件解析 + 类型定义"
```

---

### Task 7: clippy + fmt + 全量验证

- [ ] **Step 1: Rust 格式化检查**

Run: `cargo fmt --all -- --check`

- [ ] **Step 2: clippy**

Run: `cargo clippy --workspace --all-targets -- -D warnings`
Expected: 0 warnings

- [ ] **Step 3: Rust 全量测试**

Run: `cargo test --workspace`
Expected: 全通过

- [ ] **Step 4: 前端测试**

Run: `npm --prefix web run test`
Expected: 全通过

- [ ] **Step 5: 前端构建**

Run: `npm --prefix web run build`
Expected: 构建成功

- [ ] **Step 6: Commit（如有修复）**

```bash
git add -A
git commit -s -m "chore: fmt + clippy 修复"
```

---

### Task 8: 文档同步

**Files:**
- Modify: `docs/plans/2026-04-30-adr-0015-phase1-reactive-edge.md`（已 merged，不改）
- Modify: `docs/specs/2026-04-30-adr-0015-reactive-data-pin-design.md`
- Modify: `docs/plans/2026-04-28-architecture-review.md`
- Modify: `AGENTS.md`

- [ ] **Step 1: architecture review plan ADR-0015 checkbox 更新**

标记 Phase 2/3 完成：

```markdown
- [x] Phase 2 实施（变量 Reactive + IPC）
- [x] Phase 3 实施（前端 UI）
```

- [ ] **Step 2: AGENTS.md ADR-0015 状态更新**

更新状态行反映 Phase 2/3 完成。

- [ ] **Step 3: 本 plan prepend Status**

加 `> **Status:** merged in <SHA>`

- [ ] **Step 4: Commit**

```bash
git add docs/ AGENTS.md
git commit -s -m "docs: ADR-0015 Phase 2/3 完成 + AGENTS.md 状态同步"
```
