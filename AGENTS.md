# AGENTS.md

This file is the **single source of truth** for contributing to Nazh — design invariants, coding rules, collaboration conventions, and documentation discipline.

It is read by both humans and AI agents (Claude Code, OpenCode, Cursor, etc.). `CLAUDE.md` is a **symbolic link** to this file, so tools that look for Claude-specific guidance find the same content. When any other doc (README / rustdoc / ADR body / memory / comment) conflicts with this file, **this file wins** — the conflict is a bug to open a PR against.

## Project Overview

Nazh is an industrial-edge workflow orchestration engine with AI as a first-class capability. It connects device ingestion, data transformation, scripted logic, AI-assisted authoring, and a desktop operations UI into a single local runtime.

Stack: **Rust engine (Cargo workspace, 9 crates) + Tauri v2 desktop shell + React 18 / FlowGram.AI canvas**.

Everything runs in one process — no HTTP/gRPC server, no external broker. AI features (script generation, thinking-mode completions, workflow composition) flow through the `AiService` trait in Ring 0 (`nazh_core::ai`); the OpenAI-compatible HTTP/SSE implementation lives in the `ai` crate (ADR-0019).

## Build & Dev Commands

```bash
# Install frontend dependencies
npm --prefix web install

# Start desktop dev mode (Tauri auto-launches Vite on port 1420)
cd src-tauri && ../web/node_modules/.bin/tauri dev --no-watch

# Engine tests (all workspace members)
cargo test --workspace

# Re-generate TypeScript types from Rust (ts-rs)
# 经 tauri-bindings 的 ts-export feature 传递触发全工作区导出（ADR-0017）
cargo test -p tauri-bindings --features ts-export export_bindings

# Tauri shell compile-check only
cargo check --manifest-path src-tauri/Cargo.toml

# Frontend
npm --prefix web run test          # Vitest unit tests
npm --prefix web run test:e2e      # Playwright E2E (needs compiled Tauri app)
npm --prefix web run build         # Production build

# Lint & format (run both before committing)
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings

# Single test by name
cargo test <test_name>

# Example
cargo run --example phase1_demo

# Dependency audit (requires cargo-deny)
cargo deny check
```

## Architecture

### Three-Layer Stack

1. **Rust Engine** — Cargo workspace rooted at `/` with 9 crates (see below). Public facade is the `nazh-engine` library crate at `src/lib.rs`.
2. **Tauri Shell** (`src-tauri/`) — Desktop app binary `nazh-desktop`. Exposes IPC commands to the frontend, bridges engine events to the UI, manages shell-side concerns (observability store, project library files, MQTT/Timer/Serial trigger supervisors).
3. **React Frontend** (`web/`) — Vite + React 18 + TypeScript + FlowGram.AI. Communicates **exclusively** via Tauri `invoke` / `Window::emit` — no HTTP or gRPC.

### Cargo Workspace Layout (Ring 0 / Ring 1)

```
crates/
  core/              # Ring 0 — NodeTrait, Plugin, DataStore, ResourceGuard, EngineError, ExecutionEvent
                     #   Zero protocol dependencies. The engine kernel.
  pipeline/          # Ring 1 — linear pipeline abstraction
  connections/       # Ring 1 — ConnectionManager, ConnectionGuard RAII, health/circuit-breaker
  scripting/         # Ring 1 — Rhai engine base (with AI-aware ai_complete() helper)
  nodes-flow/        # Ring 1 — if / switch / loop / tryCatch / code (Rhai script)
  nodes-io/          # Ring 1 — timer / serial / native / modbus / http / mqtt / bark / sql / debugConsole
  ai/                # Ring 1 — OpenAI-compatible client (streaming, thinking-mode) + 壳层配置型；AiService trait 在 Ring 0（ADR-0019）
  tauri-bindings/    # IPC — Tauri 命令请求/响应类型 + ts-rs 导出汇总（ADR-0017）
src/                 # Root facade crate `nazh-engine` — DAG orchestration + `standard_registry()`
src-tauri/           # Tauri shell binary `nazh-desktop`
web/                 # Frontend workspace
```

**Ring rules** (enforced by convention, verified at review):
- Ring 0 (`crates/core/`) depends on no other workspace crate. It may depend on `tokio`, `serde`, `thiserror`, `chrono`, `dashmap`, etc. — but never on protocol crates (`reqwest`, `rumqttc`, `rusqlite`, `tokio-modbus`) and never on `ts-rs` as a hard dependency (`ts-rs` is feature-gated via `ts-export`，详见 ADR-0017).
- Ring 1 crates may depend on Ring 0 and on sibling Ring 1 crates where it makes sense (`nodes-io` depends on `connections`). Avoid creating cycles.
- The facade (`src/`) may depend on everything.
- The Tauri shell (`src-tauri/`) depends on the facade.

See RFC-0002 (`docs/rfcs/0002-分层内核与插件架构.md`) for the full rationale.

### Data Flow

```
React / FlowGram canvas
  → Export JSON AST
  → Tauri invoke("deploy_workflow")
  → Rust: parse AST → validate DAG (Kahn) → per-node Tokio task via NodeRegistry
  → Nodes communicate via MPSC channels carrying ContextRef (~64 bytes)
  → Actual payloads live in DataStore (ArenaDataStore by default)
  → Events via Window::emit("workflow://node-status", "workflow://result")
  → Frontend updates canvas highlights, Runtime dock, log panel
```

### Engine Core Modules

- **`crates/core/src/context.rs`** — `WorkflowContext` (trace_id, timestamp, payload) + `ContextRef` (DataStore pointer). Kept to three fields; metadata does NOT live here.
- **`crates/core/src/data.rs`** — `DataStore` trait + `ArenaDataStore` (in-memory default). `DataId` indexes payloads with Arc ref-counting.
- **`crates/core/src/event.rs`** — `ExecutionEvent { Started, Completed(CompletedExecutionEvent), Failed, Output, Finished }`. `Completed` carries a `metadata: Option<Map<String, Value>>` — execution metadata walks this channel, not the data channel.
- **`crates/core/src/node.rs`** — `NodeTrait` with async `transform(trace_id, payload) → NodeExecution`. `NodeOutput { payload, metadata, dispatch }`. `NodeDispatch::Broadcast | Route(Vec<String>)`.
- **`crates/core/src/plugin.rs`** — `Plugin` + `NodeRegistry` + `PluginHost` + `RuntimeResources` (typed Any bag). Engine core has **zero hardcoded nodes** — all Ring 1 crates register themselves.
- **`crates/core/src/guard.rs`** — panic isolation helpers (AssertUnwindSafe + catch_unwind + timeout).
- **`src/graph/`** — `WorkflowGraph` schema, topology (Kahn cycle check), `deploy_workflow` / `deploy_workflow_with_ai`, per-node `run_node` loop. See ADR-0020 for long-term placement.

### Type Contract (ts-rs)

IPC boundary types are defined once in Rust and auto-generated to TypeScript via **ts-rs**, ensuring frontend/backend type safety at compile time.

- Rust structs use `#[cfg_attr(feature = "ts-export", derive(TS), ts(export))]`（不再无条件 `#[derive(TS)]`），生成 `.ts` 文件到 `web/src/generated/`。
- `web/src/types.ts` re-exports generated types and extends them with frontend-only fields.
- `tsc` errors if Rust types change without regenerating — run `cargo test -p tauri-bindings --features ts-export export_bindings` after any Rust type change.
- IPC response types (`DeployResponse`, `DispatchResponse`, `UndeployResponse`, `NodeTypeEntry`, `ListNodeTypesResponse`) live in `crates/tauri-bindings/src/lib.rs`（since ADR-0017 已实施）。该 crate 还提供 `list_node_types_response(&NodeRegistry)` 与统一的 `export_all()` ts-rs 入口。`ts-rs` 在各 crate 中通过 `ts-export` feature 门控；只需启用 `tauri-bindings` 的 `ts-export` feature 即可经依赖传递触发全工作区导出。

### Tauri IPC Surface (`src-tauri/src/lib.rs`)

~22 commands covering: workflow lifecycle (`deploy_workflow`, `dispatch_payload`, `undeploy_workflow`, `list_runtime_workflows`, `set_active_runtime_workflow`, `list_dead_letters`, `list_node_types`), connections (`list_connections`, `load/save_connection_definitions`), serial (`list_serial_ports`, `test_serial_connection`), AI (`list_ai_providers`, `save_ai_provider`, `delete_ai_provider`, `test_ai_provider`, `copilotComplete`), observability (`query_observability`), deployment persistence, project library.

Event channels: `workflow://node-status`, `workflow://result`, `workflow://deployed`, `workflow://undeployed`, `workflow://runtime-focus`. Runtime result/status events are scoped payloads with `workflow_id`.

## Critical Coding Constraints

Industrial-reliability requirements. **Enforced by Cargo lints**; violations fail CI.

1. **No `.unwrap()` / `.expect()` in production code.** `clippy::unwrap_used = "deny"` + `clippy::expect_used = "deny"` at workspace level. All errors flow through `Result<T, EngineError>` using `thiserror`. Test modules may opt in with `#[allow(clippy::unwrap_used)]` per-module.
2. **No `unsafe`.** `unsafe_code = "forbid"` at workspace level.
3. **Panic isolation is mandatory.** Node execution is wrapped in `AssertUnwindSafe + catch_unwind + timeout`. One bad node must never crash the DAG.
4. **Nodes never access hardware directly.** All I/O goes through `ConnectionManager` (borrow → use → release via RAII `ConnectionGuard`).
5. **Channel-based message passing over shared state.** Tokio MPSC between nodes. The only shared mutable state is `ConnectionManager` behind `Arc<RwLock<...>>` and `DataStore` behind `Arc<dyn DataStore>`.
6. **Rhai scripts must have step limits** (`max_operations`, default 50k) to prevent infinite loops.
7. **`NodeTrait::transform(trace_id, payload) → NodeExecution` is the contract.** Nodes must not touch `DataStore`. The Runner is solely responsible for store reads/writes.
8. **Execution metadata must not leak into payload.** Return metadata via `NodeOutput::metadata` + `NodeExecution::with_metadata()`, using non-underscore keys (`"timer"`, `"http"`, `"modbus"`, `"serial"`, `"sql_writer"`, `"debug_console"`, `"connection"`, `"bark"`, `"ai"`). The Runner merges metadata into `ExecutionEvent::Completed` events. Only routing context (`_loop`, `_error`) is allowed to remain in the payload. See ADR-0008.
9. **Field visibility: prefer private + getters for stable core types.** `WorkflowNodeDefinition` is the reference pattern — fields are private, access via `id()` / `node_type()` / `connection_id()` / etc., mutations only through methods like `normalize()` and `config_mut()`. Apply the same to future stable types.

## Design Principles (team-aligned contract)

These principles guide day-to-day decisions. When in doubt, reach for the principle that preserves these.

1. **ADR-driven architecture evolution.** Non-trivial architecture changes go through an ADR (`docs/adr/NNNN-title.md`). Existing code changes that embody a decision should be recorded retrospectively (e.g. ADR-0008 documents the metadata separation that landed before the ADR existed). "Evaluation ADRs" (like ADR-0020) record *decisions not to act*, with trigger conditions.
2. **Control plane vs data plane separation.** Payload (business data) flows through `DataStore` + `ContextRef`. Metadata (observability, provenance) flows through `ExecutionEvent`. Configuration (setpoints, thresholds, shared state) will flow through `WorkflowVariables` (ADR-0012, pending). These planes do not cross-contaminate.
3. **Ring purity.** Ring 0 stays free of protocol-specific dependencies. Ring 1 crates depend on Ring 0 and may compose horizontally, but should avoid creating cross-Ring-1 fan-out cycles. Prefer **trait abstraction + dependency injection** over direct imports when coupling Ring 1 crates (ADR-0019 proposes this for AI).
4. **RAII for resources.** Connections, lifecycle guards, and future resource holders use Drop-based cleanup, never explicit `close()` / `release()` call pairs. Examples: `ConnectionGuard`（连接借出）、`LifecycleGuard`（节点后台任务，ADR-0009）。
5. **Plugin-first node registration.** Adding a node means implementing `NodeTrait` in a Ring 1 crate and registering via `Plugin::register(&mut NodeRegistry)`. Do not hardcode node types in the engine or facade. `standard_registry()` in `src/lib.rs` loads the baseline set (`FlowPlugin`, `IoPlugin`) — other plugins can be added to compose custom deployments.
6. **Fast fail, loud logs.** Deploy-time validation (DAG, types, configs) is cheap and should happen before any node runs. Runtime failures emit `ExecutionEvent::Failed` with `trace_id`, `stage`, and structured error via `tracing::error!`. Silent drops are bugs.
7. **AI is a first-class capability, not a bolt-on.** `ai` crate provides `AiService` trait + OpenAI-compatible client. Scripts call `ai_complete()`. The `code` node has built-in AI generation. Future AI providers (Anthropic native, local Llama, Qwen) should implement `AiService`, not replace the call site.

## Collaboration Conventions

### Language

- **Code & docs in Chinese (中文):** All Rust doc comments (`///`, `//!`), error messages, log messages, commit messages, inline comments. TypeScript/JSDoc comments too. CHANGELOG entries and ADR bodies are in Chinese.
- **English for tooling:** CLAUDE.md, RFCs (optional), README.md English-language sections can stay English. File/module/function names are English (Rust convention). Chinese symbol names are allowed in test function names (`fn 事件通道关闭时不崩溃()`).

### Git

- **Sign-off required** on every commit: `git commit -s`. CI rejects unsigned commits.
- **Commit messages in Chinese**, prefixed by type: `feat:` / `fix:` / `refactor:` / `docs:` / `test:` / `chore:` / `perf:`.
- **One concern per commit.** "One PR, many commits" is fine; "one commit, five concerns" makes bisecting painful.
- **No `--amend` on pushed commits.** Revise via new commit.
- **No `--no-verify` / `--no-gpg-sign`** unless explicitly approved.
- **Destructive git ops (force push, reset --hard, branch -D)** require user confirmation before execution. Destroy nothing without asking.

### Code Review

- **Check invariants** from the "Critical Coding Constraints" list — they're not just style preferences, they are reliability claims.
- **Trait signatures and public APIs** are contract changes. Flag them in the PR description. Private field changes behind getters are OK as long as the getter surface is preserved.
- **Run `cargo test --workspace` + `cargo clippy --workspace --all-targets -- -D warnings` + `cargo fmt --all -- --check`** locally before requesting review. CI enforces all three.
- **Regenerate `web/src/generated/` types** if you changed any `#[ts(export)]` struct. Diff-check the generated TS before committing.
- **UI/frontend changes:** start the dev server and exercise the feature in a browser. Type-checking passes ≠ feature works.

### Memory System (Claude Code sessions)

When Claude Code is used to work on this repo, a persistent memory system at `~/.claude/projects/-home-zhihongniu-Nazh/memory/` carries context across sessions:

- **`project_system_architecture.md`** — current ring layout, phase progress, known tech debt.
- **`project_architecture_review_2026_04.md`** — proposal ↔ ADR mapping (提案-01~09 ↔ ADR-0008~0016).
- **`user_nazh_owner.md`** — owner profile and working preferences.
- **`MEMORY.md`** — index.

**Updating memory:** when a commit materially changes the architecture state (Phase completes, ADR lands, tech debt paid/created), update the relevant memory file in the same PR. Stale memory misleads future sessions.

## Documentation Rules

Documents rot silently. These rules exist to keep docs synchronized with code so that a new contributor (or an AI agent reading `AGENTS.md`) can always trust what they read.

### Single Source of Truth

**`AGENTS.md` is the authority.** `CLAUDE.md` is a symlink to `AGENTS.md` — both names exist so that Claude Code, OpenCode, Cursor, and other agent tools find it via their native conventions, but the content is one file.

**Per-crate AGENTS.md** — Each workspace crate (`crates/*/`) has exactly one `AGENTS.md` documenting crate-specific design intent and conventions. No `README.md` / `CLAUDE.md` at the crate level — AGENTS.md is the cross-vendor agent convention (Claude Code, Copilot CLI, Codex, Aider all find it), so one file is enough. Root `AGENTS.md` owns cross-cutting rules (language, git, testing, layout); crate `AGENTS.md` owns that crate's internal contracts, dependency constraints, and "how to modify" checklists. If they conflict, root wins.

**约定（conventions）住在 AGENTS.md，不住在 ADR 里。** ADR 记录的是"我们做了什么决定、为什么"——一次性的架构抉择。落地后的"位分配表 / 节点能力对照 / checklist / 修改时同步哪些文件"这类**随代码演进而更新**的约定，放在对应 crate 的 `AGENTS.md` 或 rustdoc 里。ADR 通常不再更新，约定随时可更新。

When `AGENTS.md` conflicts with any other doc (README / rustdoc / ADR / memory / comment), **`AGENTS.md` wins and the conflict is a bug to fix**. Open a PR to resolve.

### Freshness Contract

1. **Same-PR doc updates.** A PR that changes crate layout, public API, build commands, Critical Coding Constraints, Design Principles, IPC surface, or node inventory **must** update `AGENTS.md` in the same PR. Reviewers enforce this.
2. **Cite `file:line` or commit SHA for volatile references.** E.g., "see `src-tauri/src/lib.rs:2499`" rather than "see the shell layer". When a cited location moves, the citation moves with it — or the whole section rewrites.
3. **Date-stamp decay-prone sections.** "Project Status" / "Known tech debt" / "Current roadmap" sections must carry a date header (YYYY-MM-DD). Review them at every major release boundary.
4. **Evaluation ADRs must declare a revisit trigger.** Any "提议中 / 暂缓" ADR that intentionally defers action (e.g. ADR-0020) must list an observable condition (metric, row count, calendar date) that forces reconsideration.
5. **Memory files are point-in-time observations.** Files in `~/.claude/projects/.../memory/` may be read for context but **must** be verified against current code before making a claim or recommendation. Stale memory = recorded lies; when detected, update in the same session.
6. **Old plans and specs are kept as historical record.** Don't delete `docs/superpowers/plans/*.md` after merge — prepend a `> **Status:** merged in <SHA> / superseded by <new plan>` line at the top. Future archaeologists rely on this.

### Documentation Triggers (When X → Update Y)

| When you ... | You must update ... |
|--------------|---------------------|
| Add / rename / remove a crate | Root `AGENTS.md` workspace layout + root `README.md` crate table + new crate's own `AGENTS.md` (single file, no symlinks) |
| Change a crate's public API, internal contracts, or dependency constraints | That crate's `AGENTS.md` (the "对外暴露" / "内部约定" / "依赖约束" / "修改本 crate 时" sections) in the same PR |
| Add a new `#[ts(export)]` type or rename one | Run `cargo test -p tauri-bindings --features ts-export export_bindings`; commit the diff in `web/src/generated/` |
| Add / remove a `NodeTrait` implementation | The owning crate's `AGENTS.md` node inventory table + `src/registry.rs` contract test + root `README.md` node catalog |
| Change a node's `capabilities()` | The owning crate's `AGENTS.md` capability table + `src/registry.rs` contract test — both in the same PR (one without the other fails CI) |
| Accept / implement / deprecate an ADR | The ADR's own status + `docs/adr/README.md` index row (do **not** update past ADRs with new implementation details — put those in the relevant crate `AGENTS.md`) |
| Add a Tauri IPC command or event channel | Root `AGENTS.md` Tauri IPC surface + root `README.md` IPC tables + `crates/tauri-bindings/AGENTS.md` if it introduces new response types |
| Change any Critical Coding Constraint | Root `AGENTS.md` (this file) + signal explicitly in PR description |
| Complete a roadmap item in RFC-0002 | Update the RFC's "Implementation Progress" section |
| Land work matching a 提案 in architecture review memory | Update `memory/project_architecture_review_2026_04.md` status mapping |
| Add or rewrite a large module | Ensure `//!` module-level doc reflects purpose; run `cargo doc --no-deps` to sanity-check; update crate `AGENTS.md` if the module is part of public API |
| Rename or move a file cited in an ADR/README/crate AGENTS.md | Update the citation; don't leave dead paths |

### ADR Writing Requirements

**When to write an ADR** (`docs/adr/NNNN-title.md`):
- Decision that establishes a non-obvious invariant, even if small (e.g. "metadata via event channel, not payload" → ADR-0008)
- Decision that took > 1 discussion or rejected a plausible alternative
- Retrospective: discovering an undocumented invariant in existing code
- Evaluation: deciding *not* to act, with revisit trigger

**When NOT to write an ADR:**
- "How to implement feature X" — that's an implementation plan (`docs/superpowers/plans/`)
- Pure implementation detail with no architectural implication
- Decisions obvious from the code itself

**Required structure** (follow `docs/adr/template.md`):
- Front-matter: `状态` / `日期` / `决策者` / `关联`（可选）
- `## 背景` — what problem, what constraints, what was observed
- `## 决策` — the decision itself, stated as a quote block `> 我们决定 ...`
- `## 可选方案` — **at least 3** alternatives labeled 方案 A/B/C(/D), each with 优势 / 劣势
- `## 后果` — three subsections: `### 正面影响` / `### 负面影响` / `### 风险` (with mitigations)
- `## 备注` — references, implementation notes, related work

**Numbering & filename:**
- Sequential `NNNN`, starting from `0001`. Next number = `max(existing) + 1`
- No gap-filling of deleted/deprecated ADRs (deleted numbers stay buried)
- Filename: `NNNN-<kebab-case-chinese-title>.md` — Chinese title is fine, avoid slashes/spaces

**Status lifecycle:**

```
提议中  →  已接受  →  已实施
              ↓          ↓
           已废弃  /  已取代（写明替代 ADR 编号）
```

Never move backward (已接受 → 提议中 is invalid; write a new ADR that updates the old one's status to 已取代).

**Language:** Body is Chinese. Headings are Chinese. Code snippets inside are Rust / TOML / shell (unchanged).

### RFC Writing Requirements

**ADR vs RFC distinction:**
- **ADR** = "here's what we decided and why" — structured, narrow scope, records a commitment
- **RFC** = "let's explore this design space" — prose, broad scope, may result in 0-N ADRs

**When to write an RFC** (`docs/rfcs/NNNN-title.md`):
- Major architectural changes (e.g. RFC-0002 layered-core + plugin system)
- Cross-cutting subsystem proposals (e.g. RFC-0001 node plugin mechanism)
- Exploring a design space where the final decision isn't ready

**Suggested sections** (freer than ADR):
- `## 动机` — why are we exploring this?
- `## 需求与约束` — hard requirements and negotiables
- `## 设计` — main content, can include diagrams / pseudocode / phase plans
- `## 备选方案考虑` — space of alternatives
- `## 实施拆解` — phases / milestones (for subsequent ADRs to cite)
- `## 风险与未知` — what could go wrong, what's undecided

**RFC → ADR flow:**
1. Write RFC exploring the space
2. When decisions crystallize, spin off focused ADRs that **cite the RFC** in their 关联 field
3. Track RFC implementation progress in the RFC itself (e.g. RFC-0002 has "Phase N: status" list)
4. Don't rewrite decisions into the RFC — that's the ADRs' job

**Numbering & filename:** Same rules as ADR, in `docs/rfcs/`. RFCs and ADRs have **independent** number spaces.

### Plan & Spec Writing Requirements

Located in `docs/superpowers/plans/` and `docs/superpowers/specs/`. These are **working documents** for implementation.

**Plans** (`docs/superpowers/plans/YYYY-MM-DD-topic.md`):
- Use when implementation will touch > 2 files or take > 1 day
- Required header: `**Goal:**`, `**Architecture:**`, `**Tech Stack:**`
- Body is checkbox tasks (`- [ ]`) organized by logical steps
- Cross off (`- [x]`) as work progresses, **in the same PR as the code change**
- On merge, prepend a status line at top: `> **Status:** merged in <commit-SHA>`
- Plans are never deleted — they're historical record of how things got built

**Specs** (`docs/superpowers/specs/YYYY-MM-DD-topic-design.md`):
- Use for "design phase" output before a plan
- Longer prose, design tradeoffs, UI mockups, API sketches
- When a spec converts to a plan, cross-link both: spec notes "implemented via <plan-file>", plan notes "designed in <spec-file>"

### Code Comments

**Language:** Chinese for all comment content (`///`, `//!`, `//`, `/* */`, JSDoc).

**When to comment:**
- `//!` at file head — what this module is for, key invariants
- `///` on public API — explain WHY and invariants, not just WHAT (the signature already shows WHAT)
- Inline `//` — non-obvious logic, references to ADR/RFC/issue numbers, workarounds for external bugs

**When NOT to comment:**
- Restating what code already says ("increment counter" on `counter += 1` is noise)
- Multi-paragraph rustdoc on private helpers — one short line if needed
- Generated files (TS bindings, build output)

**Obsolete comments are bugs.** When you refactor away the meaning of a comment, delete the comment. Lying comments are worse than missing comments.

**TODO/FIXME discipline:** Don't commit bare `// TODO: something` — include an issue number, ADR reference, or explicit owner: `// TODO(ADR-0009): 迁回引擎生命周期钩子`. Bare TODOs rot faster than the code they annotate.

## Testing

### Layers

- **Rust unit tests (`#[cfg(test)]` modules):** per-crate, ran via `cargo test --workspace --lib`. Scripting, plugin system, event emission, template engine, etc.
- **Rust integration tests (`tests/`):**
  - `tests/pipeline.rs` — linear pipeline timing/error/panic/timeout.
  - `tests/workflow.rs` — DAG end-to-end with connection pool, Rhai integration, AI helpers.
- **Frontend unit tests (`web/src/lib/__tests__/`):** Vitest. Event parsing, state reduction, workflow status, settings, graph parsing, layout, FlowGram conversion, workflow orchestrator, AI config state.
- **Frontend E2E (`web/e2e/`):** Playwright against compiled Tauri app. Deploy / dispatch / undeploy lifecycle.

### When to add what

- New `NodeTrait` implementation → add a unit test to the same crate + integrate into `tests/workflow.rs` with a minimal DAG.
- New Tauri command → add an integration test that calls it via Tauri's test harness, or add to `web/src/lib/__tests__/tauri.test.ts` for the TS wrapper.
- Bug fix → add a regression test that would have caught the bug, before fixing.
- Refactor (no behavior change) → existing tests must pass unchanged; if they require modification, that's a behavior change in disguise — split the PR.

## Project Status

**Phases 1-5 complete** (crate extraction, DataStore, ConnectionGuard, Ring 1 split, Plugin system). See `docs/rfcs/0002-分层内核与插件架构.md`.

**Current batch of ADRs** (2026-04-17 to 2026-04-26):
- ADR-0008 (metadata separation) — **accepted / landed**
- ADR-0017 (IPC + ts-rs 迁出 Ring 0) — **已实施**（2026-04-24，见 `crates/tauri-bindings/`）
- ADR-0011 (节点能力标签 `NodeCapabilities`) — **已实施**（2026-04-24，位图落在 `crates/core/src/node.rs`，前端常量表 `web/src/lib/node-capabilities.ts`）
- ADR-0009 (节点生命周期钩子) — **已实施**（2026-04-26，`crates/core/src/lifecycle.rs` + Timer / Serial / MQTT 三类节点 `on_deploy` + `WorkflowDeployment::shutdown`；壳层 ~1000 行回收）
- ADR-0010 (Pin 声明系统) — **已实施 Phase 1 + Phase 3**（Phase 1: 2026-04-26，`crates/core/src/pin.rs` + `NodeTrait::input_pins/output_pins` 默认 Any/Any + `src/graph/pin_validator.rs` 阶段 0.5 校验 + `if`/`switch`/`loop`/`tryCatch` 四个分支节点声明具体输出 pin。Phase 3: 2026-04-26，`modbusRead` / `sqlWriter` / `httpClient` / `mqttClient` 四协议节点 input/output 收紧到 `Json`（mqttClient 按 mode 实例方法切换 pin）+ 兼容矩阵合约 fixture `tests/fixtures/pin_compat_matrix.jsonc` 作为前后端共享真值源 + 反向兼容性集成测试。Phase 2 前端可视化（多端口画布 + 连接期 UI 校验 + IPC `describe_node_pins`）另立 plan）
- ADR-0019 (AI 能力依赖反转) — **已实施**（2026-04-26，`AiService` trait + 请求/响应类型上移到 `crates/core/src/ai.rs`；`ai` crate 改为纯实现 + 配置型；`scripting` / `nodes-flow` 不再依赖 `ai`）
- ADR-0018 (`nodes-io` 按协议 feature 门控) — **已实施**（2026-04-26，`io-sql/io-http/io-mqtt/io-modbus/io-serial/io-notify` + 元 feature `io-all`；facade 转传；`debug/native/timer/template` 永远启用）
- ADR-0012 ~ ADR-0016 / ADR-0020 — **proposed**, awaiting review. See `docs/adr/README.md` for the index.

**Immediate known tech debt:**
- ~~MQTT subscriber / Timer / Serial root lifecycle is owned by the Tauri shell.~~ **已偿还（2026-04-26，ADR-0009 已实施）**。三类触发器节点现自持 `on_deploy` + `LifecycleGuard`；壳层 `src-tauri/src/lib.rs` 由 3609 行降至 2498 行（约 -31%）。**语义变化**：触发器节点走 `NodeHandle::emit` 而非 `dispatch_router` 的 trigger lane，失去 backpressure / DLQ / retry / metrics 防御能力，等 ADR-0014 / ADR-0016 引擎级背压能力补回。
- ~~IPC response types in `crates/core/` contradict Ring 0 purity. ADR-0017 plans to extract `crates/tauri-bindings/`.~~ **已偿还（2026-04-24，ADR-0017 已实施）**
- ~~`cargo clippy --workspace --all-targets -- -D warnings` 在 `src-tauri` 与 observability 上失败~~ **已偿还（2026-04-26，见 `docs/superpowers/plans/2026-04-25-cargo-clippy-workspace-fixes.md`）**。`crates/nodes-io/src/http_client.rs` / `bark_push.rs` 的 `too_many_lines` 现以 `#[allow]` 抑制（同上）。

**ADR Execution Order**（2026-04-24 共识，依赖与独立性已分析过）：

> 0. ✅ **ADR-0017** IPC + ts-rs 迁出 Ring 0 — 已实施（独立支线，crate 卫生）
> 1. ✅ **ADR-0011** 节点能力标签 — 已实施（首发第一阶段：`NodeTrait::capabilities()`、`NodeRegistry::register_with_capabilities`、IPC `NodeTypeEntry.capabilities` 透传、前端 badges；Runner 侧 `spawn_blocking` / 缓存等调度决策按 ADR 后续阶段推进）
> 2. ✅ **ADR-0009** 生命周期钩子（`on_deploy` + `LifecycleGuard`）— **已实施**（2026-04-26，Ring 0 lifecycle 模块 + Runner 两阶段部署 + Timer/Serial/MQTT 三类节点迁回；壳层 `src-tauri/src/lib.rs` 由 3609 → 2498 行）
> 3. ✅ **ADR-0010** Pin 声明系统 — **Phase 1 + Phase 3 已实施**（2026-04-26，Phase 1: Ring 0 类型 + 部署期校验器 + 4 个分支节点；Phase 3: 4 协议节点 input/output 收紧到 `Json`（保守方案，不引入 `Custom`——见 ADR 备注）+ 兼容矩阵合约 fixture 前后端共享。Phase 2 前端多端口画布 + 连接期 UI 校验另立 plan）
> 4. ✅ **ADR-0018 / ADR-0019**（独立支线，**已实施**，2026-04-26）— `nodes-io` 协议 feature 门控 + AI 依赖反转。`nazh-core::ai` 现为 trait + 类型源头；`nodes-io` 协议 dep 全部 optional
> 5. **ADR-0012** 工作流变量（依赖 0009 + 0010）
> 6. **ADR-0013** 子图与宏（依赖 0010）
> 7. **Phase 6 (RFC-0002)** EventBus + EdgeBackpressure + ConcurrencyPolicy — 与 Pin 系统可并行
> 8. **ADR-0014** Exec/Data 边分离 — 本批最重，待 Pin 系统稳定后再启动
> 9. **ADR-0015 / ADR-0016** 反应式数据引脚 + 边级可观测性 — polish 阶段
> 10. 真实协议驱动扩展（OPC-UA、Kafka 消费者等）
> 11. AI 能力扩展（embeddings、vision，未来 ADR）

**评估性 ADR**：
- ADR-0020 `src/graph/` 编排层归属 — 当前不实施，待触发条件出现（见 ADR 正文）。
