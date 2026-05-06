> **Status:** merged in 2d01400

# ADR-0012 工作流变量 Phase 2 Implementation Plan

> **Status:** ✅ 全部 commit 已合入 main（2026-04-27）
> 落地点：ADR-0012 → "已实施 Phase 1+2"、`docs/adr/README.md` 索引、`AGENTS.md` Project Status / IPC Surface 均已更新。

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 在 ADR-0012 Phase 1 基础上落地"声明 + 写入 + 观测"三件事的 UI 闭环：`VariableChanged` 事件广播（write-on-change 语义）、IPC `set_workflow_variable` 写命令、前端运行时变量面板（实时显示 + 类型化编辑）。

**Architecture:**
- **Ring 0（`crates/core`）**：`ExecutionEvent` 加 `VariableChanged` 变体（含 RFC3339 时间戳与 updated_by）；`WorkflowVariables` 加 `Option<mpsc::Sender<ExecutionEvent>>` 字段；`set` / `compare_and_swap` 在 `entry.value != new` 时才 emit（**B 方案：write-on-change 语义在事件层 dedup，`set` 行为本身保持向后兼容**）。
- **Facade（`src/graph/`）**：`deploy_workflow_with_ai` 重排——event_tx 创建早于 `build_workflow_variables`，`build_workflow_variables` 加可选 `event_tx` 参数注入到 `WorkflowVariables`。
- **Tauri shell（`src-tauri`）**：现有 `ExecutionEvent` drain 循环加分支——`VariableChanged` 转发到 `Window::emit("workflow://variable-changed", payload)`；新 IPC 命令 `set_workflow_variable` 走 `Arc<WorkflowVariables>::set` 路径。
- **IPC 类型（`crates/tauri-bindings`）**：`SetWorkflowVariableRequest` / `SetWorkflowVariableResponse` / `VariableChangedPayload`。
- **Frontend（`web/`）**：新增 `workflow-variables.ts` lib（IPC wrappers + event 订阅 helper）；新增 `RuntimeVariablesPanel.tsx` 集成进 `RuntimeDock` 作为新 Tab；类型化编辑表单按 `PinType.kind` 渲染对应输入。

**Tech Stack:** Rust 2024、`tokio` mpsc、`chrono::DateTime<Utc>`（已用）、Tauri v2 `Window::emit` / `listen`、React 18 + TypeScript、Vitest。

---

## 背景速览（实施人必读）

本次改动与 Phase 1 共享同一份 ADR；动手前请先看：

- `docs/adr/0012-工作流变量.md`（决策原文 + Phase 1 落地记录的 8 个偏离点）
- `docs/plans/2026-04-27-workflow-variables.md`（Phase 1 plan，已合入 main）
- `CLAUDE.md` → Critical Coding Constraints（`no unwrap / no unsafe / 节点不碰 DataStore / RAII`）
- `crates/core/src/event.rs`（`ExecutionEvent` 已有变体 + 自定义 serde 影子枚举 `ExecutionEventSerde` + `From<&ExecutionEvent>` impl 模式）
- `web/src/lib/tauri.ts`（IPC `invoke` + `listen<T>('workflow://...')` 既有模式；新增订阅函数照样写）
- `web/src/components/app/RuntimeDock.tsx`（侧栏 Tab 容器；本次新增"变量"Tab 集成进去）

## 本计划的范围界线

**包含（Phase 2 最小可用版）：**
1. Ring 0 `ExecutionEvent::VariableChanged` 变体（含 RFC3339 timestamp + updated_by）+ serde 影子 + `From<&ExecutionEvent>` 同步
2. `WorkflowVariables` 加 `event_tx: Option<mpsc::Sender<ExecutionEvent>>` 字段；`set` / `compare_and_swap` 在值变化时 `try_send`
3. `build_workflow_variables` 加 `event_tx: Option<...>` 参数；deploy.rs 重排 event_tx 早于 vars 构造
4. Tauri shell `ExecutionEvent` drain 循环加分支：`VariableChanged` → `Window::emit("workflow://variable-changed", VariableChangedPayload)`
5. IPC `set_workflow_variable(workflow_id, name, value)` + 类型 + ts-rs 导出
6. 前端 `web/src/lib/workflow-variables.ts`：`setWorkflowVariable(...)` / `snapshotWorkflowVariables(...)` IPC wrappers + `onWorkflowVariableChanged(handler)` 订阅
7. 前端 `RuntimeVariablesPanel.tsx`：变量列表（实时刷新）+ 类型化编辑表单
8. `RuntimeDock` 集成：新增"变量"Tab
9. ADR-0012 → Phase 2 落地记录小节、AGENTS.md / memory 同步、Phase 2 plan checkboxes

**不包含（留给 Phase 3+ / 独立 ADR）：**
- 持久化（变量进程退出即清零，ADR-0012 风险章节明确第一版不持久化）
- 历史曲线图、time series 存储
- `compare_and_swap` 之外的并发原语（`fetch_add` 等）
- 跨工作流变量共享 / 全局变量
- 变量重命名 / schema 演进迁移工具
- 子图变量作用域规则（ADR-0013 处理）

---

## File Structure

### 新建

- `web/src/lib/workflow-variables.ts` — IPC wrappers + `onWorkflowVariableChanged` 订阅 helper（仿 `tauri.ts` 既有形态但放独立文件，与 Phase 1 IPC `snapshot_workflow_variables` 共置）。
- `web/src/components/app/RuntimeVariablesPanel.tsx` — 运行时变量面板组件（列表 + 编辑表单）。
- `web/src/lib/__tests__/workflow-variables.test.ts` — Vitest 单元测试（IPC wrapper + 事件处理）。

### 修改

- `crates/core/src/event.rs` — `ExecutionEvent::VariableChanged` 变体 + serde 影子 + From impl。
- `crates/core/src/variables.rs` — `WorkflowVariables` 加 `event_tx` 字段 + `set` / `compare_and_swap` 在值变化时 emit。
- `crates/core/src/lib.rs` — 导出新类型（如有）。
- `src/graph/variables_init.rs` — `build_workflow_variables` 签名加 `event_tx` 参数。
- `src/graph/deploy.rs` — 重排 event_tx 创建时机；调用 build 时传入 event_tx。
- `crates/tauri-bindings/src/lib.rs` — 加 `SetWorkflowVariableRequest/Response` + `VariableChangedPayload` 类型 + `export_all` 注册。
- `src-tauri/src/lib.rs` — `ExecutionEvent` drain 循环加 `VariableChanged` 分支；实现 `set_workflow_variable` 命令并注册。
- `web/src/components/app/RuntimeDock.tsx` — 引入 `RuntimeVariablesPanel` 作为新 Tab。
- `web/src/lib/tauri.ts` — 视情况加导出（如果新订阅函数复用现有 listen helper）。
- `docs/adr/0012-工作流变量.md` — `### Phase 2 落地记录（YYYY-MM-DD）` 小节。
- `docs/adr/README.md` — 索引行更新（`已实施 (Phase 1+2)`）。
- `AGENTS.md` — Project Status + ADR Execution Order 更新。
- `crates/core/AGENTS.md` — `ExecutionEvent::VariableChanged` 注记。
- `crates/scripting/AGENTS.md`（如需）。
- `~/.claude/projects/-home-zhihongniu-Nazh/memory/{project_system_architecture.md, project_architecture_review_2026_04.md, MEMORY.md}` — Phase 2 状态同步。

---

## Task 1: Ring 0 — `ExecutionEvent::VariableChanged` 变体

**Files:**
- Modify: `crates/core/src/event.rs`

- [x] **Step 1: 写测试（先固定事件形状）**

打开 `crates/core/src/event.rs`，在已有 `#[cfg(test)] mod tests`（如有）或文件末尾追加：

```rust
#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod variable_changed_tests {
    use super::*;

    #[test]
    fn variable_changed_往返序列化() {
        let event = ExecutionEvent::VariableChanged {
            workflow_id: "wf-1".to_owned(),
            name: "setpoint".to_owned(),
            value: serde_json::json!(25.5),
            updated_at: "2026-04-27T10:00:00+00:00".to_owned(),
            updated_by: Some("node-A".to_owned()),
        };
        let json = serde_json::to_string(&event).unwrap();
        let restored: ExecutionEvent = serde_json::from_str(&json).unwrap();
        assert_eq!(event, restored);
    }

    #[test]
    fn variable_changed_updated_by_缺省时反序列化为_none() {
        let json = serde_json::json!({
            "VariableChanged": {
                "workflow_id": "wf-1",
                "name": "x",
                "value": 1,
                "updated_at": "2026-04-27T10:00:00+00:00"
            }
        });
        let restored: ExecutionEvent = serde_json::from_value(json).unwrap();
        match restored {
            ExecutionEvent::VariableChanged { updated_by, .. } => {
                assert!(updated_by.is_none(), "updated_by 缺省应为 None");
            }
            other => panic!("expected VariableChanged, got {other:?}"),
        }
    }
}
```

- [x] **Step 2: 运行测试观察失败**

```bash
cargo test -p nazh-core variable_changed_tests 2>&1 | tail -10
```

预期：`ExecutionEvent::VariableChanged` 不存在，编译失败。

- [x] **Step 3: 加变体 + serde 影子 + From impl**

在 `ExecutionEvent` 枚举末尾（`Finished` 之后）加：

```rust
    /// 工作流变量值变更（ADR-0012 Phase 2，write-on-change 语义）。
    ///
    /// 仅当 `set` / `compare_and_swap` 检测到 `entry.value != new` 时 emit；
    /// 写入相同值不触发本事件（避免轮询脚本制造事件刷屏）。
    /// `updated_at` 是 RFC3339 字符串，保持与 [`TypedVariableSnapshot`](crate::TypedVariableSnapshot) 一致；
    /// `updated_by` 是写入方 node_id（IPC 写入时为 `Some("ipc")` / 类似哨兵）。
    VariableChanged {
        workflow_id: String,
        name: String,
        value: serde_json::Value,
        updated_at: String,
        #[cfg_attr(feature = "ts-export", ts(optional))]
        updated_by: Option<String>,
    },
```

> **注意**：`workflow_id` 字段是必填——发出事件时由 Runner 注入，因为 `WorkflowVariables` 自己不知道所属 workflow_id。详见 Task 2 与 Task 3。

`ExecutionEventSerde`（影子枚举）末尾加对应变体：

```rust
    VariableChanged {
        workflow_id: String,
        name: String,
        value: serde_json::Value,
        updated_at: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        updated_by: Option<String>,
    },
```

`From<&ExecutionEvent> for ExecutionEventSerde` 加分支（紧贴 `Finished` 分支之前）：

```rust
            ExecutionEvent::VariableChanged {
                workflow_id,
                name,
                value,
                updated_at,
                updated_by,
            } => Self::VariableChanged {
                workflow_id: workflow_id.clone(),
                name: name.clone(),
                value: value.clone(),
                updated_at: updated_at.clone(),
                updated_by: updated_by.clone(),
            },
```

`From<ExecutionEventSerde> for ExecutionEvent`（如有反向 From，文件可能没有，按现有形态判断；若只用 Serialize 再 from_str 走中间 owned，加 `From<ExecutionEventSerde>`）：

> **read 现有的 `From<&ExecutionEvent> for ExecutionEventSerde` 与 `Deserialize for ExecutionEvent`/`From<ExecutionEventSerde> for ExecutionEvent` 实现**——按它们的模式补对称分支。文件 90-180 行附近会有完整序列化/反序列化路径。

- [x] **Step 4: 测试通过**

```bash
cargo test -p nazh-core variable_changed_tests 2>&1 | tail -10
cargo test -p nazh-core --lib 2>&1 | tail -5
```

预期：2 个新测试 + 全部已有测试通过。

- [x] **Step 5: ts-rs 导出更新**

```bash
cargo test -p tauri-bindings --features ts-export export_bindings
git diff web/src/generated/ExecutionEvent.ts
```

预期：`ExecutionEvent.ts` 多了 `VariableChanged` 变体。

- [x] **Step 6: Commit**

```bash
git add crates/core/src/event.rs web/src/generated/
git commit -s -m "feat(core): ExecutionEvent::VariableChanged 变体（ADR-0012 Phase 2 事件层）"
```

---

## Task 2: `WorkflowVariables` 加 `event_tx` 字段 + change-detection emit

**Files:**
- Modify: `crates/core/src/variables.rs`

- [x] **Step 1: 写"set 写入相同值不发事件 + 写入不同值发事件"的测试**

打开 `crates/core/src/variables.rs::tests` 模块（已有 `#[allow(clippy::unwrap_used)]`），追加：

```rust
    #[tokio::test]
    async fn set_值变化时发_variablechanged_事件() {
        use tokio::sync::mpsc;

        let (tx, mut rx) = mpsc::channel(8);
        let mut decls = HashMap::new();
        decls.insert(
            "x".to_owned(),
            VariableDeclaration {
                variable_type: PinType::Integer,
                initial: Value::from(0_i64),
            },
        );
        let mut vars = WorkflowVariables::from_declarations(&decls).unwrap();
        vars.set_event_sender("wf-1".to_owned(), tx);

        vars.set("x", Value::from(1_i64), Some("node-A")).unwrap();

        let event = rx.recv().await.expect("应收到事件");
        match event {
            crate::ExecutionEvent::VariableChanged {
                workflow_id,
                name,
                value,
                updated_by,
                ..
            } => {
                assert_eq!(workflow_id, "wf-1");
                assert_eq!(name, "x");
                assert_eq!(value, Value::from(1_i64));
                assert_eq!(updated_by.as_deref(), Some("node-A"));
            }
            other => panic!("expected VariableChanged, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn set_值未变化时不发事件() {
        use tokio::sync::mpsc;
        use tokio::time::{timeout, Duration};

        let (tx, mut rx) = mpsc::channel(8);
        let mut decls = HashMap::new();
        decls.insert(
            "x".to_owned(),
            VariableDeclaration {
                variable_type: PinType::Integer,
                initial: Value::from(42_i64),
            },
        );
        let mut vars = WorkflowVariables::from_declarations(&decls).unwrap();
        vars.set_event_sender("wf-1".to_owned(), tx);

        // 写入与初值相同的值
        vars.set("x", Value::from(42_i64), Some("node-A")).unwrap();

        // 等 50ms 确保不会有事件到达
        let result = timeout(Duration::from_millis(50), rx.recv()).await;
        assert!(result.is_err(), "值未变化应不发事件，但收到：{result:?}");
    }

    #[tokio::test]
    async fn cas_成功且值变化时发事件() {
        use tokio::sync::mpsc;

        let (tx, mut rx) = mpsc::channel(8);
        let mut decls = HashMap::new();
        decls.insert(
            "c".to_owned(),
            VariableDeclaration {
                variable_type: PinType::Integer,
                initial: Value::from(0_i64),
            },
        );
        let mut vars = WorkflowVariables::from_declarations(&decls).unwrap();
        vars.set_event_sender("wf-1".to_owned(), tx);

        let ok = vars
            .compare_and_swap("c", &Value::from(0_i64), Value::from(1_i64), None)
            .unwrap();
        assert!(ok);

        let event = rx.recv().await.expect("应收到事件");
        assert!(matches!(
            event,
            crate::ExecutionEvent::VariableChanged { .. }
        ));
    }

    #[tokio::test]
    async fn cas_失败时不发事件() {
        use tokio::sync::mpsc;
        use tokio::time::{timeout, Duration};

        let (tx, mut rx) = mpsc::channel(8);
        let mut decls = HashMap::new();
        decls.insert(
            "c".to_owned(),
            VariableDeclaration {
                variable_type: PinType::Integer,
                initial: Value::from(0_i64),
            },
        );
        let mut vars = WorkflowVariables::from_declarations(&decls).unwrap();
        vars.set_event_sender("wf-1".to_owned(), tx);

        // expected 不匹配，CAS 应返回 false 不写入
        let ok = vars
            .compare_and_swap("c", &Value::from(99_i64), Value::from(1_i64), None)
            .unwrap();
        assert!(!ok);

        let result = timeout(Duration::from_millis(50), rx.recv()).await;
        assert!(result.is_err(), "CAS 失败不应发事件");
    }

    #[tokio::test]
    async fn 未设置_event_sender_时_set_仍然正常工作() {
        let mut decls = HashMap::new();
        decls.insert(
            "x".to_owned(),
            VariableDeclaration {
                variable_type: PinType::Integer,
                initial: Value::from(0_i64),
            },
        );
        let vars = WorkflowVariables::from_declarations(&decls).unwrap();

        // 未调 set_event_sender，set 不应 panic 也不应报错
        vars.set("x", Value::from(7_i64), Some("node-A")).unwrap();
        assert_eq!(vars.get_value("x"), Some(Value::from(7_i64)));
    }
```

- [x] **Step 2: 运行测试观察失败**

```bash
cargo test -p nazh-core variables 2>&1 | tail -20
```

预期：`set_event_sender` 方法不存在，编译失败。

- [x] **Step 3: 加 `event_tx` 字段 + setter + emit 逻辑**

在 `WorkflowVariables` 结构体加字段（注意：`DashMap` 后端意味着结构体本身可以是 `Send + Sync` 不需 `&mut self` 写入）：

```rust
pub struct WorkflowVariables {
    inner: DashMap<String, TypedVariable>,
    /// ADR-0012 Phase 2：事件发送通道。`None` 表示未注入（测试 / IPC 单点写入场景）；
    /// `Some` 时 `set` / `compare_and_swap` 在值真正变化时 try_send 一条 `VariableChanged`。
    /// 用 `arc_swap::ArcSwapOption` 让 `Arc<WorkflowVariables>` 共享后仍可设置一次。
    event_sink: arc_swap::ArcSwapOption<EventSink>,
}

struct EventSink {
    workflow_id: String,
    sender: tokio::sync::mpsc::Sender<crate::ExecutionEvent>,
}
```

> **关于 `arc_swap`**：`Arc<WorkflowVariables>` 在 deploy 中被 clone 到多处（`SharedResources` + `NodeLifecycleContext` + `DesktopWorkflow`）。Phase 1 完成后用 `Arc::new(...)` 不可变。Phase 2 要"构造时缺 event_tx，注入后才有"——`ArcSwapOption` 是最干净的"原子可选写一次"原语。
>
> `arc_swap` 是否已是 workspace dep？grep `Cargo.toml` 看是否有引用——**如果没有**，`arc_swap = "1"` 加进 `[workspace.dependencies]` 与 `crates/core/Cargo.toml`。
>
> **若不想引入新 dep**，alternative：`tokio::sync::OnceCell<EventSink>`（已是 workspace dep）。`OnceCell` 的 `set(...)` 只能成功调用一次，符合"注入唯一 sink"语义。本 plan 默认走 `OnceCell`。

修改字段为：

```rust
use tokio::sync::OnceCell;

pub struct WorkflowVariables {
    inner: DashMap<String, TypedVariable>,
    /// ADR-0012 Phase 2：事件发送通道（注入一次）。未注入时 `set` 等仍正常工作但不发事件。
    event_sink: OnceCell<EventSink>,
}

struct EventSink {
    workflow_id: String,
    sender: tokio::sync::mpsc::Sender<crate::ExecutionEvent>,
}
```

构造器（`from_declarations` 与 `empty`）初始化 `event_sink: OnceCell::new()`。

加 setter（注意：`OnceCell::set` 接受 `&self`，所以不需要 `&mut self`——可以在 `Arc::new(vars)` 之后调用）：

```rust
impl WorkflowVariables {
    /// 注入事件通道。仅可调用一次；重复调用返回 `Err(())`，调用方按 invariant 处理。
    ///
    /// 设计为 `&self`（非 `&mut`）以便在 `Arc<WorkflowVariables>` 构造完成后注入——
    /// deploy 流程是 `let vars = Arc::new(build_workflow_variables(...)?);` 后再
    /// `vars.set_event_sender(workflow_id, event_tx)`。
    pub fn set_event_sender(
        &self,
        workflow_id: String,
        sender: tokio::sync::mpsc::Sender<crate::ExecutionEvent>,
    ) {
        // OnceCell::set 失败仅意味着重复注入——不视为致命错误，记日志即返回。
        if self
            .event_sink
            .set(EventSink {
                workflow_id,
                sender,
            })
            .is_err()
        {
            tracing::warn!("WorkflowVariables event_sink 重复注入，已忽略");
        }
    }
}
```

> **测试代码改 `vars.set_event_sender(...)`** —— 测试中 `vars` 是 `WorkflowVariables`（非 `Arc`），方法签名是 `&self` 所以即便 `vars` 不是 `mut` 也可以调；但 Step 1 的测试代码当前是 `let mut vars = ...`，需改为 `let vars = ...`（删 `mut`）才不告警。

修改 `set` 方法（在 `entry.value = value;` 之前 / 之后判断），最终形态：

```rust
pub fn set(
    &self,
    name: &str,
    value: Value,
    updated_by: Option<&str>,
) -> Result<(), EngineError> {
    let mut entry = self
        .inner
        .get_mut(name)
        .ok_or_else(|| EngineError::unknown_variable(name))?;
    if !pin_type_matches_value(&entry.variable_type, &value) {
        return Err(EngineError::variable_type_mismatch(
            name,
            entry.variable_type.to_string(),
            json_value_label(&value),
        ));
    }
    let value_changed = entry.value != value;
    entry.value = value;
    entry.updated_at = Utc::now();
    entry.updated_by = updated_by.map(str::to_owned);

    // 拿到事件需要的快照后释放 entry 借用，避免在 try_send 期间持有 shard 写锁
    let event_payload = if value_changed {
        Some((
            entry.value.clone(),
            entry.updated_at.to_rfc3339(),
            entry.updated_by.clone(),
        ))
    } else {
        None
    };
    drop(entry);

    if let (Some((value, updated_at, updated_by)), Some(sink)) =
        (event_payload, self.event_sink.get())
    {
        let event = crate::ExecutionEvent::VariableChanged {
            workflow_id: sink.workflow_id.clone(),
            name: name.to_owned(),
            value,
            updated_at,
            updated_by,
        };
        // try_send 非阻塞：通道满 / 关闭都不阻塞 set 的快路径，仅记 debug 日志
        if let Err(error) = sink.sender.try_send(event) {
            tracing::debug!(?error, "VariableChanged 事件 try_send 失败（通道满或关闭）");
        }
    }

    Ok(())
}
```

`compare_and_swap` 同模式：在 `entry.value = new;` 之后判断 `value_changed`（CAS 成功一定意味着值变化，因为 expected 必须等于旧值；但保留 `entry.value != new` 守卫以防 `expected == new` 的退化情形）：

```rust
pub fn compare_and_swap(
    &self,
    name: &str,
    expected: &Value,
    new: Value,
    updated_by: Option<&str>,
) -> Result<bool, EngineError> {
    let mut entry = self
        .inner
        .get_mut(name)
        .ok_or_else(|| EngineError::unknown_variable(name))?;
    if !pin_type_matches_value(&entry.variable_type, &new) {
        return Err(EngineError::variable_type_mismatch(
            name,
            entry.variable_type.to_string(),
            json_value_label(&new),
        ));
    }
    if &entry.value != expected {
        return Ok(false);
    }
    let value_changed = entry.value != new;
    entry.value = new;
    entry.updated_at = Utc::now();
    entry.updated_by = updated_by.map(str::to_owned);

    let event_payload = if value_changed {
        Some((
            entry.value.clone(),
            entry.updated_at.to_rfc3339(),
            entry.updated_by.clone(),
        ))
    } else {
        None
    };
    drop(entry);

    if let (Some((value, updated_at, updated_by)), Some(sink)) =
        (event_payload, self.event_sink.get())
    {
        let event = crate::ExecutionEvent::VariableChanged {
            workflow_id: sink.workflow_id.clone(),
            name: name.to_owned(),
            value,
            updated_at,
            updated_by,
        };
        if let Err(error) = sink.sender.try_send(event) {
            tracing::debug!(?error, "VariableChanged 事件 try_send 失败（CAS 路径）");
        }
    }

    Ok(true)
}
```

> **关键不变量**：`drop(entry)` 在 `try_send` 之前——避免在持有 DashMap shard 写锁时调用可能阻塞 / 异步的代码。`try_send` 本身是非阻塞同步，但持锁时间越短越好。

确保 `from_declarations` 与 `empty` 都把 `event_sink: OnceCell::new()` 加进去：

```rust
pub fn empty() -> Self {
    Self {
        inner: DashMap::new(),
        event_sink: OnceCell::new(),
    }
}

pub fn from_declarations<S: BuildHasher>(
    declarations: &HashMap<String, VariableDeclaration, S>,
) -> Result<Self, EngineError> {
    let inner = DashMap::with_capacity(declarations.len());
    // ... 现有循环不变 ...
    Ok(Self {
        inner,
        event_sink: OnceCell::new(),
    })
}
```

- [x] **Step 4: 测试通过**

```bash
cargo test -p nazh-core variables 2>&1 | tail -25
cargo test -p nazh-core --lib 2>&1 | tail -5
```

预期：5 个新测试 + 已有 11 个 variables 测试 + 全部 lifecycle 测试通过。

- [x] **Step 5: clippy + fmt**

```bash
cargo clippy --workspace --all-targets -- -D warnings 2>&1 | tail -5
cargo fmt --all -- --check && echo "fmt OK"
```

- [x] **Step 6: Commit**

```bash
git add crates/core/src/variables.rs
git commit -s -m "feat(core): WorkflowVariables write-on-change 事件层 + OnceCell event_sink（ADR-0012 Phase 2）"
```

---

## Task 3: Deploy 重排 + `build_workflow_variables` 注入 event_tx

**Files:**
- Modify: `src/graph/variables_init.rs`
- Modify: `src/graph/deploy.rs`

- [x] **Step 1: 改 `build_workflow_variables` 签名**

打开 `src/graph/variables_init.rs`。当前签名：

```rust
pub fn build_workflow_variables<S: BuildHasher>(
    declarations: Option<&HashMap<String, VariableDeclaration, S>>,
) -> Result<Arc<WorkflowVariables>, EngineError>
```

**保持现有签名不变**——`event_sink` 是构造完后注入，符合 OnceCell 的设计。`build_workflow_variables` 仍然只负责类型校验 + `Arc` 包装。

> 不需要改 build_workflow_variables。Task 2 的 OnceCell 设计就是为此让步骤分离。

- [x] **Step 2: 在 `deploy.rs` 改造**

打开 `src/graph/deploy.rs`，找到现有创建顺序：

```rust
let workflow_variables = build_workflow_variables(graph.variables.as_ref())?;
// ... connection_manager 装配 ...
// ... senders / receivers ...
let event_capacity = graph.nodes.len().max(1) * 16;
let (event_tx, event_rx) = mpsc::channel(event_capacity);
```

**重排**——把 event_tx 创建提到 `workflow_variables` 之后立即（保留 stage 0 早失败 + 紧接着创建事件通道）：

```rust
// ---- 阶段 0：构造工作流变量（早于 connection 装配、Pin 校验）----
let workflow_variables = build_workflow_variables(graph.variables.as_ref())?;

// 现有 connection_manager / runtime / store / senders / receivers 不变 ...

// 事件通道照旧创建
let event_capacity = graph.nodes.len().max(1) * 16;
let (event_tx, event_rx) = mpsc::channel(event_capacity);
let (result_tx, result_rx) = mpsc::channel(event_capacity);

// ADR-0012 Phase 2：把 event_tx 注入 workflow_variables，让 set/CAS 时变更事件
// 通过同一通道流向 Tauri shell + 前端
let workflow_id = graph
    .name
    .clone()
    .unwrap_or_else(|| "anonymous".to_owned());
workflow_variables.set_event_sender(workflow_id, event_tx.clone());
```

> `WorkflowGraph.name` 是 `Option<String>`——`anonymous` fallback 防 None。如果项目对 workflow_id 有更好的来源（例如部署时调用方传入），按实际改。**先 grep `deploy_workflow_with_ai` 调用点**确认 workflow_id 怎么生成的，可能 src-tauri 已有规范。

- [x] **Step 3: 写一个端到端测试（已部署的 vars 触发事件能流到 streams）**

打开 `tests/variables.rs`，追加：

```rust
#[tokio::test]
async fn 部署后写变量触发_variablechanged_事件() {
    use nazh_engine::{
        ExecutionEvent, NodeRegistry, PinType, VariableDeclaration, WorkflowGraph,
        deploy_workflow_with_ai, shared_connection_manager, standard_registry,
    };

    let mut declarations = HashMap::new();
    declarations.insert(
        "setpoint".to_owned(),
        VariableDeclaration {
            variable_type: PinType::Float,
            initial: json!(25.0),
        },
    );
    let graph = WorkflowGraph {
        name: Some("vars-event-test".to_owned()),
        connections: vec![],
        nodes: HashMap::new(),
        edges: vec![],
        variables: Some(declarations),
    };

    let registry: NodeRegistry = standard_registry();
    let cm = shared_connection_manager();
    let mut deployment = deploy_workflow_with_ai(graph, cm, None, &registry)
        .await
        .expect("空 DAG 应能部署");

    // 从 deployment 的 SharedResources 取 vars，写一次新值
    let vars = deployment
        .resources()
        .get::<Arc<nazh_engine::WorkflowVariables>>()
        .expect("应注入 WorkflowVariables");
    vars.set("setpoint", json!(42.0), Some("test")).expect("写入应成功");

    // 从事件流读，期望收到 VariableChanged
    let mut received_change = false;
    for _ in 0..16 {
        match deployment.next_event().await {
            Some(ExecutionEvent::VariableChanged {
                workflow_id,
                name,
                value,
                updated_by,
                ..
            }) => {
                assert_eq!(workflow_id, "vars-event-test");
                assert_eq!(name, "setpoint");
                assert_eq!(value, json!(42.0));
                assert_eq!(updated_by.as_deref(), Some("test"));
                received_change = true;
                break;
            }
            Some(_) => continue,
            None => break,
        }
    }
    assert!(received_change, "未收到 VariableChanged 事件");

    deployment.shutdown().await;
}
```

> **若 `deployment.resources()` 已被 /simplify 删除**——重读 `src/graph/types.rs` 确认；`/simplify` 删过这个方法。需要：在 deploy.rs 把 `Arc::clone(&workflow_variables)` 单独抓出来，但 deployment 内通过 `into_parts()` 后 `.shared_resources` 已是 SharedResources——测试可以从 `deployment.streams()` / `into_parts()` 走拿，或者**先把 `WorkflowDeployment::resources()` 加回去（删 /simplify 的回退是合理的，因为 Phase 2 的测试需要它）**。
>
> **建议路径**：在本 Task commit 中把 `pub fn resources(&self) -> &SharedResources` 加回 `src/graph/types.rs`，并在该方法 doc 写明"为 ADR-0012 Phase 2 集成测试与 IPC 共享访问器"。/simplify 删它的理由是"无 caller"——Phase 2 需要后即不再无 caller。

- [x] **Step 4: 测试通过**

```bash
cargo test --test variables 2>&1 | grep -E "^(test|test result)" | head -10
cargo test --workspace --lib 2>&1 | tail -5
cargo clippy --workspace --all-targets -- -D warnings 2>&1 | tail -3
cargo fmt --all -- --check && echo "fmt OK"
```

预期：4 个集成测试通过（Phase 1 的 3 个 + 本 Task 1 个）。

- [x] **Step 5: Commit**

```bash
git add src/graph/deploy.rs tests/variables.rs src/graph/types.rs
git commit -s -m "feat(graph): deploy 期注入 WorkflowVariables event_sink + 加回 resources() 访问器（ADR-0012 Phase 2）"
```

---

## Task 4: Tauri shell `VariableChanged` 转发 + IPC `set_workflow_variable`

**Files:**
- Modify: `crates/tauri-bindings/src/lib.rs`
- Modify: `src-tauri/src/lib.rs`

- [x] **Step 1: 在 `tauri-bindings` 加 IPC 类型**

打开 `crates/tauri-bindings/src/lib.rs`。在已有 `SnapshotWorkflowVariables*` 类型旁加：

```rust
/// `set_workflow_variable` 命令的请求。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-export", derive(TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct SetWorkflowVariableRequest {
    pub workflow_id: String,
    pub name: String,
    pub value: serde_json::Value,
}

/// `set_workflow_variable` 命令的响应。
///
/// 成功返回写入后的快照（含新 `updated_at` / `updated_by = Some("ipc")`）；
/// 类型不匹配 / 变量未声明等错误通过 `Err(String)` 上抛。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-export", derive(TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct SetWorkflowVariableResponse {
    pub snapshot: TypedVariableSnapshot,
}

/// `workflow://variable-changed` 事件载荷。
///
/// 与 `ExecutionEvent::VariableChanged` 字段一致，但是 ts-rs 导出路径独立——
/// 前端订阅时类型直接就位，不必从 `ExecutionEvent` 联合中分支。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-export", derive(TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct VariableChangedPayload {
    pub workflow_id: String,
    pub name: String,
    pub value: serde_json::Value,
    pub updated_at: String,
    #[cfg_attr(feature = "ts-export", ts(optional))]
    pub updated_by: Option<String>,
}
```

`export_all` 加注册：

```rust
SetWorkflowVariableRequest::export()?;
SetWorkflowVariableResponse::export()?;
VariableChangedPayload::export()?;
```

- [x] **Step 2: 在 `src-tauri/src/lib.rs` 实现 `set_workflow_variable`**

按 `snapshot_workflow_variables` 同模式（块作用域 drop guard 后调 `vars.set`）：

```rust
#[tauri::command]
async fn set_workflow_variable(
    state: State<'_, DesktopState>,
    request: SetWorkflowVariableRequest,
) -> Result<SetWorkflowVariableResponse, String> {
    let vars = {
        let workflows = state.workflows.lock().await;
        let workflow = workflows
            .get(&request.workflow_id)
            .ok_or_else(|| format!("工作流 `{}` 未部署或已撤销", request.workflow_id))?;
        workflow
            .shared_resources
            .get::<std::sync::Arc<nazh_engine::WorkflowVariables>>()
            .ok_or_else(|| {
                tracing::error!(
                    workflow_id = %request.workflow_id,
                    "WorkflowVariables 缺失：deploy_workflow_with_ai 应无条件注入"
                );
                format!(
                    "内部错误：工作流 `{}` 无 WorkflowVariables 资源",
                    request.workflow_id
                )
            })?
    };

    vars.set(&request.name, request.value, Some("ipc"))
        .map_err(|err| err.to_string())?;

    let snapshot = vars
        .get(&request.name)
        .ok_or_else(|| format!("变量 `{}` 写入后未能读回", request.name))?
        .into();

    Ok(SetWorkflowVariableResponse { snapshot })
}
```

注册到 `tauri::generate_handler![...]` 列表。

- [x] **Step 3: 转发 `VariableChanged` 事件**

找到 `ExecutionEvent` drain 循环（多半在 `deploy_workflow` 命令里 spawn 的后台任务，搜 `match event` 或 `node-status` / `workflow://result`）。在已有 match 分支末尾加：

```rust
ExecutionEvent::VariableChanged {
    workflow_id,
    name,
    value,
    updated_at,
    updated_by,
} => {
    let payload = tauri_bindings::VariableChangedPayload {
        workflow_id,
        name,
        value,
        updated_at,
        updated_by,
    };
    if let Err(error) = app.emit("workflow://variable-changed", payload) {
        tracing::warn!(?error, "workflow://variable-changed 事件转发失败");
    }
}
```

> **read 现有 drain 循环结构**——不要硬套，按 match 既有风格写。可能是 `app_handle.emit_all` 或 `app.emit_to(...)`，按现有用法。

- [x] **Step 4: 验证（手测 + 自动测试）**

自动测：

```bash
cargo test -p tauri-bindings --features ts-export export_bindings
cargo check --manifest-path src-tauri/Cargo.toml 2>&1 | tail -5
cargo clippy --workspace --all-targets -- -D warnings 2>&1 | tail -5
cargo fmt --all -- --check
```

预期：3 个新 .ts 文件出现在 `web/src/generated/`。

`web/src/generated/index.ts` 加 barrel：

```typescript
export * from './SetWorkflowVariableRequest';
export * from './SetWorkflowVariableResponse';
export * from './VariableChangedPayload';
```

按现有 barrel 风格（无 `.ts` 后缀 / camelCase 等）调整。

- [x] **Step 5: Commit**

```bash
git add crates/tauri-bindings/src/lib.rs src-tauri/src/lib.rs web/src/generated/
git commit -s -m "feat(ipc): set_workflow_variable 命令 + VariableChanged 转发到 workflow://variable-changed（ADR-0012 Phase 2）"
```

---

## Task 5: Frontend lib `workflow-variables.ts`

**Files:**
- Create: `web/src/lib/workflow-variables.ts`
- Create: `web/src/lib/__tests__/workflow-variables.test.ts`

- [x] **Step 1: 调研 tauri.ts 既有形态**

```bash
sed -n '600,680p' web/src/lib/tauri.ts
```

理解：
- `listen<T>('workflow://...', handler)` → `Promise<UnlistenFn>`
- `invoke<R>('cmd_name', args)` → `Promise<R>`
- 现有 IPC wrapper 函数返回 typed Promise

- [x] **Step 2: 写测试（先固定 IPC wrapper API）**

创建 `web/src/lib/__tests__/workflow-variables.test.ts`：

```typescript
import { describe, expect, it, vi, beforeEach } from 'vitest';

// Mock @tauri-apps/api/core invoke
const invokeMock = vi.fn();
vi.mock('@tauri-apps/api/core', () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));

const listenMock = vi.fn();
vi.mock('@tauri-apps/api/event', () => ({
  listen: (...args: unknown[]) => listenMock(...args),
}));

import {
  setWorkflowVariable,
  snapshotWorkflowVariables,
  onWorkflowVariableChanged,
} from '../workflow-variables';

describe('workflow-variables IPC wrappers', () => {
  beforeEach(() => {
    invokeMock.mockReset();
    listenMock.mockReset();
  });

  it('setWorkflowVariable 通过 invoke 调用 set_workflow_variable', async () => {
    const expected = {
      snapshot: {
        value: 25.0,
        variableType: { kind: 'float' },
        updatedAt: '2026-04-27T10:00:00Z',
        updatedBy: 'ipc',
      },
    };
    invokeMock.mockResolvedValue(expected);
    const result = await setWorkflowVariable({
      workflowId: 'wf-1',
      name: 'setpoint',
      value: 25.0,
    });
    expect(invokeMock).toHaveBeenCalledWith('set_workflow_variable', {
      request: { workflowId: 'wf-1', name: 'setpoint', value: 25.0 },
    });
    expect(result).toEqual(expected);
  });

  it('snapshotWorkflowVariables 通过 invoke 调用 snapshot_workflow_variables', async () => {
    invokeMock.mockResolvedValue({ variables: {} });
    const result = await snapshotWorkflowVariables('wf-1');
    expect(invokeMock).toHaveBeenCalledWith('snapshot_workflow_variables', {
      request: { workflowId: 'wf-1' },
    });
    expect(result).toEqual({ variables: {} });
  });

  it('onWorkflowVariableChanged 注册 listener 并返回 unlisten', async () => {
    const unlisten = vi.fn();
    listenMock.mockResolvedValue(unlisten);
    const handler = vi.fn();
    const result = await onWorkflowVariableChanged(handler);
    expect(listenMock).toHaveBeenCalledWith(
      'workflow://variable-changed',
      expect.any(Function),
    );
    expect(result).toBe(unlisten);
  });

  it('onWorkflowVariableChanged 调用 handler 时透传 payload', async () => {
    let registeredHandler: ((event: { payload: unknown }) => void) | undefined;
    listenMock.mockImplementation((_channel, h) => {
      registeredHandler = h;
      return Promise.resolve(() => {});
    });
    const handler = vi.fn();
    await onWorkflowVariableChanged(handler);
    const payload = {
      workflowId: 'wf-1',
      name: 'x',
      value: 1,
      updatedAt: '2026-04-27T10:00:00Z',
      updatedBy: 'node-A',
    };
    registeredHandler!({ payload });
    expect(handler).toHaveBeenCalledWith(payload);
  });
});
```

- [x] **Step 3: 测试观察失败**

```bash
npm --prefix web run test workflow-variables 2>&1 | tail -20
```

预期：`workflow-variables.ts` 不存在，测试 fail。

- [x] **Step 4: 写实现**

创建 `web/src/lib/workflow-variables.ts`：

```typescript
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import type {
  SetWorkflowVariableRequest,
  SetWorkflowVariableResponse,
  SnapshotWorkflowVariablesResponse,
  VariableChangedPayload,
} from '../generated';

/**
 * 写入工作流变量（ADR-0012 Phase 2）。
 *
 * 类型不匹配 / 变量未声明 / 工作流未部署等错误以 Promise reject 抛出。
 */
export async function setWorkflowVariable(
  request: SetWorkflowVariableRequest,
): Promise<SetWorkflowVariableResponse> {
  return invoke<SetWorkflowVariableResponse>('set_workflow_variable', {
    request,
  });
}

/** 读取工作流变量当前快照（ADR-0012 Phase 1 命令）。 */
export async function snapshotWorkflowVariables(
  workflowId: string,
): Promise<SnapshotWorkflowVariablesResponse> {
  return invoke<SnapshotWorkflowVariablesResponse>(
    'snapshot_workflow_variables',
    { request: { workflowId } },
  );
}

/**
 * 订阅 `workflow://variable-changed` 事件。
 *
 * 返回 unlisten 函数；调用方负责在组件卸载 / hook cleanup 时调用以释放监听器。
 */
export async function onWorkflowVariableChanged(
  handler: (payload: VariableChangedPayload) => void,
): Promise<() => void> {
  return listen<VariableChangedPayload>(
    'workflow://variable-changed',
    (event) => handler(event.payload),
  );
}
```

- [x] **Step 5: 测试通过**

```bash
npm --prefix web run test workflow-variables 2>&1 | tail -20
```

预期：4 个测试通过。

- [x] **Step 6: tsc 类型检查 + 全工作区前端测试不回归**

```bash
npm --prefix web run build 2>&1 | tail -20
npm --prefix web run test 2>&1 | tail -20
```

预期：build 成功（生成 .ts 类型已就位 = Task 4 完成），全工作区 vitest 不回归。

- [x] **Step 7: Commit**

```bash
git add web/src/lib/workflow-variables.ts web/src/lib/__tests__/workflow-variables.test.ts
git commit -s -m "feat(web): workflow-variables.ts IPC wrappers + variable-changed 订阅 helper（ADR-0012 Phase 2）"
```

---

## Task 6: Frontend `RuntimeVariablesPanel` 组件 + RuntimeDock 集成

**Files:**
- Create: `web/src/components/app/RuntimeVariablesPanel.tsx`
- Modify: `web/src/components/app/RuntimeDock.tsx`

- [x] **Step 1: 调研 RuntimeDock 结构**

```bash
wc -l web/src/components/app/RuntimeDock.tsx
grep -n "Tab\|panel\|tabs" web/src/components/app/RuntimeDock.tsx | head -20
```

理解 Tab 结构（可能是 `<Tabs>` / `<TabPanel>` / 自实现的状态切换）。**read 实际形态**——不要硬套下面的代码模板。

- [x] **Step 2: 创建 `RuntimeVariablesPanel.tsx`**

```typescript
import { useEffect, useState, useCallback } from 'react';
import {
  setWorkflowVariable,
  snapshotWorkflowVariables,
  onWorkflowVariableChanged,
} from '../../lib/workflow-variables';
import type {
  TypedVariableSnapshot,
  VariableChangedPayload,
  PinType,
} from '../../generated';

interface RuntimeVariablesPanelProps {
  workflowId: string | null;
}

interface VariableEntry extends TypedVariableSnapshot {
  name: string;
}

export function RuntimeVariablesPanel({ workflowId }: RuntimeVariablesPanelProps) {
  const [variables, setVariables] = useState<Record<string, TypedVariableSnapshot>>({});
  const [error, setError] = useState<string | null>(null);
  const [isLoading, setIsLoading] = useState(false);

  // 初始加载快照
  const refresh = useCallback(async () => {
    if (!workflowId) {
      setVariables({});
      return;
    }
    setIsLoading(true);
    setError(null);
    try {
      const response = await snapshotWorkflowVariables(workflowId);
      setVariables(response.variables ?? {});
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      setIsLoading(false);
    }
  }, [workflowId]);

  useEffect(() => {
    void refresh();
  }, [refresh]);

  // 订阅变更事件
  useEffect(() => {
    if (!workflowId) {
      return;
    }
    let unlisten: (() => void) | undefined;
    let cancelled = false;
    void onWorkflowVariableChanged((payload: VariableChangedPayload) => {
      if (cancelled) return;
      if (payload.workflowId !== workflowId) return;
      setVariables((prev) => ({
        ...prev,
        [payload.name]: {
          value: payload.value,
          variableType: prev[payload.name]?.variableType ?? { kind: 'any' },
          updatedAt: payload.updatedAt,
          updatedBy: payload.updatedBy,
        },
      }));
    }).then((u) => {
      if (cancelled) {
        u();
      } else {
        unlisten = u;
      }
    });
    return () => {
      cancelled = true;
      unlisten?.();
    };
  }, [workflowId]);

  const handleSet = useCallback(
    async (name: string, value: unknown) => {
      if (!workflowId) return;
      try {
        await setWorkflowVariable({ workflowId, name, value });
      } catch (err) {
        setError(err instanceof Error ? err.message : String(err));
      }
    },
    [workflowId],
  );

  const entries: VariableEntry[] = Object.entries(variables).map(([name, snapshot]) => ({
    name,
    ...snapshot,
  }));

  if (!workflowId) {
    return <div className="runtime-variables-panel--empty">未选中已部署的工作流</div>;
  }

  return (
    <div className="runtime-variables-panel">
      {error && <div className="runtime-variables-panel__error">{error}</div>}
      {isLoading && entries.length === 0 ? (
        <div className="runtime-variables-panel--loading">加载中…</div>
      ) : entries.length === 0 ? (
        <div className="runtime-variables-panel--empty">该工作流未声明变量</div>
      ) : (
        <ul className="runtime-variables-panel__list">
          {entries.map((entry) => (
            <VariableRow key={entry.name} entry={entry} onSubmit={handleSet} />
          ))}
        </ul>
      )}
    </div>
  );
}

interface VariableRowProps {
  entry: VariableEntry;
  onSubmit: (name: string, value: unknown) => Promise<void>;
}

function VariableRow({ entry, onSubmit }: VariableRowProps) {
  const [draft, setDraft] = useState<string>(JSON.stringify(entry.value));
  const [isEditing, setIsEditing] = useState(false);
  const [parseError, setParseError] = useState<string | null>(null);

  const handleSubmit = async () => {
    let parsed: unknown;
    try {
      parsed = parseValueByPinType(draft, entry.variableType);
      setParseError(null);
    } catch (err) {
      setParseError(err instanceof Error ? err.message : String(err));
      return;
    }
    await onSubmit(entry.name, parsed);
    setIsEditing(false);
  };

  return (
    <li className="runtime-variables-panel__row">
      <div className="runtime-variables-panel__name">{entry.name}</div>
      <div className="runtime-variables-panel__type">{describePinType(entry.variableType)}</div>
      {!isEditing ? (
        <>
          <div className="runtime-variables-panel__value">{JSON.stringify(entry.value)}</div>
          <button onClick={() => setIsEditing(true)}>编辑</button>
        </>
      ) : (
        <>
          <input
            value={draft}
            onChange={(e) => setDraft(e.currentTarget.value)}
            onBlur={() => void handleSubmit()}
            onKeyDown={(e) => {
              if (e.key === 'Enter') void handleSubmit();
              if (e.key === 'Escape') setIsEditing(false);
            }}
            autoFocus
          />
          {parseError && <span className="runtime-variables-panel__parse-error">{parseError}</span>}
        </>
      )}
      <div className="runtime-variables-panel__meta">
        {entry.updatedBy ?? '-'} · {entry.updatedAt}
      </div>
    </li>
  );
}

function describePinType(pinType: PinType): string {
  switch (pinType.kind) {
    case 'array':
      return `array<${describePinType(pinType.inner)}>`;
    case 'custom':
      return `custom(${pinType.name})`;
    default:
      return pinType.kind;
  }
}

function parseValueByPinType(raw: string, pinType: PinType): unknown {
  const trimmed = raw.trim();
  switch (pinType.kind) {
    case 'bool':
      if (trimmed === 'true') return true;
      if (trimmed === 'false') return false;
      throw new Error('期望 true / false');
    case 'integer': {
      const n = Number(trimmed);
      if (!Number.isInteger(n)) throw new Error('期望整数');
      return n;
    }
    case 'float': {
      const n = Number(trimmed);
      if (Number.isNaN(n)) throw new Error('期望数字');
      return n;
    }
    case 'string':
      // 不强制 JSON 引号——直接当字符串
      return trimmed.startsWith('"') ? JSON.parse(trimmed) : trimmed;
    case 'json':
    case 'array':
    case 'binary':
    case 'any':
    case 'custom':
      // 一律按 JSON 解析；用户负责正确性
      return JSON.parse(trimmed);
  }
}
```

> **样式**：本组件不带 CSS——RuntimeDock 既有样式表会承担布局。如有 `.runtime-variables-panel` 等 CSS 类需要在已有 stylesheet 添加，按 RuntimeDock 同邻位 `.css` / `.module.css` 文件加。

- [x] **Step 3: 集成进 RuntimeDock**

按 Step 1 调研的 Tab 结构加新 Tab。**read 现有 Tab 添加方式**——通常是改一处 `tabs` 数组或直接 JSX 加 `<TabPanel>`。最小入侵：把 `<RuntimeVariablesPanel workflowId={activeWorkflowId} />` 加到 Tab 列表里，标题"变量"。

- [x] **Step 4: 手测**

```bash
cd src-tauri && ../web/node_modules/.bin/tauri dev --no-watch
```

部署一个含 `variables` 字段的工作流，确认：
- 变量列表显示初值
- 编辑后值更新
- 后端 set 操作触发列表实时刷新（用 code 节点定时改值，看是否 ~100ms 内反映在面板）
- 类型不匹配时错误显示

> 自动 e2e 留 Phase 3+——Playwright 测前端面板成本高、Phase 1 / 2 都未做，本 Task 不展开。

- [x] **Step 5: tsc + vitest**

```bash
npm --prefix web run build 2>&1 | tail -10
npm --prefix web run test 2>&1 | tail -10
```

预期：build pass、所有 vitest 不回归。

- [x] **Step 6: Commit**

```bash
git add web/src/components/app/RuntimeVariablesPanel.tsx web/src/components/app/RuntimeDock.tsx
git commit -s -m "feat(web): RuntimeVariablesPanel 运行时变量面板 + RuntimeDock Tab 集成（ADR-0012 Phase 2）"
```

---

## Task 7: 文档与 memory 同步

**Files:**
- Modify: `docs/adr/0012-工作流变量.md`
- Modify: `docs/adr/README.md`
- Modify: `AGENTS.md`
- Modify: `crates/core/AGENTS.md`
- Modify: `~/.claude/projects/-home-zhihongniu-Nazh/memory/{project_system_architecture.md,project_architecture_review_2026_04.md,MEMORY.md}`
- Modify: `docs/plans/2026-04-27-workflow-variables-phase2.md`（本 plan）

- [x] **Step 1: ADR-0012 加 Phase 2 落地记录**

打开 `docs/adr/0012-工作流变量.md`，状态行从 `已实施（Phase 1，2026-04-27）` → `已实施（Phase 1+2，YYYY-MM-DD）`（按当日日期填）。

文末追加 `### Phase 2 落地记录（YYYY-MM-DD）` 小节：

```markdown
### Phase 2 落地记录（YYYY-MM-DD）

**已落地范围：**
- Ring 0：`ExecutionEvent::VariableChanged` 变体（含 RFC3339 时间戳 + updated_by）+ serde 影子同步
- `WorkflowVariables` 加 `OnceCell<EventSink>` 字段；`set` / `compare_and_swap` 在 `entry.value != new` 时 `try_send` 一条 `VariableChanged`
- deploy 期 `workflow_variables.set_event_sender(workflow_id, event_tx.clone())` 注入；workflow_id 取自 `WorkflowGraph.name`（fallback `"anonymous"`）
- Tauri shell 在 `ExecutionEvent` drain 循环加 `VariableChanged` 分支，转发到 `Window::emit("workflow://variable-changed", VariableChangedPayload)`
- IPC `set_workflow_variable(workflow_id, name, value)`：走 `Arc<WorkflowVariables>::set` 路径，`updated_by = Some("ipc")`，类型不匹配通过 `Result<_, String>` 上抛
- 前端 `web/src/lib/workflow-variables.ts`：`setWorkflowVariable` / `snapshotWorkflowVariables` / `onWorkflowVariableChanged` 三个 IPC wrapper + 4 个 vitest 单测
- 前端 `RuntimeVariablesPanel.tsx`：变量列表（实时刷新）+ 类型化编辑表单（按 `PinType.kind` 分派输入解析）；集成进 `RuntimeDock` 作为新 Tab
- 把 Phase 1 /simplify 删掉的 `WorkflowDeployment::resources()` 加回，标注 ADR-0012 Phase 2 集成测试与未来 IPC 共享访问需求

**实施期间的决策偏离 Phase 2 plan 草稿：**
1. `WorkflowVariables.event_sink` 用 `tokio::sync::OnceCell` 而非 plan 草拟的 `arc_swap::ArcSwapOption`——`OnceCell` 已是 workspace dep，避免引入 `arc_swap`。语义稍弱（仅"注入一次"，不能 swap 替换），但符合 ADR-0012 单次部署内 sink 不变的事实。
2. **B 方案落地完整**：`set` 的"写就更新 updated_at"语义保留（对调用方零破坏），事件 emit 在值真正变化时才发生——审计与反应式两种语义并存。
3. workflow_id 来源是 `WorkflowGraph.name` + fallback；如 Phase 3 / ADR-0013 需要更可靠的 workflow_id，单独升级。

**Phase 3 候选项（独立 plan 启动）：**
- 持久化（变量进程退出即清零的限制解除）
- 历史曲线（time series 存储 + 前端图表）
- `set_workflow_variable` 之外的 mutation IPC（如 `reset_workflow_variable`）
- 跨工作流共享变量 / 全局变量
- `Custom` 类型变量解封（依赖 ADR-0010 Phase 4 deferred Item 2 触发）
```

- [x] **Step 2: 更新 `docs/adr/README.md` 索引**

```diff
-| [0012](0012-工作流变量.md) | 工作流级共享变量（`WorkflowVariables`） | 已实施 | 2026-04-27 |
+| [0012](0012-工作流变量.md) | 工作流级共享变量（`WorkflowVariables`） | 已实施（Phase 1+2） | YYYY-MM-DD |
```

- [x] **Step 3: 更新根 `AGENTS.md`**

`## Project Status` 区把 ADR-0012 行更新：

```diff
- ADR-0012 (工作流变量) — **已实施 Phase 1**（2026-04-27，...）
+ ADR-0012 (工作流变量) — **已实施 Phase 1+2**（Phase 1: 2026-04-27 / Phase 2: YYYY-MM-DD，事件广播 + 写 IPC + 前端面板）
```

ADR Execution Order 区第 5 项打钩并替换：

```diff
-> 5. ✅ **ADR-0012** 工作流变量 — Phase 1 已实施（2026-04-27）；Phase 2（前端面板 + 变更事件）独立 plan
+> 5. ✅ **ADR-0012** 工作流变量 — Phase 1+2 已实施
```

`### Tauri IPC Surface` 列表加 `set_workflow_variable`，事件通道列表加 `workflow://variable-changed`，命令计数 +1。

- [x] **Step 4: `crates/core/AGENTS.md`**

`ExecutionEvent` 章节列表加 `VariableChanged` 变体说明：

```markdown
- `VariableChanged { workflow_id, name, value, updated_at, updated_by }` — ADR-0012 Phase 2，write-on-change 语义，仅当 `WorkflowVariables::set` / `compare_and_swap` 检测到 `entry.value != new` 时 emit。
```

`WorkflowVariables` 章节加 `set_event_sender` 方法 + OnceCell 设计注记。

- [x] **Step 5: 更新 memory 三件套**

`project_system_architecture.md`：在 ADR-0012 Phase 1 行下追加：

```markdown
- **ADR-0012 (工作流变量) ✅ Phase 2** (YYYY-MM-DD): `ExecutionEvent::VariableChanged` write-on-change 事件 + IPC `set_workflow_variable` 写命令 + 前端 `RuntimeVariablesPanel` + `workflow://variable-changed` 事件通道。
```

`project_architecture_review_2026_04.md`：提案-05 行：

```diff
-| 提案-05 | 工作流变量 | ADR-0012 | ✅ Phase 1 已实施（2026-04-27）|
+| 提案-05 | 工作流变量 | ADR-0012 | ✅ Phase 1+2 已实施（Phase 2: YYYY-MM-DD）|
```

execution order 区第 5 项相应更新。

`MEMORY.md` 索引行：

```diff
-已实施 ADR-0008/0009/0010/0011/0012(P1)/0017/0018/0019。... 下一候选：ADR-0013 子图与宏（依赖 0010 ✅），或 Phase 6 EventBus + EdgeBackpressure。
+已实施 ADR-0008/0009/0010/0011/0012(P1+P2)/0017/0018/0019。... 下一候选：ADR-0013 子图与宏（依赖 0010 ✅，且 0012 完整后子图变量作用域规则可讨论），或 Phase 6 EventBus + EdgeBackpressure。
```

- [x] **Step 6: Plan 状态头 + checkbox**

打开 `docs/plans/2026-04-27-workflow-variables-phase2.md`（本文件）：

a) 在文件最顶部（# 标题之后）加状态头：

```markdown
> **Status:** ✅ 全部 commit 已合入 main（YYYY-MM-DD）
> 落地点：ADR-0012 → "已实施 Phase 1+2"、`docs/adr/README.md` 索引、`AGENTS.md` Project Status / IPC Surface 均已更新。
```

b) 全部 task 步骤 `- [ ]` → `- [x]`：

```bash
sed -i 's/^- \[ \]/- [x]/g' docs/plans/2026-04-27-workflow-variables-phase2.md
```

- [x] **Step 7: 全面回归 + 文档校对**

```bash
cargo test --workspace 2>&1 | tail -15
cargo clippy --workspace --all-targets -- -D warnings 2>&1 | tail -3
cargo fmt --all -- --check
cargo test -p tauri-bindings --features ts-export export_bindings
npm --prefix web run test 2>&1 | tail -10
npm --prefix web run build 2>&1 | tail -10
```

预期：全部绿灯。

- [x] **Step 8: Commit**

```bash
git add docs/ AGENTS.md crates/core/AGENTS.md
git add /home/zhihongniu/.claude/projects/-home-zhihongniu-Nazh/memory/
git commit -s -m "docs(adr-0012): Phase 2 落地后状态同步 + AGENTS / memory 更新"
```

---

## 自我审查 / Self-Review Checklist

实施完成后跑一遍：

- [x] `cargo test --workspace` 全绿
- [x] `cargo clippy --workspace --all-targets -- -D warnings` 全绿
- [x] `cargo fmt --all -- --check` 无 diff
- [x] `cargo test -p tauri-bindings --features ts-export export_bindings` 通过；`web/src/generated/` 含 3 个新 .ts（SetWorkflowVariableRequest/Response, VariableChangedPayload）
- [x] `web/src/generated/index.ts` 加 barrel export
- [x] `cargo check --manifest-path src-tauri/Cargo.toml` 编译通过
- [x] `npm --prefix web run test` 全绿（含新 `workflow-variables.test.ts` 4 个测试）
- [x] `npm --prefix web run build` 编译通过
- [x] Task 2 的 5 个新 variables 测试 + Task 3 的 1 个端到端测试通过
- [x] 手测：部署含变量的工作流 → 编辑面板触发 set → 看到列表实时更新
- [x] 手测：值未变化时不触发 UI re-render（看 React DevTools / 节流确认）
- [x] ADR-0012 状态字段更新为 "已实施 Phase 1+2"
- [x] `docs/adr/README.md` 索引行更新
- [x] 根 `AGENTS.md` Project Status + ADR Execution Order + IPC Surface + 事件通道列表都更新
- [x] memory 三件套同步

---

## 提交边界与 PR 形态

本 plan 自然分为 **7 个 commit**（每个 Task 一个），单一 PR 推荐组织：

1. `feat(core): ExecutionEvent::VariableChanged 变体（ADR-0012 Phase 2 事件层）`
2. `feat(core): WorkflowVariables write-on-change 事件层 + OnceCell event_sink`
3. `feat(graph): deploy 期注入 WorkflowVariables event_sink + 加回 resources() 访问器`
4. `feat(ipc): set_workflow_variable 命令 + VariableChanged 转发到 workflow://variable-changed`
5. `feat(web): workflow-variables.ts IPC wrappers + variable-changed 订阅 helper`
6. `feat(web): RuntimeVariablesPanel 运行时变量面板 + RuntimeDock Tab 集成`
7. `docs(adr-0012): Phase 2 落地后状态同步`

按"一 PR 多 commit"约定（CLAUDE.md），每个 commit 自包含且测试通过。
