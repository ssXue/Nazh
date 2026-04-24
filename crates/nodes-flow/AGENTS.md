# crates/nodes-flow — 流程控制节点

> **Ring**: Ring 1
> **对外 crate 名**: `nodes-flow`
> **职责**: 工作流的流程控制类节点 — `if` / `switch` / `loop` / `tryCatch` / `code`
>
> 根 `AGENTS.md` 的约束对本 crate 同样适用。

## 这个 crate 做什么

本 crate 实现 5 个基于脚本的流程控制节点，组成 `FlowPlugin` 插件：

| 节点 | 作用 | Dispatch |
|------|------|----------|
| `if` | 布尔分支 | `Route(["true"])` or `Route(["false"])` |
| `switch` | 多分支 | `Route([<matched branch>])` or `Route([<default>])` |
| `loop` | 循环展开 | 每个元素产出一条 `Route(["body"])`，末尾 `Route(["done"])` |
| `tryCatch` | 异常捕获 | `Route(["try"])` / `Route(["catch"])` |
| `code` | 任意 Rhai 脚本 | `Broadcast` |

每个节点都嵌入 `scripting::ScriptNodeBase`，组合式复用脚本执行。

**`code` 节点的 AI 能力**：`code` 节点在 `AiGenerationParams` 配置下可调用 `ai_complete()`
生成脚本内容，或把 AI 的回答作为 payload 的一部分——这是 Nazh "AI 作为一等公民"设计的入口之一。

## 对外暴露

```text
crates/nodes-flow/src/
├── lib.rs            # FlowPlugin + re-exports
├── if_node.rs        # IfNode + IfNodeConfig
├── switch_node.rs    # SwitchNode + SwitchNodeConfig + SwitchBranchConfig
├── loop_node.rs      # LoopNode + LoopNodeConfig
├── try_catch.rs      # TryCatchNode + TryCatchNodeConfig
└── code_node.rs      # CodeNode + CodeNodeConfig + CodeNodeAiConfig
```

Plugin 注册入口：`FlowPlugin::register(&mut NodeRegistry)`，在 `lib.rs:28` 集中声明 5 个节点类型的工厂 + 能力标签。

## 内部约定

### 节点能力标签（ADR-0011）

| 节点 | 能力 | 原因 |
|------|------|------|
| `if` | `PURE \| BRANCHING` | 纯 Rhai 布尔表达式，路由到 true/false 端口 |
| `switch` | `PURE \| BRANCHING` | 纯 Rhai 匹配，路由到其中一个分支 |
| `loop` | `BRANCHING \| MULTI_OUTPUT` | 一次产出多条（每个元素一条 + done） |
| `tryCatch` | `BRANCHING` | 成功/失败分支 |
| `code` | `empty()` | 用户脚本可能含任意副作用（AI 调用、Rhai 自定义函数等）——无法静态保证 PURE |

这张表是 crate 专属契约，由 facade crate 的 `src/registry.rs::标准注册表节点能力标签与_adr_0011_契约一致` 单测守住。改这张表**必须同时改测试**，否则 CI 会挂。

### 共同契约

1. **所有节点都嵌入 `ScriptNodeBase`**。`NodeTrait` 元数据（`id` / `kind`）用 `scripting::delegate_node_base!` 宏委托。**能力标签不走 trait**，而是在 `FlowPlugin::register` 时通过 `register_with_capabilities` 声明——详见 `crates/core/AGENTS.md` 的"为什么 `NodeTrait` 没有 `capabilities()` 方法"。
2. **脚本执行遵循 `scripting` crate 的约定**（`max_operations`、payload 变量名、Scope 单次使用）。
3. **不借用连接、不做 I/O**。`code` 节点可以通过 `ai_complete()` 发 AI 请求，但这是经过 `ai` crate 的；本 crate 不直接用 `reqwest` / `rusqlite` / 其他协议。
4. **分支端口名固定**：`if` 用 `"true"` / `"false"`，`tryCatch` 用 `"try"` / `"catch"`，`loop` 用 `"body"` / `"done"`，`switch` 用配置里声明的分支名。改端口名是前端画布的 breaking change。

## 依赖约束

- 允许：`nazh-core`、`scripting`、`ai`（给 `code` 节点用）
- 禁止：`connections`、`nodes-io`、任何协议 crate

本 crate 是 Ring 1 但避免了协议依赖——全部 I/O 都被挡在 `nodes-io`。这让它在嵌入式场景下可以
单独编译（未来可能的 ADR-0018 feature 门控）。

## 修改本 crate 时

| 改动 | 必须同步 |
|------|----------|
| 新增流程节点 | 本文件能力表 + `FlowPlugin::register` + `src/registry.rs` 契约测试 + `NODE_CATEGORY_MAP`（前端）+ ADR 若决策性 |
| 改节点能力标签 | 本文件能力表 + `src/registry.rs` 契约测试（两者不能分离） |
| 改节点 config schema | ts-rs 重新生成（若 `#[ts(export)]`） + 前端配置面板 |
| 改分支端口名 | 前端画布 + 所有示例工作流（风险高，尽量不做） |

测试：
```bash
cargo test -p nodes-flow
cargo test -p nazh-engine --test workflow   # 集成测试，覆盖分支+循环+异常路径
```

## 关联 ADR / RFC

- **ADR-0002** Rhai 脚本引擎（本 crate 的全部节点都依赖）
- **ADR-0008** 元数据通道（节点输出遵循此约定）
- **ADR-0011** 节点能力标签（能力分配见上表）
- **（待）ADR-0010** Pin 声明系统（未来会让分支/循环的端口声明更强类型）
