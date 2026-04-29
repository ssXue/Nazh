# ADR-0015 反应式数据引脚（ReactivePin）实施设计

> **关联 ADR**: `docs/adr/0015-反应式数据引脚.md`
> **状态**: 设计中
> **日期**: 2026-04-30
> **前置条件**: ADR-0010（Pin 声明系统）✅ / ADR-0014（PinKind Exec/Data）✅ / ADR-0012（工作流变量）✅

## 设计决策

ADR 原文提议 `ReactivePin<T>` 泛型 + `broadcast` channel。经评审修订：

1. **不做泛型**——引擎层所有值都是 `serde_json::Value`，泛型增加复杂度无收益
2. **不用 broadcast**——RFC-0002 Phase 6 已否决（Lagged 丢值违反工业可靠性），改用 `watch` channel
3. **不新建 ReactiveSlot 类型**——复用现有 `OutputCache`（已提供 watch + subscribe），ReactivePin 是 OutputCache + Runner dispatch 行为的组合
4. **PinKind 新增 Reactive variant**——反应式是边行为，不是数据类型

核心思路：**Reactive = Data（写缓存）+ Exec（推 ContextRef）**。

## §1 PinKind 扩展

**文件**: `crates/core/src/pin.rs`

```rust
pub enum PinKind {
    Exec,     // push，每次触发下游 transform
    Data,     // pull，按需读缓存
    Reactive, // watch，值变化时自动唤醒下游
}
```

PinType 不变——数据类型仍由 PinType 表达（Float/Json/...）。PinKind 表达传输语义。

ts-rs 导出 + 前端 pin-compat/pin-schema-cache 同步更新。

兼容矩阵扩展：
- Reactive 输出 → 可连 Reactive / Exec / Data 输入
- Data 输入 → 可从 Reactive 输出拉取（Data 是 Reactive 的子集语义）
- Exec 输出 → 不可连 Reactive 输入（Exec 无缓存值可供 watch）

## §2 Runner dispatch 扩展

**文件**: `src/graph/runner.rs`

当前 dispatch（~lines 132-152）：

```
for target in targets:
  Data     → write OutputCache only
  Exec     → push ContextRef via MPSC
```

扩展为三分支：

```
for target in targets:
  Data     → write OutputCache only
  Exec     → push ContextRef via MPSC
  Reactive → write OutputCache + push ContextRef via MPSC
```

下游 `run_node` loop 不变。收到 ContextRef 后照常 `pull_data_inputs()` 读 Reactive pin 的最新缓存值。

## §3 OutputCache on_change 扩展

**文件**: `crates/core/src/cache.rs`

OutputCache 的 `write_now()` 在写入新值时，对比旧值——值不同时标记 `changed = true`。

Runner 在 dispatch Reactive 目标时检查 `changed`：仅值真正变化时才推 ContextRef。避免上游重复输出相同值时无谓唤醒下游。

实现：`write_now()` 返回 `bool`（是否值变更）。签名变更：

```rust
pub fn write_now(&self, pin_id: &str, output: CachedOutput) -> Result<bool, EngineError>
// Returns true if value changed, false if same value overwritten
```

## §4 WorkflowVariables Reactive 升级

**文件**: `crates/core/src/variables.rs`

当前 `set()` / `cas()` 已有 `try_emit_changed()` 发 `ExecutionEvent::VariableChanged`。

升级：每个变量内部维护一个 `watch::Sender<Option<(DateTime<Utc>, Value)>>`。

```rust
struct TypedVariable {
    pin_type: PinType,
    value: Value,
    updated_at: DateTime<Utc>,
    // 新增
    watch_tx: watch::Sender<Option<(DateTime<Utc>, Value)>>,
}
```

`set()` 流程：
1. 写 DashMap（现有）
2. 写 watch sender（新增：`watch_tx.send(Some((now, value)))`）
3. emit `ExecutionEvent::VariableChanged`（现有）

Runner 部署期：对标记为 Reactive 变量输入的节点，注册 `watch::Receiver`。启动 per-variable watch task：

```rust
tokio::spawn(async move {
    while rx.changed().await.is_ok() {
        let ctx = ContextRef::new(trace_id, data_id);
        if let Err(e) = downstream_tx.send(ctx).await {
            tracing::error!(?e, "Reactive 变量推送失败");
            break;
        }
    }
});
```

watch task 生命周期跟随 `WorkflowDeployment::shutdown`（与 LifecycleGuard 同模式）。

## §5 前端 IPC

**新增 IPC 命令**: `subscribe_reactive_pin(workflow_id, node_id, pin_id)`

返回：通过 Tauri 事件 channel `workflow://reactive-update/{workflow_id}/{node_id}/{pin_id}` 推送变更。

实现：后端持有对应 OutputCache 的 `watch::Receiver`，`rx.changed().await` 时 emit Tauri event。

前端：复用 `useTauriEvent` hook 监听 `workflow://reactive-update/*`，驱动仪表盘组件更新。

## §6 部署期校验

**文件**: `src/graph/pin_validator.rs`

`pin_validator` 扩展兼容矩阵：

| 源 PinKind | 目标 PinKind | 允许 |
|-----------|-------------|------|
| Exec → Exec | ✅ |
| Exec → Data | ✅ |
| Exec → Reactive | ❌（Exec 无缓存） |
| Data → Exec | ✅ |
| Data → Data | ✅ |
| Data → Reactive | ❌（Data 无推送） |
| Reactive → Exec | ✅ |
| Reactive → Data | ✅ |
| Reactive → Reactive | ✅ |

前端 `pin-validator.ts` 同步更新兼容矩阵。

## §7 分阶段实施

### Phase 1：核心 Reactive 边（最小可用）

改动范围：`crates/core/src/pin.rs` + `crates/core/src/cache.rs` + `src/graph/runner.rs` + `src/graph/pull.rs`

- `PinKind::Reactive` 枚举扩展
- `OutputCache::write_now()` 返回 `bool`
- Runner 三分支 dispatch
- 集成测试：两节点 Reactive 连接 + 上游 emit 触发下游 transform + 重复值不触发

### Phase 2：变量 Reactive + IPC

改动范围：`crates/core/src/variables.rs` + `src/graph/runner.rs` + `src-tauri/src/commands/` + `crates/tauri-bindings/`

- `WorkflowVariables` watch channel
- per-variable watch task
- `subscribe_reactive_pin` IPC 命令
- `workflow://reactive-update` 事件 channel
- 集成测试：变量变更触发 Reactive 下游

### Phase 3：前端 UI

改动范围：`web/src/lib/{pin-compat,pin-schema-cache,pin-validator}.ts` + `web/src/lib/flowgram.ts`

- FlowGram 端口着色区分 Reactive（新增颜色 CSS 变量）
- pin-schema-cache Reactive schema
- 兼容矩阵前端同步
- 仪表盘组件订阅 demo（如需要）

## 与 ADR 原文的差异

| 项目 | ADR 原文 | 本设计 | 理由 |
|------|---------|--------|------|
| Channel 类型 | broadcast | watch | RFC-0002 Phase 6 否决 broadcast Lagged 语义 |
| 泛型 ReactivePin\<T\> | 有 | 无 | 引擎层值统一为 Value，泛型增加复杂度 |
| 新类型 ReactiveSlot | 有 | 无 | 复用 OutputCache，Reactive = Data + Exec 行为组合 |
| PinType::Reactive(Box\<PinType\>) | 有 | 无 | 反应式是边行为（PinKind），不是数据类型 |
| EdgeKind 枚举 | 新建 | 不建 | 代码用 PinKind（已落地），EdgeKind 是设计概念 |

ADR 正文不做修改——实施偏差记录在本 spec。解冻后可选择性回写 ADR 备注。
