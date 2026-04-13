# 代码审查修复计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 修复代码审查发现的全部 7 个 Critical 和 12 个高优先 Important 问题，确保仓库质量达到试点可用标准。

**Architecture:** 按 Rust 引擎 → Tauri Shell → 前端 的层级顺序修复，每层内的修改互相独立，可并行执行。

**Tech Stack:** Rust (Tokio, thiserror, Rhai, reqwest, rusqlite)、Tauri v2、React 18 + TypeScript

---

## 第一批：Critical 修复（7 项）

### Task 1: HttpClientNode — 消除 unwrap_or_default，构造函数返回 Result

**Files:**
- Modify: `src/nodes/http_client.rs:203-219`
- Modify: `src/graph/instantiate.rs:149-160`

- [ ] **Step 1: 修改 `HttpClientNode::new` 返回 `Result<Self, EngineError>`**

将 `src/nodes/http_client.rs:203-219` 中的 `pub fn new(...) -> Self` 改为：

```rust
impl HttpClientNode {
    pub fn new(
        id: impl Into<String>,
        config: HttpClientNodeConfig,
        ai_description: impl Into<String>,
    ) -> Result<Self, EngineError> {
        let id = id.into();
        let client = reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::limited(10))
            .build()
            .map_err(|error| {
                EngineError::node_config(
                    id.clone(),
                    format!("HTTP 客户端初始化失败: {error}"),
                )
            })?;
        Ok(Self {
            id,
            ai_description: ai_description.into(),
            config,
            client,
        })
    }
}
```

- [ ] **Step 2: 更新 `instantiate.rs` 中的调用**

将 `src/graph/instantiate.rs:155-160` 中的调用添加 `?`：

```rust
        "httpClient" | "http/client" => {
            let config: HttpClientNodeConfig = parse_config(definition)?;
            let description = resolve_description(
                definition,
                "将 payload 发送到 HTTP 端点（如钉钉机器人告警）",
            );
            Ok(Arc::new(HttpClientNode::new(
                definition.id.clone(),
                config,
                description,
            )?))
        }
```

- [ ] **Step 3: 运行验证**

Run: `cargo test && cargo clippy --all-targets -- -D warnings`
Expected: PASS

---

### Task 2: LoopNode — 添加迭代上限，防止 OOM

**Files:**
- Modify: `src/nodes/loop_node.rs:70-106`

- [ ] **Step 1: 在 `collect_loop_items` 中添加上限**

在 `src/nodes/loop_node.rs` 文件顶部添加常量，然后修改 `collect_loop_items`：

```rust
/// Loop 节点单次执行的最大迭代数量，防止恶意脚本导致 OOM。
const MAX_LOOP_ITERATIONS: usize = 10_000;
```

然后修改整数分支（两处）添加上限检查。将 `return Ok((0..n).map(|_| None).collect());` 前加上：

```rust
        if n > MAX_LOOP_ITERATIONS {
            return Err(EngineError::payload_conversion(
                node_id.to_owned(),
                format!("Loop 迭代次数 {n} 超过上限 {MAX_LOOP_ITERATIONS}"),
            ));
        }
```

同样在 `u64` 分支添加相同检查。在 `Array` 分支添加：

```rust
    if let Some(items) = result.try_cast::<Array>() {
        if items.len() > MAX_LOOP_ITERATIONS {
            return Err(EngineError::payload_conversion(
                node_id.to_owned(),
                format!("Loop 数组长度 {} 超过上限 {MAX_LOOP_ITERATIONS}", items.len()),
            ));
        }
        return items
```

- [ ] **Step 2: 运行验证**

Run: `cargo test && cargo clippy --all-targets -- -D warnings`
Expected: PASS

---

### Task 3: Timer / SerialTrigger — 替换 unwrap_or_default 为显式 Map::new()

**Files:**
- Modify: `src/nodes/timer.rs:58-64`
- Modify: `src/nodes/serial_trigger.rs:157-163`

- [ ] **Step 1: 修改 `timer.rs:58-64`**

将：
```rust
        let existing_timer = payload_map
            .remove("_timer")
            .and_then(|value| match value {
                Value::Object(map) => Some(map),
                _ => None,
            })
            .unwrap_or_default();
```

替换为：
```rust
        let existing_timer = payload_map
            .remove("_timer")
            .and_then(|value| match value {
                Value::Object(map) => Some(map),
                _ => None,
            })
            .unwrap_or_else(Map::new);
```

- [ ] **Step 2: 修改 `serial_trigger.rs:157-163`**

将：
```rust
        let incoming_frame = payload_map
            .remove("_serial_frame")
            .and_then(|value| match value {
                Value::Object(map) => Some(map),
                _ => None,
            })
            .unwrap_or_default();
```

替换为：
```rust
        let incoming_frame = payload_map
            .remove("_serial_frame")
            .and_then(|value| match value {
                Value::Object(map) => Some(map),
                _ => None,
            })
            .unwrap_or_else(Map::new);
```

- [ ] **Step 3: 运行验证**

Run: `cargo test && cargo clippy --all-targets -- -D warnings`
Expected: PASS

---

### Task 4: deploy_workflow TOCTOU 竞态 — 原子化部署操作

**Files:**
- Modify: `src-tauri/src/lib.rs:247-275`

- [ ] **Step 1: 将 deploy_workflow 中的两段锁合并为一段**

将 `src-tauri/src/lib.rs:247-275` 替换为单段锁操作：

```rust
    let mut trigger_tasks;
    {
        let mut workflow_guard = state.workflow.lock().await;
        if let Some(mut existing) = workflow_guard.take() {
            existing.abort_triggers().await;
        }
        trigger_tasks = spawn_timer_root_tasks(ingress.clone(), timer_roots);
        trigger_tasks.extend(spawn_serial_root_tasks(
            app.clone(),
            ingress.clone(),
            state.connection_manager.clone(),
            observability_store.clone(),
            serial_roots,
        ));
        *workflow_guard = Some(DesktopWorkflow {
            ingress,
            trigger_tasks: trigger_tasks.clone(),
        });
    }
```

注意：需要让 `DesktopWorkflow` 中的 `trigger_tasks` 字段可 clone，或者在锁内完成所有操作。如果 `TriggerTask` 的 `JoinHandle` 不 impl `Clone`，则需要在锁内直接构建 `DesktopWorkflow` 而不提前 clone。

方案：将 trigger_tasks 构建 *在锁内* 完成，ingress.clone() 在锁外完成。

- [ ] **Step 2: 将 observability 锁也合并到同一段**

在部署成功后设置 observability 时，用独立 Mutex 保持不变（observability 不参与竞态，因为它不涉及 trigger 任务管理）。此项不需要改。

- [ ] **Step 3: 运行验证**

Run: `cargo check --manifest-path src-tauri/Cargo.toml`
Expected: PASS

---

### Task 5: resolve_project_workspace_dir — 添加路径安全校验

**Files:**
- Modify: `src-tauri/src/lib.rs:712-732`

- [ ] **Step 1: 添加路径校验函数**

在 `resolve_project_workspace_dir` 函数之前添加校验：

```rust
/// 检查工作路径是否指向已知的系统敏感目录。
fn is_safe_workspace_path(path: &Path) -> Result<(), String> {
    let path_str = path.to_string_lossy();
    let forbidden_prefixes = [
        "/etc",
        "/var",
        "/sys",
        "/proc",
        "/dev",
        "/System",
        "/Library",
        "/usr",
        "/bin",
        "/sbin",
        "/private/etc",
        "/private/var",
    ];
    for prefix in &forbidden_prefixes {
        if path_str.starts_with(prefix) {
            return Err(format!(
                "工作路径不允许指向系统目录: {prefix}"
            ));
        }
    }
    Ok(())
}
```

- [ ] **Step 2: 在 `resolve_project_workspace_dir` 中调用校验**

在 `Ok((expanded, false))` 之前添加：

```rust
    is_safe_workspace_path(&expanded)?;
```

- [ ] **Step 3: 运行验证**

Run: `cargo check --manifest-path src-tauri/Cargo.toml`
Expected: PASS

---

### Task 6: 前端 — 修复自适应窗口尺寸清理竞态

**Files:**
- Modify: `web/src/hooks/use-workflow-engine.ts:178-193`

- [ ] **Step 1: 用 alive flag 保护清理**

将 `use-workflow-engine.ts:178-193` 替换为：

```ts
  useEffect(() => {
    if (!hasTauriRuntime()) {
      return;
    }

    let alive = true;
    let cleanup: (() => void) | null = null;

    void enableAdaptiveWindowSizing().then((nextCleanup) => {
      if (alive) {
        cleanup = nextCleanup;
      } else {
        nextCleanup();
      }
    });

    return () => {
      alive = false;
      cleanup?.();
    };
  }, []);
```

- [ ] **Step 2: 运行验证**

Run: `npm --prefix web run build`
Expected: PASS

---

### Task 7: 前端 — 修复倒计时自动恢复的过期闭包

**Files:**
- Modify: `web/src/App.tsx:1003-1020`

- [ ] **Step 1: 用 ref 達免过期闭包**

在 `App.tsx` 中找到倒计时 effect（约 1003 行），替换为：

```ts
  useEffect(() => {
    if (!pendingRestoreSession) {
      return;
    }

    if (restoreCountdown <= 0) {
      void handleConfirmRestore(pendingRestoreSession);
      return;
    }

    const timeoutId = window.setTimeout(() => {
      setRestoreCountdown((current) => current - 1);
    }, 1000);

    return () => {
      window.clearTimeout(timeoutId);
    };
  }, [pendingRestoreSession, restoreCountdown, handleConfirmRestore]);
```

注意：`handleConfirmRestore` 需要用 `useCallback` 包裹或在 deps 数组中声明。如果它依赖很多 state，最安全的做法是用 ref 持有最新的 handler：

在组件顶部（约 handleConfirmRestore 定义之后）添加：
```ts
  const handleConfirmRestoreRef = useRef(handleConfirmRestore);
  handleConfirmRestoreRef.current = handleConfirmRestore;
```

然后倒计时 effect 改为：
```ts
  useEffect(() => {
    if (!pendingRestoreSession) {
      return;
    }

    if (restoreCountdown <= 0) {
      void handleConfirmRestoreRef.current(pendingRestoreSession);
      return;
    }

    const timeoutId = window.setTimeout(() => {
      setRestoreCountdown((current) => current - 1);
    }, 1000);

    return () => {
      window.clearTimeout(timeoutId);
    };
  }, [pendingRestoreSession, restoreCountdown]);
```

- [ ] **Step 2: 运行验证**

Run: `npm --prefix web run build`
Expected: PASS

---

## 第二批：高优先 Important 修复（12 项）

### Task 8: with_connection — panic 安全的连接释放

**Files:**
- Modify: `src/nodes/helpers.rs:55-87`

- [ ] **Step 1: 用 catch_unwind 包裹 operation 调用**

将 `src/nodes/helpers.rs` 中的 `with_connection` 函数替换为：

```rust
pub(crate) async fn with_connection<F>(
    connection_manager: &SharedConnectionManager,
    connection_id: Option<&str>,
    operation: F,
) -> Result<WorkflowContext, EngineError>
where
    F: FnOnce(Option<&ConnectionLease>) -> Result<WorkflowContext, EngineError>,
{
    let lease = if let Some(conn_id) = connection_id {
        Some(connection_manager.borrow(conn_id).await?)
    } else {
        None
    };

    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        operation(lease.as_ref())
    }))
    .map_err(|_| {
        EngineError::payload_conversion(
            connection_id.unwrap_or("unknown").to_owned(),
            "节点操作发生 panic",
        )
    })?;

    let operation_error = result
        .as_ref()
        .err()
        .map(std::string::ToString::to_string);

    if let Some(lease) = lease.as_ref() {
        let release_result = connection_manager
            .release_lease(lease, result.is_ok(), operation_error.as_deref())
            .await;
        if let Err(error) = release_result {
            if result.is_ok() {
                return Err(error);
            }
        }
    }

    result
}
```

注意：需要 `use std::panic::{AssertUnwindSafe, catch_unwind};` 但项目已经在外层 `guarded_execute` 中做了 catch_unwind，此处可能不需要重复。不过为了连接释放安全，加一层保护是合理的。

实际上，因为 `guarded_execute` 已经包裹了整个 `node.execute()`，而 `with_connection` 是在 `execute()` 内部被调用的，所以 panic 会被外层捕获。但 `release_lease` 不会被调用。更简洁的方案是：不在这里加 catch_unwind，而是用 scopeguard 模式保证释放。

- [ ] **Step 2: 运行验证**

Run: `cargo test && cargo clippy --all-targets -- -D warnings`
Expected: PASS

---

### Task 9: SqlWriter — 数据库路径穿越校验

**Files:**
- Modify: `src/nodes/sql_writer.rs` (database_path 校验)

- [ ] **Step 1: 在 `execute` 方法中添加路径校验**

在 `SqlWriterNode::execute()` 中使用 `database_path` 之前添加校验：

```rust
        let database_path = self.config.database_path.trim().to_owned();
        if database_path.contains("..") || database_path.starts_with('/') {
            return Err(EngineError::node_config(
                self.id.clone(),
                "database_path 不允许包含路径穿越（..）或绝对路径",
            ));
        }
```

- [ ] **Step 2: 运行验证**

Run: `cargo test && cargo clippy --all-targets -- -D warnings`
Expected: PASS

---

### Task 10: deploy_workflow — 原子化部署（旧部署不被失败销毁）

**Files:**
- Modify: `src-tauri/src/lib.rs:186-320`

- [ ] **Step 1: 调整部署顺序——先尝试新部署，成功后再替换旧部署**

在 `deploy_workflow` 命令中，将解析和部署图的逻辑移到获取旧工作流之前：

当前顺序：take old → deploy new → insert new
修改为：deploy new → take old → abort old → insert new

具体修改：将 `deploy_workflow_graph` 调用移到 `state.workflow.lock().await.take()` 之前。

这一步已在 Task 4 中部分完成（合并锁）。需要确保如果 `deploy_workflow_graph` 失败，旧 workflow 不受影响。

- [ ] **Step 2: 运行验证**

Run: `cargo check --manifest-path src-tauri/Cargo.toml`
Expected: PASS

---

### Task 11: 事件转发任务管理——存储 JoinHandle 并在重新部署时取消

**Files:**
- Modify: `src-tauri/src/lib.rs` (DesktopWorkflow struct + abort_triggers)

- [ ] **Step 1: 在 `DesktopWorkflow` 中添加 forwarding_tasks 字段**

```rust
struct DesktopWorkflow {
    ingress: nazh_engine::WorkflowIngress,
    trigger_tasks: Vec<TriggerTask>,
    forwarding_tasks: Vec<tauri::async_runtime::JoinHandle<()>>,
}
```

- [ ] **Step 2: 存储 spawn 返回的 JoinHandle**

在 `deploy_workflow` 命令中，将两个 `tauri::async_runtime::spawn(...)` 的返回值收集到 `forwarding_tasks`。

- [ ] **Step 3: 在 `abort_triggers` 中也取消 forwarding tasks**

```rust
async fn abort_triggers(&mut self) -> usize {
    for task in &self.forwarding_tasks {
        task.abort();
    }
    // ... existing trigger abort logic
}
```

- [ ] **Step 4: 运行验证**

Run: `cargo check --manifest-path src-tauri/Cargo.toml`
Expected: PASS

---

### Task 12: IPC 输入大小限制

**Files:**
- Modify: `src-tauri/src/lib.rs`

- [ ] **Step 1: 添加常量和校验**

在文件顶部（`DesktopState` 之前）添加：

```rust
/// IPC 命令输入的最大允许字节数（10 MB）。
const MAX_IPC_INPUT_BYTES: usize = 10 * 1024 * 1024;
```

- [ ] **Step 2: 在 `deploy_workflow` 中添加 AST 大小校验**

在命令函数开头：
```rust
    if ast.len() > MAX_IPC_INPUT_BYTES {
        return Err("AST 超过最大允许大小（10 MB）".to_owned());
    }
```

- [ ] **Step 3: 在 `save_project_library_file` 中添加大小校验**

```rust
    if library_text.len() > MAX_IPC_INPUT_BYTES {
        return Err("工程库文件超过最大允许大小（10 MB）".to_owned());
    }
```

- [ ] **Step 4: 运行验证**

Run: `cargo check --manifest-path src-tauri/Cargo.toml`
Expected: PASS

---

### Task 13: 串口读取器 — 用 std::thread::spawn 替换 spawn_blocking

**Files:**
- Modify: `src-tauri/src/lib.rs` (spawn_serial_root_tasks)

- [ ] **Step 1: 将 `spawn_blocking` 改为 `std::thread::spawn`**

在 `spawn_serial_root_tasks` 函数中，将：
```rust
let join = tauri::async_runtime::spawn_blocking(move || {
    run_serial_root_reader(...);
});
```

替换为：
```rust
let join = std::thread::spawn(move || {
    run_serial_root_reader(...);
});
```

同时更新 `TriggerTask` 的 `join` 字段类型从 `tauri::async_runtime::JoinHandle<()>` 改为 `std::thread::JoinHandle<()>`。

需要调整 `TriggerTask` 结构体和 `abort_triggers` 方法。`std::thread::JoinHandle` 没有 `abort()` 方法，所以取消只能通过 `cancel` flag。`join()` 仍然可用。

```rust
struct TriggerTask {
    cancel: Arc<AtomicBool>,
    join: std::thread::JoinHandle<()>,
}
```

在 `abort_triggers` 中：
```rust
task.cancel.store(true, Ordering::Relaxed);
let _ = task.join.join();
```

注意 `join()` 会阻塞当前线程。如果串口读取器的循环很快退出（cancel flag 被检查），这应该没问题。

- [ ] **Step 2: 运行验证**

Run: `cargo check --manifest-path src-tauri/Cargo.toml`
Expected: PASS

---

### Task 14: observability — active_spans TTL 清理

**Files:**
- Modify: `src-tauri/src/observability.rs:131-133, 173-285`

- [ ] **Step 1: 在 `record_execution_event` 中添加 TTL 清理**

在 `ObservabilityRuntimeState` 的 `record_execution_event` 方法中，每次收到 `Started` 事件时清理超过 1 小时的 span：

```rust
    fn record_execution_event(&mut self, event: &ExecutionEvent, project_id: &str, project_name: &str, environment_id: &str, environment_name: &str) -> ObservabilityEntry {
        let now = Utc::now();

        // 清理超过 1 小时的遗留 span
        self.active_spans.retain(|_, started_at| {
            (now - *started_at).num_seconds() < 3600
        });

        // ... rest of method
```

- [ ] **Step 2: 运行验证**

Run: `cargo check --manifest-path src-tauri/Cargo.toml`
Expected: PASS

---

### Task 15: observability — trace 状态用结构化 event_kind 替代中文匹配

**Files:**
- Modify: `src-tauri/src/observability.rs` (build_trace_summaries + record_execution_event)

- [ ] **Step 1: 在 `ObservabilityEntry` 中添加 `event_kind` 字段**

```rust
pub struct ObservabilityEntry {
    // ... existing fields ...
    pub event_kind: Option<String>,
}
```

- [ ] **Step 2: 在 `record_execution_event` 中填充 `event_kind`**

根据 `ExecutionEvent` 的变体设置 `event_kind`：`"started"`, `"completed"`, `"failed"`, `"output"`, `"finished"`。

- [ ] **Step 3: 在 `build_trace_summaries` 中使用 `event_kind` 替代中文子串匹配**

将 `entry.message.contains("输出")` 改为 `entry.event_kind.as_deref() == Some("output")`，将 `entry.message.contains("完成")` 改为 `entry.event_kind.as_deref() == Some("completed")`。

- [ ] **Step 4: 运行验证**

Run: `cargo check --manifest-path src-tauri/Cargo.toml && cargo test`
Expected: PASS

---

### Task 16: 前端 — workflow-events unsafe type assertion 防御

**Files:**
- Modify: `web/src/lib/workflow-events.ts:106+`

- [ ] **Step 1: 添加类型防御**

在 `parseWorkflowEventPayload` 中，将 `const event = payload as ExecutionEvent;` 替换为防御性检查：

```ts
  if (typeof payload !== 'object' || payload === null) {
    return null;
  }

  const event = payload as Record<string, unknown>;

  if ('Started' in event && isRecord(event.Started)) {
    return {
      type: 'Started' as const,
      stage: String(event.Started.stage ?? ''),
      trace_id: String(event.Started.trace_id ?? ''),
    };
  }
  // ... similar for other variants
```

其中 `isRecord` 是已有的辅助函数。

- [ ] **Step 2: 运行验证**

Run: `npm --prefix web run build && npm --prefix web run test`
Expected: PASS

---

### Task 17: 前端 — legacy 迁移 effect 防止无限循环

**Files:**
- Modify: `web/src/App.tsx:250-279`

- [ ] **Step 1: 用 ref flag 确保迁移只执行一次**

在组件中添加：
```ts
  const migrationDoneRef = useRef(false);
```

在迁移 effect 开头添加：
```ts
  useEffect(() => {
    if (migrationDoneRef.current) return;
    migrationDoneRef.current = true;
    // ... existing migration logic
  }, [/* 保持原有依赖 */]);
```

或者更安全地，将 `connectionLibrary.connections` 从依赖数组中移除（迁移只需要运行一次，不需要响应 connections 变化）。

- [ ] **Step 2: 运行验证**

Run: `npm --prefix web run build`
Expected: PASS

---

### Task 18: 前端 — 硬编码 #ffffff 替换为 CSS 变量

**Files:**
- Modify: `web/src/styles.css:275`
- Modify: `web/src/styles.css:1289`

- [ ] **Step 1: 替换 styles.css:275**

将 `button { color: #ffffff; }` 改为 `button { color: var(--text-primary); }`

- [ ] **Step 2: 替换 styles.css:1289**

将 `.settings-segment__button.is-active { color: #ffffff; }` 改为 `.settings-segment__button.is-active { color: var(--nav-active-text, #ffffff); }`

- [ ] **Step 3: 运行验证**

Run: `npm --prefix web run build`
Expected: PASS

---

### Task 19: 汇总验证 + commit

- [ ] **Step 1: 全量 Rust 测试**

Run: `cargo test && cargo clippy --all-targets -- -D warnings && cargo fmt --all -- --check`
Expected: PASS

- [ ] **Step 2: 全量前端验证**

Run: `npm --prefix web run build && npm --prefix web run test`
Expected: PASS

- [ ] **Step 3: Tauri shell 编译检查**

Run: `cargo check --manifest-path src-tauri/Cargo.toml`
Expected: PASS

- [ ] **Step 4: Commit**

```bash
git add -A
git commit -s -m "fix: 修复代码审查发现的 7 个 Critical 和 12 个 Important 问题

Critical:
- HttpClientNode 构造函数返回 Result，消除 unwrap_or_default
- LoopNode 添加 MAX_LOOP_ITERATIONS 上限，防止 OOM
- Timer/SerialTrigger 替换 unwrap_or_default 为 unwrap_or_else
- deploy_workflow 合并锁操作，消除 TOCTOU 竞态
- resolve_project_workspace_dir 添加系统目录路径校验
- 自适应窗口尺寸清理竞态修复
- 倒计时自动恢复过期闭包修复

Important:
- with_connection 添加 panic 安全的连接释放
- SqlWriter database_path 路径穿越校验
- deploy_workflow 原子化部署顺序调整
- 事件转发任务存储 JoinHandle 并在重部署时取消
- IPC 命令添加输入大小限制
- 串口读取器改用 std::thread::spawn 替代 spawn_blocking
- active_spans 添加 TTL 清理
- trace 状态使用结构化 event_kind 替代中文匹配
- workflow-events 添加类型防御
- legacy 迁移 effect 防止无限循环
- 硬编码 #ffffff 替换为 CSS 变量"
```
