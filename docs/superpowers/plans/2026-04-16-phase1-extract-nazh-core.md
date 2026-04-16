# Phase 1: 提取 nazh-core 实施计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 创建 Cargo workspace，将 5 个无依赖的 Ring 0 模块（error / context / event / guard / ipc）提取到 `nazh-core` crate，nazh-engine 变为 facade 重导出。前端零影响，所有测试通过。

**Architecture:** 在根 Cargo.toml 添加 `[workspace]` 段，根 crate 保持为 `nazh-engine`（不移动 `src/` 目录）。新建 `crates/nazh-core/` 承载 Ring 0 类型。nazh-engine 依赖 nazh-core 并 `pub use` 重导出，对外 API 签名完全不变。

**Tech Stack:** Rust workspace, Cargo, ts-rs

---

### Task 1: 创建 Cargo workspace 和 nazh-core crate 骨架

**Files:**
- Modify: `Cargo.toml` (根 — 添加 `[workspace]` 段)
- Create: `crates/nazh-core/Cargo.toml`
- Create: `crates/nazh-core/src/lib.rs`

- [ ] **Step 1: 在根 Cargo.toml 顶部添加 workspace 定义**

在 `[package]` 段之前插入：

```toml
[workspace]
members = [".", "crates/nazh-core", "src-tauri"]
resolver = "2"

[workspace.lints.rust]
unsafe_code = "forbid"

[workspace.lints.clippy]
unwrap_used = "deny"
expect_used = "deny"
pedantic = { level = "warn", priority = -1 }
module_name_repetitions = "allow"
must_use_candidate = "allow"
```

同时将根 `[package]` 下的 `[lints.rust]` 和 `[lints.clippy]` 段替换为：

```toml
[lints]
workspace = true
```

- [ ] **Step 2: 在 src-tauri/Cargo.toml 中继承 workspace lints**

将 `src-tauri/Cargo.toml` 中的：

```toml
[lints.rust]
unsafe_code = "forbid"

[lints.clippy]
unwrap_used = "deny"
expect_used = "deny"
```

替换为：

```toml
[lints]
workspace = true
```

- [ ] **Step 3: 创建 nazh-core crate 目录和 Cargo.toml**

```bash
mkdir -p crates/nazh-core/src
```

`crates/nazh-core/Cargo.toml`:

```toml
[package]
name = "nazh-core"
version = "0.1.0"
edition = "2021"
rust-version = "1.94"
description = "Nazh 引擎内核：Ring 0 类型定义与基础原语"
license = "MIT"
authors = ["Niu Zhihong"]
repository = "https://github.com/zhihongniu/Nazh"

[lints]
workspace = true

[dependencies]
chrono = { version = "0.4", features = ["clock", "serde"] }
futures-util = "0.3"
serde = { version = "1", features = ["derive"] }
serde_json = "1"
thiserror = "1"
tokio = { version = "1", features = ["sync", "time"] }
ts-rs = { version = "10", features = ["serde-compat", "serde-json-impl", "chrono-impl", "uuid-impl"] }
uuid = { version = "1", features = ["serde", "v4"] }
```

注意：无 `rhai`、`reqwest`、`rusqlite` — 这是 Ring 0 内核的依赖边界约束。

`crates/nazh-core/src/lib.rs`:

```rust
//! # Nazh Core
//!
//! Nazh 引擎的 Ring 0 内核，定义工作流运行时的基础类型与原语。
//!
//! 本 crate 不包含任何具体节点实现、脚本引擎或协议驱动，
//! 仅提供引擎运行所需的最小类型集合。
```

- [ ] **Step 4: 在根 Cargo.toml 添加 nazh-core 依赖**

在 `[dependencies]` 段添加：

```toml
nazh-core = { path = "crates/nazh-core" }
```

- [ ] **Step 5: 验证 workspace 编译**

```bash
cargo check --workspace
```

预期：成功，无错误。

- [ ] **Step 6: 提交**

```bash
git add Cargo.toml src-tauri/Cargo.toml crates/
git commit -s -m "chore: 创建 Cargo workspace 并添加 nazh-core 骨架 crate"
```

---

### Task 2: 迁移 error.rs 到 nazh-core

**Files:**
- Create: `crates/nazh-core/src/error.rs` (从 `src/error.rs` 复制)
- Modify: `crates/nazh-core/src/lib.rs`
- Modify: `src/error.rs` (改为重导出)
- Modify: `src/lib.rs` (更新导出路径)

- [ ] **Step 1: 复制 error.rs 到 nazh-core**

```bash
cp src/error.rs crates/nazh-core/src/error.rs
```

error.rs 无 `use crate::` 引用，无需改内部导入。

- [ ] **Step 2: 在 nazh-core/src/lib.rs 中声明并导出**

```rust
//! # Nazh Core
//!
//! Nazh 引擎的 Ring 0 内核，定义工作流运行时的基础类型与原语。
//!
//! 本 crate 不包含任何具体节点实现、脚本引擎或协议驱动，
//! 仅提供引擎运行所需的最小类型集合。

pub mod error;

pub use error::EngineError;
```

- [ ] **Step 3: 将 src/error.rs 替换为重导出**

将 `src/error.rs` 的全部内容替换为：

```rust
//! 引擎错误类型（委托至 [`nazh_core::error`]）。

pub use nazh_core::error::*;
```

- [ ] **Step 4: 验证编译**

```bash
cargo check --workspace
```

预期：成功。nazh-engine 中所有 `use crate::EngineError` 都通过 re-export 解析到 nazh-core 的实现。

---

### Task 3: 迁移 context.rs 到 nazh-core

**Files:**
- Create: `crates/nazh-core/src/context.rs` (从 `src/context.rs` 复制)
- Modify: `crates/nazh-core/src/lib.rs`
- Modify: `src/context.rs` (改为重导出)

- [ ] **Step 1: 复制 context.rs 到 nazh-core**

```bash
cp src/context.rs crates/nazh-core/src/context.rs
```

context.rs 无 `use crate::` 引用，无需改内部导入。

- [ ] **Step 2: 更新 nazh-core/src/lib.rs**

添加模块声明和导出：

```rust
pub mod context;
pub mod error;

pub use context::WorkflowContext;
pub use error::EngineError;
```

- [ ] **Step 3: 将 src/context.rs 替换为重导出**

```rust
//! 工作流上下文（委托至 [`nazh_core::context`]）。

pub use nazh_core::context::*;
```

- [ ] **Step 4: 验证编译**

```bash
cargo check --workspace
```

预期：成功。

---

### Task 4: 迁移 event.rs 到 nazh-core

**Files:**
- Create: `crates/nazh-core/src/event.rs` (从 `src/event.rs` 复制，修改内部导入)
- Modify: `crates/nazh-core/src/lib.rs`
- Modify: `src/event.rs` (改为重导出)

- [ ] **Step 1: 复制 event.rs 到 nazh-core 并修改内部导入**

```bash
cp src/event.rs crates/nazh-core/src/event.rs
```

编辑 `crates/nazh-core/src/event.rs`，将第 11 行的：

```rust
use crate::EngineError;
```

改为：

```rust
use crate::error::EngineError;
```

（在 nazh-core 内部，`crate::` 指向 nazh-core 自身，EngineError 在 `crate::error` 模块中。）

- [ ] **Step 2: 更新 nazh-core/src/lib.rs**

```rust
pub mod context;
pub mod error;
pub mod event;

pub use context::WorkflowContext;
pub use error::EngineError;
pub use event::ExecutionEvent;
```

- [ ] **Step 3: 将 src/event.rs 替换为重导出**

```rust
//! 执行生命周期事件（委托至 [`nazh_core::event`]）。

pub use nazh_core::event::*;
```

- [ ] **Step 4: 验证编译**

```bash
cargo check --workspace
```

预期：成功。event.rs 中的 `emit_event` 和 `emit_failure` 是 `pub(crate)` 函数，在 nazh-core 中它们的可见性限制在 nazh-core 内部。nazh-engine 中使用它们的模块（graph/runner.rs）通过 `use crate::event::{emit_event, emit_failure}` 引用——这会失败，因为它们现在是 nazh-core 的 `pub(crate)`。

**修复：** 将 `crates/nazh-core/src/event.rs` 中 `emit_event` 和 `emit_failure` 的可见性从 `pub(crate)` 改为 `pub`，并在 `src/event.rs` 的重导出中补充：

```rust
//! 执行生命周期事件（委托至 [`nazh_core::event`]）。

pub use nazh_core::event::*;
```

由于 nazh-engine 的 `src/event.rs` 用 `pub use nazh_core::event::*` 重导出了所有公共项，而 runner.rs 等引用 `crate::event::{emit_event, emit_failure}` 将正确解析。

- [ ] **Step 5: 再次验证编译**

```bash
cargo check --workspace
```

---

### Task 5: 迁移 guard.rs 到 nazh-core

**Files:**
- Create: `crates/nazh-core/src/guard.rs` (从 `src/guard.rs` 复制，修改内部导入)
- Modify: `crates/nazh-core/src/lib.rs`
- Modify: `src/guard.rs` (改为重导出)

- [ ] **Step 1: 复制 guard.rs 到 nazh-core 并修改内部导入**

```bash
cp src/guard.rs crates/nazh-core/src/guard.rs
```

编辑 `crates/nazh-core/src/guard.rs`，将第 11 行的：

```rust
use crate::EngineError;
```

改为：

```rust
use crate::error::EngineError;
```

- [ ] **Step 2: guard.rs 中的可见性**

guard.rs 中 `guarded_execute` 是 `pub(crate)`。同 event.rs 的处理，改为 `pub` 以便 nazh-engine 通过重导出使用。

- [ ] **Step 3: 更新 nazh-core/src/lib.rs**

```rust
pub mod context;
pub mod error;
pub mod event;
pub mod guard;

pub use context::WorkflowContext;
pub use error::EngineError;
pub use event::ExecutionEvent;
```

不在 lib.rs 中 `pub use guard::*`——guard 是内部工具，仅供 nazh-engine 的 runner 直接引用。

- [ ] **Step 4: 将 src/guard.rs 替换为重导出**

```rust
//! 异步执行守卫（委托至 [`nazh_core::guard`]）。

pub use nazh_core::guard::*;
```

- [ ] **Step 5: 验证编译和测试**

```bash
cargo check --workspace
cargo test -p nazh-core
```

预期：guard.rs 自带 4 个单元测试，应全部通过。

---

### Task 6: 迁移 ipc.rs 到 nazh-core

**Files:**
- Create: `crates/nazh-core/src/ipc.rs` (从 `src/ipc.rs` 复制)
- Modify: `crates/nazh-core/src/lib.rs`
- Modify: `src/ipc.rs` (改为重导出)

- [ ] **Step 1: 复制 ipc.rs 到 nazh-core**

```bash
cp src/ipc.rs crates/nazh-core/src/ipc.rs
```

ipc.rs 无 `use crate::` 引用，无需改内部导入。

- [ ] **Step 2: 更新 nazh-core/src/lib.rs**

```rust
pub mod context;
pub mod error;
pub mod event;
pub mod guard;
pub mod ipc;

pub use context::WorkflowContext;
pub use error::EngineError;
pub use event::ExecutionEvent;
pub use ipc::{DeployResponse, DispatchResponse, UndeployResponse};
```

- [ ] **Step 3: 将 src/ipc.rs 替换为重导出**

```rust
//! IPC 响应类型（委托至 [`nazh_core::ipc`]）。

pub use nazh_core::ipc::*;
```

- [ ] **Step 4: 验证编译**

```bash
cargo check --workspace
```

---

### Task 7: 全量验证

- [ ] **Step 1: 全部测试**

```bash
cargo test --workspace
```

预期：所有引擎单元测试 + guard 单元测试 + 集成测试（tests/workflow.rs, tests/pipeline.rs）全部通过。

- [ ] **Step 2: Tauri 壳层编译**

```bash
cargo check --manifest-path src-tauri/Cargo.toml
```

预期：成功。src-tauri 依赖 nazh-engine，nazh-engine 重导出所有类型，无变化。

- [ ] **Step 3: ts-rs 类型导出**

```bash
TS_RS_EXPORT_DIR=web/src/generated cargo test --workspace --lib export_bindings 2>&1 | tail -5
```

预期：所有 ts-rs export 测试通过，`web/src/generated/` 下的 `.ts` 文件内容不变。

- [ ] **Step 4: Clippy**

```bash
cargo clippy --workspace --all-targets -- -D warnings
```

预期：无警告。

- [ ] **Step 5: 前端构建**

```bash
npm --prefix web run build
```

预期：成功，零错误。

- [ ] **Step 6: 验证 nazh-core 无协议依赖**

```bash
cargo tree -p nazh-core --depth 1
```

预期输出中不应出现 `rhai`、`reqwest`、`rusqlite`、`serialport`。

---

### Task 8: 更新文档和提交

- [ ] **Step 1: 更新 CLAUDE.md 中的 ts-rs 命令**

将 CLAUDE.md 中的：

```
TS_RS_EXPORT_DIR=web/src/generated cargo test --lib export_bindings
```

改为：

```
TS_RS_EXPORT_DIR=web/src/generated cargo test --workspace --lib export_bindings
```

- [ ] **Step 2: 最终提交**

```bash
git add -A
git commit -s -m "refactor: 提取 nazh-core crate (Ring 0) — error/context/event/guard/ipc

将 5 个无外部依赖的基础类型模块迁移到 nazh-core crate，
建立 Cargo workspace 结构。nazh-engine 通过 pub use 重导出，
对外 API 零变化。

RFC-0002 Phase 1 第一步：画线。"
```
