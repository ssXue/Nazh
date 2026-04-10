# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

Nazh is an industrial edge workflow orchestration engine prototype. It connects device ingestion, data transformation, scripted logic, and a desktop operations UI into a lightweight local runtime. The stack is Rust engine + Tauri v2 desktop shell + React/FlowGram.AI canvas frontend.

## Build & Dev Commands

```bash
# Install frontend dependencies
npm --prefix web install

# Start desktop dev mode (Tauri auto-launches the Vite dev server on port 1420)
cd src-tauri && ../web/node_modules/.bin/tauri dev --no-watch

# Run engine tests
cargo test

# Check Tauri shell compiles
cargo check --manifest-path src-tauri/Cargo.toml

# Regenerate TypeScript types from Rust (ts-rs)
TS_RS_EXPORT_DIR=web/src/generated cargo test --lib export_bindings

# Build frontend
npm --prefix web run build

# Run a single test by name
cargo test <test_name>

# Run example
cargo run --example phase1_demo

# Lint & format
cargo fmt --all -- --check
cargo clippy --all-targets -- -D warnings

# Generate documentation
cargo doc --no-deps --open

# Dependency audit (requires cargo-deny)
cargo deny check
```

## Architecture

### Three-Layer Stack

1. **Rust Engine** (`src/`) — Core library crate `nazh_engine`. Defines the workflow DAG, node execution, pipeline abstraction, and connection pool.
2. **Tauri Shell** (`src-tauri/`) — Desktop app binary `nazh-desktop`. Depends on `nazh-engine` via local path. Exposes four IPC commands to the frontend and bridges engine events to the UI.
3. **React Frontend** (`web/`) — Vite + React 18 + TypeScript. Uses FlowGram.AI for the visual node/edge canvas editor. Communicates exclusively via Tauri `invoke` / `Window::emit` (no HTTP/gRPC).

### Data Flow

```
React/FlowGram canvas → Export JSON AST → Tauri invoke("deploy_workflow")
  → Rust: parse AST → validate DAG → spawn Tokio task per node
  → Nodes communicate via MPSC channels
  → Events emitted back via Window::emit("workflow://node-status", "workflow://result")
  → Frontend updates canvas highlights and RuntimeDock
```

### Type Contract (ts-rs)

IPC boundary types are defined once in Rust and auto-generated to TypeScript via **ts-rs**, ensuring frontend/backend type safety at compile time.

- Rust structs annotated with `#[derive(TS)]` + `#[ts(export)]` generate `.ts` files to `web/src/generated/`.
- `web/src/types.ts` re-exports generated types and extends them with frontend-only fields (`meta.position`, `editor_graph`).
- `tsc` will error if Rust types change without regenerating (see Build Commands above).
- Response types (`DeployResponse`, `DispatchResponse`, `UndeployResponse`) live in `src/ipc.rs` (engine crate), not in src-tauri.

### Engine Core Modules (`src/`)

- **`context.rs`** — `WorkflowContext`: the immutable data envelope (trace_id, timestamp, JSON payload) that flows through the DAG. Must be `Clone + Send`.
- **`event.rs`** — `ExecutionEvent`: unified lifecycle events (Started, Completed, Failed, Output, Finished) shared by both DAG and pipeline execution modes.
- **`graph.rs`** — `WorkflowGraph` + `deploy_workflow()`: DAG parsing from JSON AST, topological sort (Kahn's algorithm), cycle detection, and async deployment. Each node gets its own Tokio task connected by MPSC channels.
- **`nodes.rs`** — `NodeTrait` (async interface) with two implementations:
  - `NativeNode`: Rust-native logic (protocol I/O, data injection, connection borrowing).
  - `RhaiNode`: Sandboxed scripting via embedded Rhai engine with configurable step limit (`max_operations`, default 50k).
- **`pipeline.rs`** — `build_linear_pipeline()`: sequential stage execution with per-stage timeouts, panic isolation (`catch_unwind`), and event channels.
- **`connection.rs`** — `ConnectionManager`: global `Arc<RwLock<...>>` resource pool. Nodes borrow/release connections; never access hardware directly. Currently a skeleton (no Modbus/MQTT/HTTP drivers yet).
- **`ipc.rs`** — IPC response types (`DeployResponse`, `DispatchResponse`, `UndeployResponse`) shared with the Tauri shell and auto-exported to TypeScript via ts-rs.
- **`error.rs`** — `EngineError` enum via `thiserror`. All errors propagate structured context (node_id, trace_id, stage name).

### Tauri Commands (`src-tauri/src/lib.rs`)

Four commands exposed to the frontend (response types defined in `src/ipc.rs`):
- `deploy_workflow(ast: String)` → `DeployResponse` — Deserialize + validate + deploy DAG, spawn event/result forwarding tasks.
- `dispatch_payload(payload: Value)` → `DispatchResponse` — Submit a `WorkflowContext` to all root nodes of the active workflow.
- `undeploy_workflow()` → `UndeployResponse` — Tear down the active workflow and abort timer tasks.
- `list_connections()` → `Vec<ConnectionRecord>` — Snapshot of the connection pool.

### Frontend Key Files (`web/src/`)

- `generated/` — Auto-generated TypeScript type definitions from Rust via ts-rs. Do NOT edit manually.
- `types.ts` — Re-exports IPC contract types from `generated/`, extends them with frontend-only fields (`meta`, `editor_graph`), plus `SAMPLE_AST`, `SAMPLE_PAYLOAD`, and pure-frontend types.
- `lib/tauri.ts` — Tauri IPC wrappers and event listeners.
- `lib/flowgram.ts` — Bidirectional conversion between Nazh AST and FlowGram WorkflowJSON.
- `lib/graph.ts` — Client-side topological sort for layout positioning.
- `App.tsx` — Main orchestrator: state management, panel routing, workflow deployment lifecycle.

## Git Conventions

- All commits must use `--signoff` (`git commit -s`) to add a `Signed-off-by` trailer.

## Language Convention

- All code comments, doc comments (`///`, `//!`), error messages, and log messages must be written in **Chinese (中文)**.
- TypeScript/JSDoc comments in the frontend follow the same rule.
- File-level documentation, CHANGELOG entries, and commit messages also use Chinese.
- CLAUDE.md and AI-Context.md remain in their current language for tooling compatibility.

## Critical Coding Constraints

These come from the project's industrial reliability requirements (see `AI-Context.md`):

- **Never use `.unwrap()` or `.expect()` in Rust.** Enforced by `clippy::unwrap_used = "deny"` and `clippy::expect_used = "deny"` in `Cargo.toml`. All errors must propagate via `Result<T, EngineError>` using `thiserror`. The runtime must never panic.
- **`unsafe` code is forbidden.** Enforced by `unsafe_code = "forbid"` in `Cargo.toml`.
- **Panic isolation is mandatory.** Use `AssertUnwindSafe` + `catch_unwind` around node execution to prevent one node from crashing the DAG.
- **Nodes never access hardware directly.** All I/O goes through `ConnectionManager` (borrow → use → release pattern).
- **Channel-based message passing over shared state.** Use Tokio MPSC channels for inter-node data flow. The only shared mutable state is `ConnectionManager` behind `Arc<RwLock<...>>`.
- **Rhai scripts must have step limits** (`max_operations`) to prevent infinite loops in user-provided code.
- **Every node has an `ai_description` field** for future LLM-driven script generation.

## Testing

Integration tests live in `tests/`:
- `tests/pipeline.rs` — Linear pipeline transformations and error resilience.
- `tests/workflow.rs` — End-to-end DAG execution with Rhai integration and connection pool.

All tests run with `cargo test`. There are no frontend tests currently.

## Development Phases (from AI-Context.md)

The project follows a phased roadmap. Phases 1-5 (infrastructure → nodes → AST parsing → connection manager → Tauri + frontend) are complete. Next priorities: real protocol drivers (Modbus TCP, MQTT, HTTP), runtime observability, AI copilot script generation, and workflow persistence.
