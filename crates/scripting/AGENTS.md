# crates/scripting — 脚本引擎基座

> **Ring**: Ring 1
> **对外 crate 名**: `scripting`
> **职责**: 为所有基于脚本的节点（`if` / `switch` / `loop` / `tryCatch` / `code`）提供统一的 Rhai 引擎封装
>
> 根 `AGENTS.md` 的约束对本 crate 同样适用。

## 这个 crate 做什么

Nazh 有 5 个节点类型都依赖脚本求值（ADR-0002 选了 Rhai）。为了避免重复 `Engine` 初始化、脚本编译、
`Scope` 注入、step-limit 配置，本 crate 提供 `ScriptNodeBase` 作为**组合基座**：
新节点把它作为字段嵌入即可复用整套脚本执行能力。

核心抽象：
- `ScriptNodeBase` — 持有 `AST` / `Scope` / `Engine` / `id` / `max_operations` / 可选 `Arc<dyn AiService>`（`AiService` 来自 `nazh_core::ai`）
- `ScriptNodeBase::evaluate(payload)` — 注入 payload 为 `payload` 变量，求值，返回 `(scope, result)`
- `ScriptNodeBase::evaluate_catching(payload)` — 捕获脚本错误而非直接传播（给 `tryCatch` 用）
- `ScriptNodeBase::payload_from_scope(&scope)` — 把 scope 里的 `payload` 变量读回 JSON
- `NazhScriptPackage` — 自定义 Rhai 包，提供 `ai_complete()`、`sleep_ms()` 等内建函数
- `delegate_node_base!` 宏 — 给嵌入 `base` 字段的节点委托 `NodeTrait::{id, kind}`

## 对外暴露

```text
crates/scripting/src/
├── lib.rs           # ScriptNodeBase + default_max_operations + delegate_node_base! 宏
└── package.rs       # NazhScriptPackage（Rhai 自定义包）
```

关键 API：`ScriptNodeBase`、`NazhScriptPackage`、`default_max_operations()`（50,000 步）、`delegate_node_base!`。

## 内部约定

1. **step-limit 是硬约束**。所有脚本必须设 `Engine::max_operations` 上限，默认 50k 步。用户可通过 config 调整但不允许取消。这是避免"脚本死循环吃满 CPU"的最后防线（ADR-0002）。
2. **脚本拿不到 `DataStore` 也拿不到 `ConnectionManager`**。脚本只看见 payload 变量（JSON → Rhai Dynamic），所以"副作用只能通过返回值表达"。这让 `if` / `switch` 天然满足 `PURE` 能力标签。
3. **`code` 节点例外**：它可以通过 `ai_complete()` 发 AI 请求——这是**内建的、显式的**副作用。由 `NazhScriptPackage` 注入，`AiService` trait 来自 Ring 0（`nazh_core::ai`），具体实现由 `ai` crate 提供（ADR-0019 实施后）。这也是为什么 `code` 节点不标 `PURE`。
4. **Scope 是每次调用重建**。`ScriptNodeBase::evaluate` 每次构造全新 `Scope`，不跨调用保留状态。脚本间无隐式共享。
5. **注入变量名固定为 `payload`**。脚本作者依赖这个名字；改名是 breaking change。
6. **不负责节点身份**。`ScriptNodeBase` 持有 `id` 字段只是为了错误消息有上下文；`NodeTrait::kind()` 由嵌入节点用 `delegate_node_base!` 宏提供。

## 依赖约束

- 允许：`nazh-core`（含 `nazh_core::ai::AiService`）、`rhai`、`fastrand`、`serde_json`、`tokio`
- 禁止：`ai`（自 ADR-0019 起）、`connections`、`nodes-*`、协议 crate

ADR-0019 已实施（2026-04-26）：`AiService` trait 现住在 Ring 0，本 crate **不再**依赖 `ai`。`ai_complete()` 内建函数注入的是 `Arc<dyn nazh_core::ai::AiService>`，调用方（壳层）决定具体实现。

## 修改本 crate 时

| 改动 | 必须同步 |
|------|----------|
| 改 `ScriptNodeBase` 公共 API | 所有 `nodes-flow` 节点实现（`if_node` / `switch_node` / `loop_node` / `try_catch` / `code_node`） |
| 改默认 `max_operations` | 本文件"内部约定"；若默认收紧可能破坏既有工作流，考虑过 ADR |
| 加 Rhai 内建函数（`NazhScriptPackage`） | 本文件 + 前端脚本提示表（若前端提示已添加） |
| 改注入变量名 | 禁止，除非走 ADR 并考虑迁移脚本的方法 |

测试：
```bash
cargo test -p scripting
cargo test -p nodes-flow      # 脚本节点集成测试
```

## 关联 ADR / RFC

- **ADR-0002** Rhai 作为脚本引擎
- **ADR-0019** AI 能力依赖反转 — **已实施**（2026-04-26），本 crate 已脱离 `ai` 依赖
