# Nazh Engine src/ 架构重构计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 消除重复代码、提取共享抽象、统一惯用写法，将 ~3200 行引擎代码精简 ~180 行并提升可维护性。

**Architecture:** 提取执行守卫（panic/timeout 隔离）为独立模块；用宏消除 NodeTrait 样板；抽取模板渲染为可复用模块；用 `json!()` 替代手动 Map 构造；清理工厂和拓扑模块中的冗余。

**Tech Stack:** Rust 2021, Tokio, serde_json, futures-util, rhai, reqwest, rusqlite

---

## File Structure

### New Files
| File | Responsibility |
|------|------|
| `src/guard.rs` | 异步执行守卫：统一的 panic 隔离 + 超时保护 |
| `src/nodes/template.rs` | `{{placeholder}}` 模板渲染引擎，从 http_client 抽取 |

### Modified Files
| File | Changes |
|------|---------|
| `src/lib.rs` | 增加 `mod guard` |
| `src/nodes/mod.rs` | 增加 `delegate_node_base!` / `impl_node_meta!` 宏，增加 `pub(crate) mod template` |
| `src/graph/runner.rs` | 用 `guarded_execute` 替代内联 panic/timeout 逻辑 |
| `src/pipeline/runner.rs` | 同上 |
| `src/nodes/rhai.rs` | 应用 `delegate_node_base!` 宏 |
| `src/nodes/if_node.rs` | 同上 |
| `src/nodes/switch_node.rs` | 同上 |
| `src/nodes/try_catch.rs` | 同上 |
| `src/nodes/loop_node.rs` | 同上 |
| `src/nodes/native.rs` | 应用 `impl_node_meta!` 宏 |
| `src/nodes/timer.rs` | 应用宏 + `json!()` 替代 Map 构造 |
| `src/nodes/modbus_read.rs` | 同上 |
| `src/nodes/debug_console.rs` | 同上 |
| `src/nodes/sql_writer.rs` | 同上 |
| `src/nodes/http_client.rs` | 应用宏 + `json!()` + 移除模板函数改用 template 模块 |
| `src/graph/instantiate.rs` | 提取 `resolve_description` 辅助 |
| `src/graph/topology.rs` | 消除不必要的 HashMap clone |

---

### Task 1: 提取执行守卫 `guard.rs`

**Files:**
- Create: `src/guard.rs`
- Modify: `src/lib.rs`
- Modify: `src/graph/runner.rs`
- Modify: `src/pipeline/runner.rs`

- [ ] **Step 1: 创建 `src/guard.rs`**

```rust
//! 异步执行守卫：统一的 panic 隔离与超时保护。
//!
//! DAG 节点运行循环和线性流水线阶段均通过 [`guarded_execute`] 执行，
//! 保证单个任务的 panic 或超时不会导致整个运行时崩溃。

use std::{future::Future, panic::AssertUnwindSafe, time::Duration};

use futures_util::FutureExt;
use uuid::Uuid;

use crate::EngineError;

/// 在 panic 隔离和可选超时保护下执行异步任务。
///
/// - 通过 [`AssertUnwindSafe`] + [`catch_unwind`](FutureExt::catch_unwind) 捕获 panic
/// - 可选的 [`tokio::time::timeout`] 保护
/// - panic 转换为 [`EngineError::StagePanicked`]
/// - 超时转换为 [`EngineError::StageTimeout`]
pub(crate) async fn guarded_execute<T, Fut>(
    stage: &str,
    trace_id: Uuid,
    timeout: Option<Duration>,
    fut: Fut,
) -> Result<T, EngineError>
where
    Fut: Future<Output = Result<T, EngineError>> + Send,
{
    let guarded = AssertUnwindSafe(fut).catch_unwind();

    if let Some(duration) = timeout {
        match tokio::time::timeout(duration, guarded).await {
            Ok(Ok(result)) => result,
            Ok(Err(_)) => Err(EngineError::StagePanicked {
                stage: stage.to_owned(),
                trace_id,
            }),
            Err(_) => Err(EngineError::StageTimeout {
                stage: stage.to_owned(),
                trace_id,
                timeout_ms: duration.as_millis(),
            }),
        }
    } else {
        guarded.await.unwrap_or_else(|_| {
            Err(EngineError::StagePanicked {
                stage: stage.to_owned(),
                trace_id,
            })
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn 正常执行返回结果() {
        let trace_id = Uuid::new_v4();
        let result: Result<i32, EngineError> =
            guarded_execute("test", trace_id, None, async { Ok(42) }).await;
        assert!(matches!(result, Ok(42)));
    }

    #[tokio::test]
    async fn 内部错误正常传播() {
        let trace_id = Uuid::new_v4();
        let result: Result<i32, EngineError> = guarded_execute("test", trace_id, None, async {
            Err(EngineError::invalid_graph("测试错误"))
        })
        .await;
        assert!(matches!(
            result,
            Err(EngineError::InvalidGraph(ref msg)) if msg.contains("测试错误")
        ));
    }

    #[tokio::test]
    async fn panic_被捕获转为阶段异常() {
        let trace_id = Uuid::new_v4();
        let result: Result<i32, EngineError> =
            guarded_execute("panicky", trace_id, None, async { panic!("boom") }).await;
        assert!(matches!(
            result,
            Err(EngineError::StagePanicked { ref stage, .. }) if stage == "panicky"
        ));
    }

    #[tokio::test]
    async fn 超时返回阶段超时错误() {
        let trace_id = Uuid::new_v4();
        let timeout = Some(Duration::from_millis(10));
        let result: Result<i32, EngineError> = guarded_execute("slow", trace_id, timeout, async {
            tokio::time::sleep(Duration::from_secs(10)).await;
            Ok(0)
        })
        .await;
        assert!(matches!(
            result,
            Err(EngineError::StageTimeout { ref stage, .. }) if stage == "slow"
        ));
    }
}
```

- [ ] **Step 2: 在 `src/lib.rs` 中注册模块**

在 `pub mod connection;` 之前添加一行：

```rust
mod guard;
```

注意：不需要 `pub`，这是 crate 内部模块。

- [ ] **Step 3: 运行 guard 单元测试**

Run: `cargo test --lib guard`
Expected: 4 tests pass

- [ ] **Step 4: 重写 `src/graph/runner.rs` 使用 `guarded_execute`**

完整替换后的文件：

```rust
//! 单节点异步执行循环与事件发射。
//!
//! [`run_node`] 在独立的 Tokio 任务中运行，持续从输入通道接收上下文，
//! 执行节点逻辑，并根据 [`NodeDispatch`] 将输出分发到下游或结果流。

use std::{sync::Arc, time::Duration};

use tokio::sync::mpsc;
use uuid::Uuid;

use super::types::{DownstreamTarget, WorkflowEvent};
use crate::{guard::guarded_execute, EngineError, NodeDispatch, NodeTrait, WorkflowContext};

/// 单节点的异步执行循环：接收 → 执行 → 分发 → 发射事件。
pub(crate) async fn run_node(
    node: Arc<dyn NodeTrait>,
    timeout: Option<Duration>,
    mut input_rx: mpsc::Receiver<WorkflowContext>,
    downstream_senders: Vec<DownstreamTarget>,
    result_tx: mpsc::Sender<WorkflowContext>,
    event_tx: mpsc::Sender<WorkflowEvent>,
) {
    let node_id = node.id().to_owned();

    while let Some(ctx) = input_rx.recv().await {
        let trace_id = ctx.trace_id;

        emit_event(
            &event_tx,
            WorkflowEvent::NodeStarted {
                node_id: node_id.clone(),
                trace_id,
            },
        )
        .await;

        let result = guarded_execute(&node_id, trace_id, timeout, node.execute(ctx)).await;

        match result {
            Ok(output) => {
                let mut send_error = None;

                for node_output in output.outputs {
                    let matching_targets = match &node_output.dispatch {
                        NodeDispatch::Broadcast => downstream_senders.iter().collect::<Vec<_>>(),
                        NodeDispatch::Route(port_ids) => downstream_senders
                            .iter()
                            .filter(|target| {
                                target
                                    .source_port_id
                                    .as_ref()
                                    .is_some_and(|port_id| {
                                        port_ids.iter().any(|candidate| candidate == port_id)
                                    })
                            })
                            .collect::<Vec<_>>(),
                    };

                    let write_result = if matching_targets.is_empty() {
                        result_tx.send(node_output.ctx).await.map_err(|_| {
                            EngineError::ChannelClosed {
                                stage: node_id.clone(),
                            }
                        })
                    } else {
                        let mut downstream_error = None;
                        for target in &matching_targets {
                            if target.sender.send(node_output.ctx.clone()).await.is_err() {
                                downstream_error = Some(EngineError::ChannelClosed {
                                    stage: node_id.clone(),
                                });
                                break;
                            }
                        }
                        if let Some(error) = downstream_error {
                            Err(error)
                        } else {
                            Ok(())
                        }
                    };

                    match write_result {
                        Ok(()) => {
                            if matching_targets.is_empty() {
                                emit_event(
                                    &event_tx,
                                    WorkflowEvent::WorkflowOutput {
                                        node_id: node_id.clone(),
                                        trace_id,
                                    },
                                )
                                .await;
                            }
                        }
                        Err(error) => {
                            send_error = Some(error);
                            break;
                        }
                    }
                }

                if let Some(error) = send_error {
                    emit_failure(&event_tx, &node_id, trace_id, &error).await;
                    break;
                }

                emit_event(
                    &event_tx,
                    WorkflowEvent::NodeCompleted {
                        node_id: node_id.clone(),
                        trace_id,
                    },
                )
                .await;
            }
            Err(error) => {
                emit_failure(&event_tx, &node_id, trace_id, &error).await;
            }
        }
    }
}

async fn emit_failure(
    event_tx: &mpsc::Sender<WorkflowEvent>,
    node_id: &str,
    trace_id: Uuid,
    error: &EngineError,
) {
    emit_event(
        event_tx,
        WorkflowEvent::NodeFailed {
            node_id: node_id.to_owned(),
            trace_id,
            error: error.to_string(),
        },
    )
    .await;
}

async fn emit_event(event_tx: &mpsc::Sender<WorkflowEvent>, event: WorkflowEvent) {
    let _ = event_tx.send(event).await;
}
```

关键变更：
- 移除 `use std::panic::AssertUnwindSafe;` 和 `use futures_util::FutureExt;`
- 移除 `#[allow(clippy::too_many_lines)]`（函数已缩短至阈值以下）
- 将 22 行内联 panic/timeout 逻辑替换为单行 `guarded_execute` 调用

- [ ] **Step 5: 重写 `src/pipeline/runner.rs` 使用 `guarded_execute`**

完整替换后的文件：

```rust
//! 单阶段异步执行循环与事件发射。
//!
//! [`run_stage`] 在独立的 Tokio 任务中运行，持续从输入通道接收上下文，
//! 执行阶段处理器，并将结果转发到下一阶段或最终结果通道。

use tokio::sync::mpsc;

use super::types::{PipelineEvent, PipelineStage};
use crate::{guard::guarded_execute, EngineError, WorkflowContext};

/// 单阶段的异步执行循环。
pub(crate) async fn run_stage(
    stage: PipelineStage,
    mut input_rx: mpsc::Receiver<WorkflowContext>,
    output_tx: Option<mpsc::Sender<WorkflowContext>>,
    result_tx: mpsc::Sender<WorkflowContext>,
    event_tx: mpsc::Sender<PipelineEvent>,
) {
    while let Some(ctx) = input_rx.recv().await {
        let trace_id = ctx.trace_id;
        let stage_name = stage.name.clone();

        emit_event(
            &event_tx,
            PipelineEvent::StageStarted {
                stage: stage_name.clone(),
                trace_id,
            },
        )
        .await;

        let result =
            guarded_execute(&stage_name, trace_id, stage.timeout, (stage.handler)(ctx)).await;

        match result {
            Ok(next_ctx) => {
                let forward_result = if let Some(tx) = &output_tx {
                    tx.send(next_ctx)
                        .await
                        .map_err(|_| EngineError::ChannelClosed {
                            stage: stage_name.clone(),
                        })
                } else {
                    result_tx
                        .send(next_ctx)
                        .await
                        .map_err(|_| EngineError::ChannelClosed {
                            stage: stage_name.clone(),
                        })
                };

                match forward_result {
                    Ok(()) => {
                        emit_event(
                            &event_tx,
                            PipelineEvent::StageCompleted {
                                stage: stage_name.clone(),
                                trace_id,
                            },
                        )
                        .await;

                        if output_tx.is_none() {
                            emit_event(&event_tx, PipelineEvent::PipelineCompleted { trace_id })
                                .await;
                        }
                    }
                    Err(error) => {
                        emit_failure(&event_tx, &stage_name, trace_id, &error).await;
                        break;
                    }
                }
            }
            Err(error) => {
                emit_failure(&event_tx, &stage_name, trace_id, &error).await;
            }
        }
    }
}

async fn emit_failure(
    event_tx: &mpsc::Sender<PipelineEvent>,
    stage: &str,
    trace_id: uuid::Uuid,
    error: &EngineError,
) {
    emit_event(
        event_tx,
        PipelineEvent::StageFailed {
            stage: stage.to_owned(),
            trace_id,
            error: error.to_string(),
        },
    )
    .await;
}

async fn emit_event(event_tx: &mpsc::Sender<PipelineEvent>, event: PipelineEvent) {
    let _ = event_tx.send(event).await;
}
```

关键变更：
- 移除 `use std::panic::AssertUnwindSafe;` 和 `use futures_util::FutureExt;`
- 将 22 行内联 panic/timeout 逻辑替换为单行 `guarded_execute` 调用

- [ ] **Step 6: 运行全量测试**

Run: `cargo test`
Expected: 全部 15 + 4 = 19 tests pass

- [ ] **Step 7: Commit**

```bash
git add src/guard.rs src/lib.rs src/graph/runner.rs src/pipeline/runner.rs
git commit -s -m "$(cat <<'EOF'
refactor: 提取 guarded_execute 统一 panic 隔离与超时保护

将 graph/runner 和 pipeline/runner 中重复的 AssertUnwindSafe +
catch_unwind + timeout 逻辑提取到 guard::guarded_execute，
消除 ~30 行重复代码，并增加 4 个单元测试覆盖守卫行为。

Co-Authored-By: Claude Opus 4.6 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

### Task 2: 定义 NodeTrait 委托宏

**Files:**
- Modify: `src/nodes/mod.rs`

- [ ] **Step 1: 在 `src/nodes/mod.rs` 的 `mod helpers;` 之前添加宏定义**

在文件开头的 `//!` 文档注释之后、`mod helpers;` 之前插入：

```rust
/// 为嵌入 `RhaiNodeBase` 的脚本节点委托 [`NodeTrait`] 元数据方法。
///
/// 需要节点结构体含有 `base: RhaiNodeBase` 字段。
macro_rules! delegate_node_base {
    ($kind:expr) => {
        fn id(&self) -> &str {
            self.base.id()
        }
        fn kind(&self) -> &'static str {
            $kind
        }
        fn ai_description(&self) -> &str {
            self.base.ai_description()
        }
    };
}
pub(crate) use delegate_node_base;

/// 为持有 `id` 和 `ai_description` 字段的非脚本节点实现 [`NodeTrait`] 元数据方法。
macro_rules! impl_node_meta {
    ($kind:expr) => {
        fn id(&self) -> &str {
            &self.id
        }
        fn kind(&self) -> &'static str {
            $kind
        }
        fn ai_description(&self) -> &str {
            &self.ai_description
        }
    };
}
pub(crate) use impl_node_meta;
```

- [ ] **Step 2: 运行编译检查**

Run: `cargo check`
Expected: 编译通过（宏已定义但尚未使用，不会产生警告）

- [ ] **Step 3: Commit**

```bash
git add src/nodes/mod.rs
git commit -s -m "$(cat <<'EOF'
refactor: 定义 delegate_node_base / impl_node_meta 宏

为 NodeTrait 的 id/kind/ai_description 三个委托方法提供宏，
后续任务将逐个节点应用以消除 ~80 行样板代码。

Co-Authored-By: Claude Opus 4.6 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

### Task 3: 应用宏到脚本节点（5 个文件）

**Files:**
- Modify: `src/nodes/rhai.rs`
- Modify: `src/nodes/if_node.rs`
- Modify: `src/nodes/switch_node.rs`
- Modify: `src/nodes/try_catch.rs`
- Modify: `src/nodes/loop_node.rs`

所有脚本节点的 `NodeTrait` impl 中，将 3 个方法替换为 `delegate_node_base!("xxx");`。

- [ ] **Step 1: 修改 `src/nodes/rhai.rs`**

添加导入并替换委托方法：

```rust
// 在文件顶部的 use 区域添加：
use super::delegate_node_base;

// 在 impl NodeTrait for RhaiNode 中，将：
//     fn id(&self) -> &str { self.base.id() }
//     fn kind(&self) -> &'static str { "rhai" }
//     fn ai_description(&self) -> &str { self.base.ai_description() }
// 替换为：
    delegate_node_base!("rhai");
```

- [ ] **Step 2: 修改 `src/nodes/if_node.rs`**

```rust
use super::delegate_node_base;
// impl NodeTrait for IfNode 中替换为：
    delegate_node_base!("if");
```

- [ ] **Step 3: 修改 `src/nodes/switch_node.rs`**

```rust
use super::delegate_node_base;
// impl NodeTrait for SwitchNode 中替换为：
    delegate_node_base!("switch");
```

- [ ] **Step 4: 修改 `src/nodes/try_catch.rs`**

```rust
use super::delegate_node_base;
// impl NodeTrait for TryCatchNode 中替换为：
    delegate_node_base!("tryCatch");
```

- [ ] **Step 5: 修改 `src/nodes/loop_node.rs`**

```rust
use super::delegate_node_base;
// impl NodeTrait for LoopNode 中替换为：
    delegate_node_base!("loop");
```

- [ ] **Step 6: 运行全量测试**

Run: `cargo test`
Expected: 全部 19 tests pass

- [ ] **Step 7: Commit**

```bash
git add src/nodes/rhai.rs src/nodes/if_node.rs src/nodes/switch_node.rs src/nodes/try_catch.rs src/nodes/loop_node.rs
git commit -s -m "$(cat <<'EOF'
refactor: 脚本节点应用 delegate_node_base 宏

5 个基于 RhaiNodeBase 的节点（rhai, if, switch, tryCatch, loop）
使用宏替代手写的 id/kind/ai_description 委托，每文件减 ~8 行。

Co-Authored-By: Claude Opus 4.6 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

### Task 4: 提取模板渲染模块

**Files:**
- Create: `src/nodes/template.rs`
- Modify: `src/nodes/mod.rs` (添加 `pub(crate) mod template;`)
- Modify: `src/nodes/http_client.rs` (移除已提取的函数，改用 template 模块)

- [ ] **Step 1: 创建 `src/nodes/template.rs`**

```rust
//! `{{placeholder}}` 模板渲染引擎。
//!
//! 本模块从 HTTP 节点中抽取，提供通用的占位符替换能力。
//! 内置变量（`trace_id`、`node_id`、`timestamp`、`payload.*`）
//! 来自工作流上下文，调用方可通过 `extras` 注入额外变量。

use serde_json::Value;
use uuid::Uuid;

/// 模板渲染时可用的变量上下文。
pub(crate) struct TemplateVars<'a> {
    pub payload: &'a Value,
    pub trace_id: &'a Uuid,
    pub node_id: &'a str,
    pub timestamp: &'a str,
    pub extras: &'a [(&'a str, &'a str)],
}

/// 沿 JSON 路径（如 `"a.b.0.c"`）在树中定位值。
pub(crate) fn resolve_json_path<'a>(root: &'a Value, path: &str) -> Option<&'a Value> {
    path.split('.')
        .filter(|segment| !segment.is_empty())
        .try_fold(root, |current, segment| match current {
            Value::Object(map) => map.get(segment),
            Value::Array(items) => segment
                .parse::<usize>()
                .ok()
                .and_then(|index| items.get(index)),
            _ => None,
        })
}

/// 将 JSON Value 转为人类可读的字符串（Null → 空串）。
pub(crate) fn value_to_display_string(value: &Value) -> String {
    match value {
        Value::Null => String::new(),
        Value::String(text) => text.clone(),
        other => other.to_string(),
    }
}

/// 截断字符串到指定字符数，超出部分用省略号替代。
pub(crate) fn truncate(text: &str, limit: usize) -> String {
    let mut result = text.chars().take(limit).collect::<String>();
    if text.chars().count() > limit {
        result.push('\u{2026}');
    }
    result
}

/// 渲染 `{{key}}` 模板，从 [`TemplateVars`] 中解析变量。
pub(crate) fn render(template: &str, vars: &TemplateVars<'_>) -> String {
    let mut result = String::with_capacity(template.len() + 48);
    let mut remaining = template;

    while let Some(start) = remaining.find("{{") {
        result.push_str(&remaining[..start]);
        let after_open = &remaining[start + 2..];

        if let Some(end) = after_open.find("}}") {
            let key = after_open[..end].trim();
            result.push_str(&resolve_key(key, vars));
            remaining = &after_open[end + 2..];
        } else {
            result.push_str(&remaining[start..]);
            return result;
        }
    }

    result.push_str(remaining);
    result
}

/// 解析单个模板变量 key。
fn resolve_key(key: &str, vars: &TemplateVars<'_>) -> String {
    match key {
        "trace_id" => vars.trace_id.to_string(),
        "node_id" => vars.node_id.to_owned(),
        "timestamp" | "event_at" => vars.timestamp.to_owned(),
        "payload" => vars.payload.to_string(),
        _ => {
            if let Some((_, value)) = vars.extras.iter().find(|(k, _)| *k == key) {
                return (*value).to_owned();
            }
            if let Some(path) = key.strip_prefix("payload.") {
                resolve_json_path(vars.payload, path)
                    .map(value_to_display_string)
                    .unwrap_or_default()
            } else {
                String::new()
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn test_vars(payload: &Value) -> TemplateVars<'_> {
        let trace_id = Box::leak(Box::new(
            Uuid::parse_str("550e8400-e29b-41d4-a716-446655440000")
                .unwrap_or_else(|_| Uuid::nil()),
        ));
        TemplateVars {
            payload,
            trace_id,
            node_id: "test-node",
            timestamp: "2026-01-01T00:00:00Z",
            extras: &[("custom_key", "custom_value")],
        }
    }

    #[test]
    fn 渲染内置变量() {
        let payload = json!({"temperature": 42});
        let vars = test_vars(&payload);
        let result = render("节点 {{node_id}} 时间 {{timestamp}}", &vars);
        assert_eq!(result, "节点 test-node 时间 2026-01-01T00:00:00Z");
    }

    #[test]
    fn 渲染_payload_路径() {
        let payload = json!({"sensor": {"temp": 55.3}});
        let vars = test_vars(&payload);
        assert_eq!(render("温度={{payload.sensor.temp}}", &vars), "温度=55.3");
    }

    #[test]
    fn 渲染额外变量() {
        let payload = json!({});
        let vars = test_vars(&payload);
        assert_eq!(
            render("自定义={{custom_key}}", &vars),
            "自定义=custom_value"
        );
    }

    #[test]
    fn 未闭合占位符保留原文() {
        let payload = json!({});
        let vars = test_vars(&payload);
        assert_eq!(render("前缀 {{未闭合", &vars), "前缀 {{未闭合");
    }

    #[test]
    fn json_path_支持数组索引() {
        let data = json!({"items": [10, 20, 30]});
        assert_eq!(
            resolve_json_path(&data, "items.1"),
            Some(&Value::from(20))
        );
    }

    #[test]
    fn 截断超长文本() {
        assert_eq!(truncate("abcde", 3), "abc\u{2026}");
        assert_eq!(truncate("ab", 3), "ab");
    }
}
```

- [ ] **Step 2: 在 `src/nodes/mod.rs` 中注册模块**

在 `mod helpers;` 之后添加：

```rust
pub(crate) mod template;
```

- [ ] **Step 3: 运行模板模块测试**

Run: `cargo test --lib nodes::template`
Expected: 6 tests pass

- [ ] **Step 4: 重构 `src/nodes/http_client.rs` 使用 template 模块**

这是最大的单文件变更。核心改动：

1. 移除已提取到 template.rs 的函数：`resolve_json_path`、`value_to_template_string`、`render_http_template`、`resolve_http_template_key`、`truncate_for_meta`
2. 导入 `super::template`
3. 在 `prepare_http_request_body` 中用 `template::render` + `template::TemplateVars` 替代直接调用
4. 在 execute 中用 `template::truncate` 替代 `truncate_for_meta`

替换后的完整 `http_client.rs`：

```rust
//! HTTP 请求节点，将 payload 发送到指定端点并将响应写入上下文。
//!
//! 支持三种 body 模式：`json`（默认）、`template`（占位符渲染）和
//! `dingtalk_markdown`（钉钉机器人 Markdown 格式）。GET/HEAD 不发送请求体，
//! 其余方法根据 body_mode 渲染请求体。响应状态码 >= 400 视为错误。

use async_trait::async_trait;
use chrono::Utc;
use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};

use super::helpers::into_payload_map;
use super::template::{self, TemplateVars};
use super::{impl_node_meta, NodeExecution, NodeTrait};
use crate::{EngineError, WorkflowContext};

fn default_http_method() -> String {
    "POST".to_owned()
}

fn default_http_webhook_kind() -> String {
    "generic".to_owned()
}

fn default_http_body_mode() -> String {
    "json".to_owned()
}

fn default_http_content_type() -> String {
    "application/json".to_owned()
}

fn default_http_request_timeout_ms() -> u64 {
    4_000
}

fn value_to_header_string(value: &Value) -> String {
    match value {
        Value::String(text) => text.clone(),
        other => other.to_string(),
    }
}

fn parse_json_or_string(raw: &str) -> Value {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        Value::Null
    } else {
        serde_json::from_str(trimmed).unwrap_or_else(|_| Value::String(trimmed.to_owned()))
    }
}

fn default_http_alarm_title_template() -> &'static str {
    "Nazh 工业告警 · {{payload.tag}} · {{payload.severity}}"
}

fn default_http_alarm_body_template() -> &'static str {
    "### Nazh 工业告警\n- 设备：{{payload.tag}}\n- 温度：{{payload.temperature_c}} °C\n- 严重级别：{{payload.severity}}\n- Trace：{{trace_id}}\n- 事件时间：{{timestamp}}"
}

fn normalize_http_webhook_kind(value: &str) -> &'static str {
    match value.trim().to_ascii_lowercase().as_str() {
        "dingtalk" | "ding_talk" | "ding-talk" => "dingtalk",
        _ => "generic",
    }
}

fn normalize_http_body_mode(value: &str, webhook_kind: &str) -> &'static str {
    match value.trim().to_ascii_lowercase().as_str() {
        "template" | "raw-template" => "template",
        "dingtalk_markdown" | "dingtalk-markdown" | "alarm-template" => "dingtalk_markdown",
        "json" | "payload-json" | "payload_json" => "json",
        _ => {
            if webhook_kind == "dingtalk" {
                "dingtalk_markdown"
            } else {
                "json"
            }
        }
    }
}

fn prepare_http_request_body(
    node_id: &str,
    config: &HttpClientNodeConfig,
    ctx: &WorkflowContext,
    requested_at: &str,
) -> Result<(String, String, String, String), EngineError> {
    let webhook_kind = normalize_http_webhook_kind(&config.webhook_kind).to_owned();
    let body_mode = normalize_http_body_mode(&config.body_mode, &webhook_kind).to_owned();
    let event_timestamp = ctx.timestamp.to_rfc3339();

    let vars = TemplateVars {
        payload: &ctx.payload,
        trace_id: &ctx.trace_id,
        node_id,
        timestamp: &event_timestamp,
        extras: &[("requested_at", requested_at)],
    };

    let body = match body_mode.as_str() {
        "template" => {
            let tpl = if config.body_template.trim().is_empty() {
                "{{payload}}"
            } else {
                config.body_template.as_str()
            };
            template::render(tpl, &vars)
        }
        "dingtalk_markdown" => {
            let title_tpl = if config.title_template.trim().is_empty() {
                default_http_alarm_title_template()
            } else {
                config.title_template.as_str()
            };
            let body_tpl = if config.body_template.trim().is_empty() {
                default_http_alarm_body_template()
            } else {
                config.body_template.as_str()
            };
            let rendered_title = template::render(title_tpl, &vars);
            let rendered_body = template::render(body_tpl, &vars);

            serde_json::to_string(&json!({
                "msgtype": "markdown",
                "markdown": {
                    "title": rendered_title,
                    "text": rendered_body,
                },
                "at": {
                    "atMobiles": config.at_mobiles,
                    "isAtAll": config.at_all,
                }
            }))
            .map_err(|error| {
                EngineError::payload_conversion(node_id.to_owned(), error.to_string())
            })?
        }
        _ => serde_json::to_string(&ctx.payload).map_err(|error| {
            EngineError::payload_conversion(node_id.to_owned(), error.to_string())
        })?,
    };

    Ok((
        body,
        config.content_type.trim().to_owned(),
        webhook_kind,
        body_mode,
    ))
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HttpClientNodeConfig {
    pub url: String,
    #[serde(default = "default_http_method")]
    pub method: String,
    #[serde(default)]
    pub headers: Map<String, Value>,
    #[serde(default = "default_http_webhook_kind")]
    pub webhook_kind: String,
    #[serde(default = "default_http_body_mode")]
    pub body_mode: String,
    #[serde(default = "default_http_content_type")]
    pub content_type: String,
    #[serde(default = "default_http_request_timeout_ms")]
    pub request_timeout_ms: u64,
    #[serde(default)]
    pub body_template: String,
    #[serde(default)]
    pub title_template: String,
    #[serde(default)]
    pub at_mobiles: Vec<String>,
    #[serde(default)]
    pub at_all: bool,
}

impl Default for HttpClientNodeConfig {
    fn default() -> Self {
        Self {
            url: String::new(),
            method: default_http_method(),
            headers: Map::new(),
            webhook_kind: default_http_webhook_kind(),
            body_mode: default_http_body_mode(),
            content_type: default_http_content_type(),
            request_timeout_ms: default_http_request_timeout_ms(),
            body_template: String::new(),
            title_template: String::new(),
            at_mobiles: Vec::new(),
            at_all: false,
        }
    }
}

/// HTTP 请求节点，内置 [`reqwest::Client`] 连接池。
pub struct HttpClientNode {
    id: String,
    ai_description: String,
    config: HttpClientNodeConfig,
    client: reqwest::Client,
}

impl HttpClientNode {
    pub fn new(
        id: impl Into<String>,
        config: HttpClientNodeConfig,
        ai_description: impl Into<String>,
    ) -> Self {
        let client = reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::limited(10))
            .build()
            .unwrap_or_default();
        Self {
            id: id.into(),
            ai_description: ai_description.into(),
            config,
            client,
        }
    }
}

#[async_trait]
impl NodeTrait for HttpClientNode {
    impl_node_meta!("httpClient");

    async fn execute(&self, ctx: WorkflowContext) -> Result<NodeExecution, EngineError> {
        let method = self.config.method.trim().to_uppercase();
        let url = self.config.url.trim().to_owned();
        if url.is_empty() {
            return Err(EngineError::node_config(
                self.id.clone(),
                "HTTP Client 节点需要配置 URL",
            ));
        }

        let requested_at = Utc::now().to_rfc3339();
        let request_timeout_ms = self.config.request_timeout_ms.max(500);
        let (payload_body, content_type, webhook_kind, body_mode) =
            prepare_http_request_body(&self.id, &self.config, &ctx, &requested_at)?;

        let reqwest_method = method.parse::<reqwest::Method>().map_err(|error| {
            EngineError::node_config(self.id.clone(), format!("无效的 HTTP 方法: {error}"))
        })?;

        let mut request = self
            .client
            .request(reqwest_method, &url)
            .timeout(std::time::Duration::from_millis(request_timeout_ms));

        for (key, value) in &self.config.headers {
            request = request.header(key.as_str(), value_to_header_string(value));
        }

        if method != "GET" && method != "HEAD" {
            let has_content_type_header = self
                .config
                .headers
                .keys()
                .any(|key| key.eq_ignore_ascii_case("content-type"));
            if !has_content_type_header && !content_type.is_empty() {
                request = request.header("Content-Type", content_type.as_str());
            }
            request = request.body(payload_body.clone());
        }

        let response = request.send().await.map_err(|error| {
            EngineError::stage_execution(
                self.id.clone(),
                ctx.trace_id,
                format!("HTTP 请求失败: {error}"),
            )
        })?;

        let status_code = response.status().as_u16();
        let response_body = response.text().await.map_err(|error| {
            EngineError::stage_execution(
                self.id.clone(),
                ctx.trace_id,
                format!("读取 HTTP 响应体失败: {error}"),
            )
        })?;
        let response_value = parse_json_or_string(&response_body);

        if status_code >= 400 {
            return Err(EngineError::stage_execution(
                self.id.clone(),
                ctx.trace_id,
                format!(
                    "HTTP Alarm 返回状态码 {status_code}: {}",
                    template::truncate(
                        &template::value_to_display_string(&response_value),
                        240
                    )
                ),
            ));
        }

        let trace_id = ctx.trace_id;
        let mut payload_map = into_payload_map(ctx.payload);
        payload_map.insert(
            "_http".to_owned(),
            json!({
                "url": url,
                "method": method,
                "webhook_kind": webhook_kind,
                "body_mode": body_mode,
                "content_type": content_type,
                "request_timeout_ms": request_timeout_ms,
                "status": status_code,
                "ok": status_code < 400,
                "requested_at": requested_at,
                "request_body_preview": template::truncate(&payload_body, 320),
            }),
        );
        payload_map.insert("http_response".to_owned(), response_value);

        Ok(NodeExecution::broadcast(WorkflowContext::from_parts(
            trace_id,
            Utc::now(),
            Value::Object(payload_map),
        )))
    }
}
```

关键变更：
- 移除 5 个已提取到 template.rs 的自由函数
- 移除 4 个 `_for_meta` 临时变量（`json!()` 直接引用原变量）
- `prepare_http_request_body` 使用 `TemplateVars` + `template::render`
- 元数据构造改用 `json!()`
- 应用 `impl_node_meta!` 宏

- [ ] **Step 5: 运行全量测试**

Run: `cargo test`
Expected: 全部 25 tests pass（原 19 + template 6）

- [ ] **Step 6: Commit**

```bash
git add src/nodes/mod.rs src/nodes/template.rs src/nodes/http_client.rs
git commit -s -m "$(cat <<'EOF'
refactor: 提取 template 模块，重构 http_client 使用共享模板引擎

将 http_client.rs 中的模板渲染逻辑提取到 nodes/template.rs，
引入 TemplateVars 上下文结构替代 6 参数函数签名，
http_client.rs 从 452 行缩减至 ~280 行。

Co-Authored-By: Claude Opus 4.6 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

### Task 5: 应用宏 + `json!()` 到非脚本节点

**Files:**
- Modify: `src/nodes/native.rs`
- Modify: `src/nodes/timer.rs`
- Modify: `src/nodes/modbus_read.rs`
- Modify: `src/nodes/debug_console.rs`
- Modify: `src/nodes/sql_writer.rs`

所有非脚本节点统一：`impl_node_meta!` 替代 3 个方法，`json!()` 替代手动 Map 构造。

- [ ] **Step 1: 修改 `src/nodes/native.rs`**

native.rs 的元数据构造是条件式的（inject 循环 + 可选 connection），不适合 json!()，仅应用宏。

导入变更：

```rust
// 添加：
use super::impl_node_meta;
```

在 `impl NodeTrait for NativeNode` 中，将 id/kind/ai_description 三个方法替换为：

```rust
    impl_node_meta!("native");
```

- [ ] **Step 2: 修改 `src/nodes/timer.rs`**

导入变更：

```rust
// 移除：
use serde_json::{Map, Value};
// 替换为：
use serde_json::{json, Value};

// 添加：
use super::impl_node_meta;
```

在 `impl NodeTrait` 中替换元数据方法为：

```rust
    impl_node_meta!("timer");
```

将 `execute` 方法中的 `_timer` 元数据构造改为 `json!()`：

```rust
    async fn execute(&self, ctx: WorkflowContext) -> Result<NodeExecution, EngineError> {
        let mut payload_map = into_payload_map(ctx.payload);

        for (key, value) in &self.config.inject {
            payload_map.insert(key.clone(), value.clone());
        }

        let existing_timer = payload_map
            .remove("_timer")
            .and_then(|value| match value {
                Value::Object(map) => Some(map),
                _ => None,
            })
            .unwrap_or_default();
        let mut timer_meta = existing_timer;
        timer_meta.insert("node_id".to_owned(), json!(self.id));
        timer_meta.insert("interval_ms".to_owned(), json!(self.config.interval_ms.max(1)));
        timer_meta.insert("immediate".to_owned(), json!(self.config.immediate));
        timer_meta.insert(
            "triggered_at".to_owned(),
            json!(Utc::now().to_rfc3339()),
        );
        payload_map.insert("_timer".to_owned(), Value::Object(timer_meta));

        Ok(NodeExecution::broadcast(WorkflowContext::from_parts(
            ctx.trace_id,
            Utc::now(),
            Value::Object(payload_map),
        )))
    }
```

注意：timer 的 `_timer` 元数据需要保留已有字段（`existing_timer`），不能用单个 `json!()` 完全替代。但可以用 `json!()` 简化 value 构造，消除 `Value::String(...)` / `Value::from(...)` / `Value::Bool(...)` 样板。

- [ ] **Step 3: 修改 `src/nodes/modbus_read.rs`**

导入变更：

```rust
// 移除：
use serde_json::{Map, Value};
// 替换为：
use serde_json::{json, Value};

// 添加：
use super::impl_node_meta;
```

替换元数据方法：

```rust
    impl_node_meta!("modbusRead");
```

将 `simulate_and_build` 中的 `_modbus` Map 构造替换为 `json!()`：

```rust
        // 将：
        // let mut modbus_meta = Map::new();
        // modbus_meta.insert("simulated".to_owned(), Value::Bool(true));
        // modbus_meta.insert("unit_id".to_owned(), Value::from(self.config.unit_id));
        // modbus_meta.insert("register".to_owned(), Value::from(self.config.register));
        // modbus_meta.insert("quantity".to_owned(), Value::from(quantity));
        // modbus_meta.insert("sampled_at".to_owned(), Value::String(Utc::now().to_rfc3339()));
        // payload_map.insert("_modbus".to_owned(), Value::Object(modbus_meta));
        // 替换为：
        payload_map.insert(
            "_modbus".to_owned(),
            json!({
                "simulated": true,
                "unit_id": self.config.unit_id,
                "register": self.config.register,
                "quantity": quantity,
                "sampled_at": Utc::now().to_rfc3339(),
            }),
        );
```

- [ ] **Step 4: 修改 `src/nodes/debug_console.rs`**

导入变更：

```rust
// 移除：
use serde_json::{Map, Value};
// 替换为：
use serde_json::{json, Value};

// 添加：
use super::impl_node_meta;
```

替换元数据方法：

```rust
    impl_node_meta!("debugConsole");
```

将 execute 中的 `_debug_console` Map 构造替换为 `json!()`：

```rust
        // 将：
        // let mut debug_meta = Map::new();
        // debug_meta.insert("label".to_owned(), Value::String(label.to_owned()));
        // debug_meta.insert("pretty".to_owned(), Value::Bool(self.config.pretty));
        // debug_meta.insert("logged_at".to_owned(), Value::String(Utc::now().to_rfc3339()));
        // payload_map.insert("_debug_console".to_owned(), Value::Object(debug_meta));
        // 替换为：
        payload_map.insert(
            "_debug_console".to_owned(),
            json!({
                "label": label,
                "pretty": self.config.pretty,
                "logged_at": Utc::now().to_rfc3339(),
            }),
        );
```

- [ ] **Step 5: 修改 `src/nodes/sql_writer.rs`**

导入变更：

```rust
// 移除：
use serde_json::{Map, Value};
// 替换为：
use serde_json::{json, Value};

// 添加：
use super::impl_node_meta;
```

替换元数据方法：

```rust
    impl_node_meta!("sqlWriter");
```

将 execute 结尾的 `_sql_writer` Map 构造替换为 `json!()`：

```rust
        // 将：
        // let mut sql_meta = Map::new();
        // sql_meta.insert("database_path".to_owned(), Value::String(database_path));
        // sql_meta.insert("table".to_owned(), Value::String(table));
        // sql_meta.insert("written_at".to_owned(), Value::String(timestamp));
        // payload_map.insert("_sql_writer".to_owned(), Value::Object(sql_meta));
        // 替换为：
        payload_map.insert(
            "_sql_writer".to_owned(),
            json!({
                "database_path": database_path,
                "table": table,
                "written_at": timestamp,
            }),
        );
```

- [ ] **Step 6: 运行全量测试**

Run: `cargo test`
Expected: 全部 25 tests pass

- [ ] **Step 7: Commit**

```bash
git add src/nodes/native.rs src/nodes/timer.rs src/nodes/modbus_read.rs src/nodes/debug_console.rs src/nodes/sql_writer.rs
git commit -s -m "$(cat <<'EOF'
refactor: 非脚本节点应用 impl_node_meta 宏和 json!() 简化

6 个非脚本节点使用宏替代元数据委托，5 个节点使用 json!()
替代手动 Map::new + insert 构造，减少 ~45 行样板代码。

Co-Authored-By: Claude Opus 4.6 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

### Task 6: 清理 `instantiate.rs` 和 `topology.rs`

**Files:**
- Modify: `src/graph/instantiate.rs`
- Modify: `src/graph/topology.rs`

- [ ] **Step 1: 为 `instantiate.rs` 提取 `resolve_description` 辅助函数**

在 `parse_config` 函数之后添加：

```rust
/// 从节点定义中获取 AI 描述，若未配置则使用 fallback。
fn resolve_description(definition: &WorkflowNodeDefinition, fallback: &str) -> String {
    definition
        .ai_description
        .clone()
        .unwrap_or_else(|| fallback.to_owned())
}
```

然后将每个 match 分支中的：

```rust
let description = definition.ai_description.clone().unwrap_or_else(|| "...".to_owned());
```

替换为：

```rust
let description = resolve_description(definition, "...");
```

同时移除函数上方的 `#[allow(clippy::too_many_lines)]`（重构后函数行数已降至阈值以下）。

- [ ] **Step 2: 清理 `topology.rs` 中不必要的 HashMap clone**

在 `topology()` 方法中，`remaining_incoming` 是 `incoming` 的 clone，但 `incoming` 在 clone 之后不再使用。将：

```rust
let mut remaining_incoming = incoming.clone();
let mut processed = 0_usize;

while let Some(node_id) = queue.pop_front() {
    processed += 1;
    if let Some(neighbors) = downstream.get(&node_id) {
        for neighbor in neighbors {
            if let Some(count) = remaining_incoming.get_mut(&neighbor.to) {
```

替换为（直接复用 `incoming`，去掉 clone）：

```rust
let mut processed = 0_usize;

while let Some(node_id) = queue.pop_front() {
    processed += 1;
    if let Some(neighbors) = downstream.get(&node_id) {
        for neighbor in neighbors {
            if let Some(count) = incoming.get_mut(&neighbor.to) {
```

- [ ] **Step 3: 运行全量测试**

Run: `cargo test`
Expected: 全部 25 tests pass

- [ ] **Step 4: Commit**

```bash
git add src/graph/instantiate.rs src/graph/topology.rs
git commit -s -m "$(cat <<'EOF'
refactor: 清理 instantiate 和 topology 中的冗余

提取 resolve_description 辅助消除 11 处重复的 ai_description 解析，
移除 topology 中不必要的 HashMap clone。

Co-Authored-By: Claude Opus 4.6 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

### Task 7: 统一执行事件模型（ADR-0004）

**Files:**
- Create: `src/event.rs`
- Modify: `src/lib.rs`
- Modify: `src/graph/types.rs` (删除 `WorkflowEvent`)
- Modify: `src/graph/runner.rs`
- Modify: `src/graph/mod.rs`
- Modify: `src/pipeline/types.rs` (删除 `PipelineEvent`)
- Modify: `src/pipeline/runner.rs`
- Modify: `src/pipeline/mod.rs`
- Modify: `tests/pipeline.rs`
- Modify: `src-tauri/src/lib.rs`
- Modify: `web/src/types.ts`
- Modify: `web/src/App.tsx`

- [ ] **Step 1: 创建 `src/event.rs`**

```rust
//! 统一的执行生命周期事件与事件发射辅助。
//!
//! [`ExecutionEvent`] 覆盖 DAG 工作流和线性流水线两种执行模式，
//! 替代原先独立的 `WorkflowEvent` 和 `PipelineEvent`。

use serde::{Deserialize, Serialize};
use tokio::sync::mpsc;
use uuid::Uuid;

use crate::EngineError;

/// 统一的执行生命周期事件。
///
/// DAG 工作流和线性流水线共享同一事件类型，
/// 前端只需注册一个事件监听器即可处理所有执行模式。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum ExecutionEvent {
    /// 阶段/节点开始执行。
    Started { stage: String, trace_id: Uuid },
    /// 阶段/节点执行完成。
    Completed { stage: String, trace_id: Uuid },
    /// 阶段/节点执行失败。
    Failed {
        stage: String,
        trace_id: Uuid,
        error: String,
    },
    /// 叶节点产出最终结果（仅 DAG 工作流模式下发出）。
    Output { stage: String, trace_id: Uuid },
    /// 整条流水线执行完毕（仅线性流水线模式下发出）。
    Finished { trace_id: Uuid },
}

/// 向事件通道发送执行事件（忽略发送失败）。
pub(crate) async fn emit_event(tx: &mpsc::Sender<ExecutionEvent>, event: ExecutionEvent) {
    let _ = tx.send(event).await;
}

/// 向事件通道发送失败事件。
pub(crate) async fn emit_failure(
    tx: &mpsc::Sender<ExecutionEvent>,
    stage: &str,
    trace_id: Uuid,
    error: &EngineError,
) {
    emit_event(
        tx,
        ExecutionEvent::Failed {
            stage: stage.to_owned(),
            trace_id,
            error: error.to_string(),
        },
    )
    .await;
}
```

- [ ] **Step 2: 更新 `src/lib.rs`**

添加模块声明和 pub use：

```rust
pub mod event;
```

在 re-exports 中，移除 `WorkflowEvent` 的导出（从 graph 模块），改为：

```rust
pub use event::ExecutionEvent;
```

同时从 graph re-exports 中移除 `WorkflowEvent`。

- [ ] **Step 3: 更新 `src/graph/types.rs`**

- 删除 `WorkflowEvent` 枚举定义（原第 62–68 行）
- 将 `WorkflowStreams` 中的 `event_rx: mpsc::Receiver<WorkflowEvent>` 改为 `mpsc::Receiver<crate::ExecutionEvent>`
- 将 `WorkflowStreams::next_event` 返回类型从 `Option<WorkflowEvent>` 改为 `Option<crate::ExecutionEvent>`
- 将 `into_receivers` 返回类型同步更新
- 将 `WorkflowDeployment::next_event` 返回类型同步更新

- [ ] **Step 4: 更新 `src/graph/runner.rs`**

- 移除 `use super::types::WorkflowEvent;`（仅保留 `DownstreamTarget`）
- 添加 `use crate::event::{emit_event, emit_failure, ExecutionEvent};`
- 删除文件底部的本地 `emit_event` 和 `emit_failure` 函数
- 将所有 `WorkflowEvent::NodeStarted { node_id: ... }` 改为 `ExecutionEvent::Started { stage: ... }`
- 将 `WorkflowEvent::NodeCompleted { node_id: ... }` 改为 `ExecutionEvent::Completed { stage: ... }`
- 将 `WorkflowEvent::WorkflowOutput { node_id: ... }` 改为 `ExecutionEvent::Output { stage: ... }`
- `NodeFailed` 的处理已由共享 `emit_failure` 覆盖

- [ ] **Step 5: 更新 `src/graph/mod.rs`**

从 re-exports 中移除 `WorkflowEvent`。

- [ ] **Step 6: 更新 `src/pipeline/types.rs`**

- 删除 `PipelineEvent` 枚举定义
- 将 `PipelineHandle` 中的 `event_rx: mpsc::Receiver<PipelineEvent>` 改为 `mpsc::Receiver<crate::ExecutionEvent>`
- 将 `PipelineHandle::next_event` 返回类型改为 `Option<crate::ExecutionEvent>`
- 在 `build_linear_pipeline` 中，将 event channel 类型从 `PipelineEvent` 改为 `crate::ExecutionEvent`

- [ ] **Step 7: 更新 `src/pipeline/runner.rs`**

- 移除 `use super::types::PipelineEvent;`
- 添加 `use crate::event::{emit_event, emit_failure, ExecutionEvent};`
- 删除文件底部的本地 `emit_event` 和 `emit_failure` 函数
- 将 `PipelineEvent::StageStarted { stage: ... }` 改为 `ExecutionEvent::Started { stage: ... }`
- 将 `PipelineEvent::StageCompleted { stage: ... }` 改为 `ExecutionEvent::Completed { stage: ... }`
- 将 `PipelineEvent::PipelineCompleted { ... }` 改为 `ExecutionEvent::Finished { ... }`
- `StageFailed` 的处理已由共享 `emit_failure` 覆盖

- [ ] **Step 8: 更新 `src/pipeline/mod.rs`**

从 re-exports 中移除 `PipelineEvent`。

- [ ] **Step 9: 运行引擎测试**

Run: `cargo test --lib`
Expected: 通过

- [ ] **Step 10: 更新 `tests/pipeline.rs`**

将所有 `PipelineEvent` 引用替换为 `ExecutionEvent`，字段名 `stage` 不变（Pipeline 本身就用 `stage`）：

```rust
// 旧：
use nazh_engine::{build_linear_pipeline, EngineError, PipelineEvent, PipelineStage, WorkflowContext};
// 新：
use nazh_engine::{build_linear_pipeline, EngineError, ExecutionEvent, PipelineStage, WorkflowContext};

// 旧：
if let Ok(Some(PipelineEvent::StageFailed { trace_id, .. })) = event {
// 新：
if let Ok(Some(ExecutionEvent::Failed { trace_id, .. })) = event {

// 旧：
Ok(Some(PipelineEvent::StageStarted { stage, .. }))
// 新：
Ok(Some(ExecutionEvent::Started { stage, .. }))
```

- [ ] **Step 11: 运行全量测试**

Run: `cargo test`
Expected: 全部测试通过

- [ ] **Step 12: 更新 `web/src/types.ts`**

```typescript
// 旧：
export interface WorkflowEvent {
  NodeStarted?: { node_id: string; trace_id: string };
  NodeCompleted?: { node_id: string; trace_id: string };
  NodeFailed?: { node_id: string; trace_id: string; error: string };
  WorkflowOutput?: { node_id: string; trace_id: string };
}

// 新：
export interface ExecutionEvent {
  Started?: { stage: string; trace_id: string };
  Completed?: { stage: string; trace_id: string };
  Failed?: { stage: string; trace_id: string; error: string };
  Output?: { stage: string; trace_id: string };
  Finished?: { trace_id: string };
}
```

- [ ] **Step 13: 更新 `web/src/App.tsx`**

将所有 `WorkflowEvent` 引用替换为 `ExecutionEvent`，字段名从 `node_id` 改为 `stage`：

- `ParsedWorkflowEvent` 接口中的字段名更新
- `parseWorkflowEventPayload` 函数中的 variant 名称更新（`NodeStarted` → `Started` 等）
- 涉及 `event.NodeStarted?.node_id` 的代码改为 `event.Started?.stage`

- [ ] **Step 14: 检查 Tauri 编译**

Run: `cargo check --manifest-path src-tauri/Cargo.toml`
Expected: 编译通过（Tauri 桥接层中 `event_rx.recv()` 的类型自动跟随上游变更）

- [ ] **Step 15: Commit**

```bash
git add src/event.rs src/lib.rs src/graph/ src/pipeline/ tests/pipeline.rs src-tauri/src/lib.rs web/src/types.ts web/src/App.tsx
git commit -s -m "$(cat <<'EOF'
refactor(ADR-0004): 统一 ExecutionEvent 替代双事件模型

用 ExecutionEvent 替代 WorkflowEvent 和 PipelineEvent，
两个 runner 共享 emit_event/emit_failure 辅助，
前端同步更新为统一事件接口。

Co-Authored-By: Claude Opus 4.6 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

### Task 8: ConnectionManager 细粒度连接锁（ADR-0005）

**Files:**
- Modify: `src/connection.rs`
- Modify: `src/nodes/helpers.rs`
- Modify: `src/graph/deploy.rs`
- Modify: `src-tauri/src/lib.rs`

- [ ] **Step 1: 重写 `src/connection.rs`**

核心变更：
- `SharedConnectionManager` 从 `Arc<RwLock<ConnectionManager>>` 改为 `Arc<ConnectionManager>`
- 内部结构改为 `RwLock<HashMap<String, Arc<Mutex<ConnectionRecord>>>>`
- 所有方法改为 `async`（通过 `&self` 而非 `&mut self` 访问）

```rust
//! 全局连接资源池。
//!
//! 节点绝不直接访问硬件。所有协议连接（Modbus、MQTT、HTTP 等）
//! 均注册在 [`ConnectionManager`] 中，通过 `Arc<ConnectionManager>`
//! 以借出/归还模式访问。每个连接拥有独立的锁，
//! 不同连接的并发借出互不阻塞。

use std::{collections::HashMap, sync::Arc};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio::sync::{Mutex, RwLock};

use crate::EngineError;

/// 全局连接池的线程安全句柄。
pub type SharedConnectionManager = Arc<ConnectionManager>;

/// 连接资源的声明式定义（用于工作流 AST）。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ConnectionDefinition {
    pub id: String,
    #[serde(rename = "type", alias = "kind")]
    pub kind: String,
    #[serde(default)]
    pub metadata: Value,
}

/// 由 [`ConnectionManager::borrow`] 返回的临时借出连接句柄。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ConnectionLease {
    pub id: String,
    pub kind: String,
    pub metadata: Value,
    pub borrowed_at: DateTime<Utc>,
}

/// 已注册连接的内部记录，追踪其借出状态。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ConnectionRecord {
    pub id: String,
    pub kind: String,
    pub metadata: Value,
    pub in_use: bool,
    pub last_borrowed_at: Option<DateTime<Utc>>,
}

/// 管理具名连接资源池，采用排他借出语义。
///
/// 每个连接拥有独立的 [`Mutex`] 锁，不同连接的并发借出互不阻塞。
/// 注册操作使用外层 [`RwLock`] 保护，仅在部署时调用一次。
#[derive(Default)]
pub struct ConnectionManager {
    connections: RwLock<HashMap<String, Arc<Mutex<ConnectionRecord>>>>,
}

/// 创建一个空的 [`ConnectionManager`]，包装在 `Arc` 中。
pub fn shared_connection_manager() -> SharedConnectionManager {
    Arc::new(ConnectionManager::default())
}

impl ConnectionManager {
    /// 注册新连接。若 ID 已存在则返回错误。
    ///
    /// # Errors
    ///
    /// 连接 ID 已存在时返回 [`EngineError::ConnectionAlreadyExists`]。
    pub async fn register_connection(
        &self,
        definition: ConnectionDefinition,
    ) -> Result<(), EngineError> {
        let connections = self.connections.read().await;
        if connections.contains_key(&definition.id) {
            return Err(EngineError::ConnectionAlreadyExists(definition.id));
        }
        drop(connections);
        self.upsert_connection(definition).await;
        Ok(())
    }

    /// 插入或替换连接定义（幂等操作）。
    pub async fn upsert_connection(&self, definition: ConnectionDefinition) {
        let record = ConnectionRecord {
            id: definition.id.clone(),
            kind: definition.kind,
            metadata: definition.metadata,
            in_use: false,
            last_borrowed_at: None,
        };
        let mut connections = self.connections.write().await;
        connections.insert(definition.id, Arc::new(Mutex::new(record)));
    }

    /// 批量插入或替换连接定义。
    pub async fn upsert_connections(
        &self,
        definitions: impl IntoIterator<Item = ConnectionDefinition>,
    ) {
        let mut connections = self.connections.write().await;
        for definition in definitions {
            let record = ConnectionRecord {
                id: definition.id.clone(),
                kind: definition.kind,
                metadata: definition.metadata,
                in_use: false,
                last_borrowed_at: None,
            };
            connections.insert(definition.id, Arc::new(Mutex::new(record)));
        }
    }

    /// 排他借出一个连接。若已被借出或不存在则返回错误。
    ///
    /// 仅锁定目标连接，不阻塞其他连接的并发借出。
    ///
    /// # Errors
    ///
    /// 连接不存在时返回 [`EngineError::ConnectionNotFound`]，
    /// 已被借出时返回 [`EngineError::ConnectionBusy`]。
    pub async fn borrow(&self, connection_id: &str) -> Result<ConnectionLease, EngineError> {
        let connections = self.connections.read().await;
        let entry = connections
            .get(connection_id)
            .cloned()
            .ok_or_else(|| EngineError::ConnectionNotFound(connection_id.to_owned()))?;
        drop(connections);

        let mut record = entry.lock().await;
        if record.in_use {
            return Err(EngineError::ConnectionBusy(connection_id.to_owned()));
        }

        let borrowed_at = Utc::now();
        record.in_use = true;
        record.last_borrowed_at = Some(borrowed_at);

        Ok(ConnectionLease {
            id: record.id.clone(),
            kind: record.kind.clone(),
            metadata: record.metadata.clone(),
            borrowed_at,
        })
    }

    /// 将已借出的连接归还到资源池。
    ///
    /// # Errors
    ///
    /// 连接不存在时返回 [`EngineError::ConnectionNotFound`]。
    pub async fn release(&self, connection_id: &str) -> Result<(), EngineError> {
        let connections = self.connections.read().await;
        let entry = connections
            .get(connection_id)
            .cloned()
            .ok_or_else(|| EngineError::ConnectionNotFound(connection_id.to_owned()))?;
        drop(connections);

        let mut record = entry.lock().await;
        record.in_use = false;
        Ok(())
    }

    /// 返回单个连接记录的快照。
    pub async fn get(&self, connection_id: &str) -> Option<ConnectionRecord> {
        let connections = self.connections.read().await;
        let entry = connections.get(connection_id)?.clone();
        drop(connections);
        Some(entry.lock().await.clone())
    }

    /// 返回所有已注册连接的快照列表。
    pub async fn list(&self) -> Vec<ConnectionRecord> {
        let connections = self.connections.read().await;
        let mut result = Vec::with_capacity(connections.len());
        for entry in connections.values() {
            result.push(entry.lock().await.clone());
        }
        result
    }
}
```

关键变更：
- `SharedConnectionManager` = `Arc<ConnectionManager>`（不再有外层 `RwLock`）
- 内部每连接独立 `Mutex`，`borrow` 只锁目标连接
- 所有方法通过 `&self`（而非 `&mut self`）访问
- `borrow`/`release` 中先 `clone` Arc，再 `drop` 外层读锁，避免持锁跨 await

- [ ] **Step 2: 更新 `src/nodes/helpers.rs` 中的 `with_connection`**

`SharedConnectionManager` 现在是 `Arc<ConnectionManager>`，不再需要 `.write().await`：

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

    let result = operation(lease.as_ref());

    if let Some(conn_id) = connection_id {
        let release_result = connection_manager.release(conn_id).await;
        if result.is_ok() {
            release_result?;
        }
    }

    result
}
```

变更：`.write().await` 调用全部移除，直接调用 `connection_manager.borrow()` / `.release()`。

- [ ] **Step 3: 更新 `src/graph/deploy.rs`**

将连接注册从 `.write().await.upsert_connections(...)` 改为直接调用：

```rust
// 旧：
if !graph.connections.is_empty() {
    let mut manager = connection_manager.write().await;
    let connections = std::mem::take(&mut graph.connections);
    manager.upsert_connections(connections);
}

// 新：
if !graph.connections.is_empty() {
    let connections = std::mem::take(&mut graph.connections);
    connection_manager.upsert_connections(connections).await;
}
```

- [ ] **Step 4: 更新 `src-tauri/src/lib.rs`**

将 `list_connections` 从 `.read().await.list()` 改为直接调用：

```rust
// 旧：
let connections = state.connection_manager.read().await.list();

// 新：
let connections = state.connection_manager.list().await;
```

- [ ] **Step 5: 更新 `tests/workflow.rs`**

两处需要修改：

1. Line 129 — 旧 API `connection_manager.read().await.list()` 改为：

```rust
let connections = connection_manager.list().await;
```

2. Lines 162-186 — 同步测试改为 async（方法签名已变为 async）：

```rust
#[tokio::test]
async fn connection_manager_borrows_and_releases_connections() {
    let manager = ConnectionManager::default();
    let register_result = manager.register_connection(ConnectionDefinition {
        id: "plc-1".to_owned(),
        kind: "modbus".to_owned(),
        metadata: json!({ "unit_id": 1 }),
    })
    .await;
    assert!(register_result.is_ok(), "connection should register");

    let lease = manager.borrow("plc-1").await;
    assert!(lease.is_ok(), "connection should be borrowable");

    let second_borrow = manager.borrow("plc-1").await;
    match second_borrow {
        Ok(_) => panic!("second borrow should fail"),
        Err(EngineError::ConnectionBusy(connection_id)) => {
            assert_eq!(connection_id, "plc-1");
        }
        Err(error) => panic!("unexpected error: {error}"),
    }

    let release_result = manager.release("plc-1").await;
    assert!(release_result.is_ok(), "connection should release");
}
```

注意：`let mut manager` 改为 `let manager`（方法现在通过 `&self` 访问，不再需要 `mut`）。

- [ ] **Step 6: 运行全量测试**

Run: `cargo test`
Expected: 全部测试通过

- [ ] **Step 7: 检查 Tauri 编译**

Run: `cargo check --manifest-path src-tauri/Cargo.toml`
Expected: 编译通过

- [ ] **Step 8: Commit**

```bash
git add src/connection.rs src/nodes/helpers.rs src/graph/deploy.rs src-tauri/src/lib.rs tests/workflow.rs
git commit -s -m "$(cat <<'EOF'
refactor(ADR-0005): ConnectionManager 改用细粒度连接锁

内部结构从全局 RwLock 改为 RwLock<HashMap<String, Arc<Mutex>>>，
不同连接的并发借出互不阻塞。SharedConnectionManager 简化为
Arc<ConnectionManager>，调用方无需手动获取外层锁。

Co-Authored-By: Claude Opus 4.6 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

### Task 9: 最终验证

- [ ] **Step 1: 运行全量测试**

Run: `cargo test`
Expected: 全部测试通过

- [ ] **Step 2: 运行 clippy**

Run: `cargo clippy --all-targets -- -D warnings`
Expected: 无警告

- [ ] **Step 3: 运行 fmt 检查**

Run: `cargo fmt --all -- --check`
Expected: 无格式差异

- [ ] **Step 4: 检查 Tauri shell 编译**

Run: `cargo check --manifest-path src-tauri/Cargo.toml`
Expected: 编译通过

- [ ] **Step 5: 检查前端编译**

Run: `npm --prefix web run build`
Expected: 编译通过
