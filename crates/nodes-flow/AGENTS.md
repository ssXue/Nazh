# crates/nodes-flow — 流程控制节点

> **Ring**: Ring 1
> **对外 crate 名**: `nodes-flow`
> **职责**: 工作流的流程控制类节点与子图桥接节点 — `if` / `switch` / `loop` / `tryCatch` / `code` / `subgraphInput` / `subgraphOutput`
>
> 根 `AGENTS.md` 的约束对本 crate 同样适用。

## 这个 crate 做什么

本 crate 实现 5 个基于脚本的流程控制节点，以及 2 个 ADR-0013 子图桥接节点，组成 `FlowPlugin` 插件：

| 节点 | 作用 | Dispatch |
|------|------|----------|
| `if` | 布尔分支 | `Route(["true"])` or `Route(["false"])` |
| `switch` | 多分支 | `Route([<matched branch>])` or `Route([<default>])` |
| `loop` | 循环展开 | 每个元素产出一条 `Route(["body"])`，末尾 `Route(["done"])` |
| `tryCatch` | 异常捕获 | `Route(["try"])` / `Route(["catch"])` |
| `code` | 任意 Rhai 脚本 | `Broadcast` |
| `subgraphInput` | 子图入口桥接 | `Broadcast` |
| `subgraphOutput` | 子图出口桥接 | `Broadcast` |

5 个脚本节点嵌入 `scripting::ScriptNodeBase`，组合式复用脚本执行。2 个子图桥接节点使用 `PassthroughNode`，只原样广播 payload。

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
├── code_node.rs      # CodeNode + CodeNodeConfig + CodeNodeAiConfig
└── passthrough.rs    # PassthroughNode，用于 subgraphInput/subgraphOutput
```

Plugin 注册入口：`FlowPlugin::register(&mut NodeRegistry)`，在 `lib.rs:28` 集中声明 7 个节点类型的工厂 + 能力标签。

## 内部约定

### 节点能力标签（ADR-0011）

| 节点 | 能力 | 原因 |
|------|------|------|
| `if` | `PURE \| BRANCHING` | 纯 Rhai 布尔表达式，路由到 true/false 端口 |
| `switch` | `PURE \| BRANCHING` | 纯 Rhai 匹配，路由到其中一个分支 |
| `loop` | `BRANCHING \| MULTI_OUTPUT` | 一次产出多条（每个元素一条 + done） |
| `tryCatch` | `BRANCHING` | 成功/失败分支 |
| `code` | `empty()` | 用户脚本可能含任意副作用（AI 调用、Rhai 自定义函数等）——无法静态保证 PURE |
| `subgraphInput` | `empty()` | 子图边界桥接节点，执行语义是 payload 透传；作为容器边界代理不参与调度优化 |
| `subgraphOutput` | `empty()` | 子图边界桥接节点，执行语义是 payload 透传；作为容器边界代理不参与调度优化 |

这张表是 crate 专属契约，由 facade crate 的 `src/registry.rs::标准注册表节点能力标签与_adr_0011_契约一致` 单测守住。改这张表**必须同时改测试**，否则 CI 会挂。

### 引脚声明（ADR-0010）

| 节点 | 输入 pin | 输出 pin |
|------|----------|----------|
| `code` | `in: Any`（默认） | `out: Any`（默认） |
| `if` | `in: Any`（默认） | `true: Any` / `false: Any` |
| `switch` | `in: Any`（默认） | **动态**：每个 `branches[i].key` 一个 `Any` 输出 + `default_branch`（去重） |
| `loop` | `in: Any`（默认） | `body: Any` / `done: Any` |
| `tryCatch` | `in: Any`（默认） | `try: Any` / `catch: Any` |
| `subgraphInput` | `in: Any`（默认） | `out: Any`（默认） |
| `subgraphOutput` | `in: Any`（默认） | `out: Any`（默认） |

**Phase 1 输入端均为默认 `Any`**——脚本节点天然吃任何 payload，没有理由收紧。输出端的具名 pin 与 `transform` 路径上 `NodeDispatch::Route([id])` 的字符串严格一致；改 pin id 必须同步改 transform，反之亦然。

**`switch` 的动态 pin 由 `output_pins(&self)` 实例方法在每次调用时读 `self.branches` + `self.default_branch` 生成**，避免把每个用户配置都注册成新类型。这是 ADR-0010 把 `output_pins` 设计为实例方法（而非 `'static` 表）的典型原因。

### 共同契约

1. **脚本节点都嵌入 `ScriptNodeBase`**。`NodeTrait` 元数据（`id` / `kind`）用 `scripting::delegate_node_base!` 宏委托。子图桥接节点使用 `PassthroughNode`，不执行脚本。**能力标签不走 trait**，而是在 `FlowPlugin::register` 时通过 `register_with_capabilities` 声明——详见 `crates/core/AGENTS.md` 的"为什么 `NodeTrait` 没有 `capabilities()` 方法"。
2. **脚本执行遵循 `scripting` crate 的约定**（`max_operations`、payload 变量名、Scope 单次使用）。
3. **不借用连接、不做 I/O**。`code` 节点可以通过 `ai_complete()` 发 AI 请求，但走的是 Ring 0 的 `nazh_core::ai::AiService` trait（具体实现由壳层注入）；本 crate 不直接用 `reqwest` / `rusqlite` / 其他协议，**也不依赖 `ai` crate**（ADR-0019）。
4. **分支端口名固定**：`if` 用 `"true"` / `"false"`，`tryCatch` 用 `"try"` / `"catch"`，`loop` 用 `"body"` / `"done"`，`switch` 用配置里声明的分支名。改端口名是前端画布的 breaking change。

## 依赖约束

- 允许：`nazh-core`（含 `nazh_core::ai::AiService`）、`scripting`、`async-trait`、`rhai`、`serde` / `serde_json`
- 禁止：`ai`（自 ADR-0019 起）、`connections`、`nodes-io`、任何协议 crate

本 crate 是 Ring 1 但避免了所有协议依赖——全部 I/O 都被挡在 `nodes-io`。ADR-0019 实施后，连 `ai` 也不再依赖：`code` 节点的 `Arc<dyn AiService>` 来自 Ring 0 trait + 壳层注入。

## 修改本 crate 时

| 改动 | 必须同步 |
|------|----------|
| 新增流程节点 | 本文件能力表 + `FlowPlugin::register` + `src/registry.rs` 契约测试 + `NODE_CATEGORY_MAP`（前端）+ ADR 若决策性 |
| 改节点能力标签 | 本文件能力表 + `src/registry.rs` 契约测试（两者不能分离） |
| 改节点 config schema | ts-rs 重新生成（若 `#[ts(export)]`） + 前端配置面板 |
| 改分支端口名 | 节点 `output_pins()` + `transform` 中的 `Route([...])` + 本文件 pin 表格 + 前端画布 + 所有示例工作流（风险高，尽量不做） |
| 给某节点收紧 pin 类型（如 `loop` 输入改成 `Array(Any)`） | 节点 `input_pins`/`output_pins` + 本文件 pin 表格 + 集成测试覆盖兼容/不兼容路径 |

测试：
```bash
cargo test -p nodes-flow
cargo test -p nazh-engine --test workflow   # 集成测试，覆盖分支+循环+异常路径
```

## 工作流变量集成（ADR-0012）

5 个脚本节点（`if` / `switch` / `loop` / `tryCatch` / `code`）的 `new()` 从工厂闭包接收 `variables: Option<Arc<WorkflowVariables>>`，传给 `ScriptNodeBase::new`。
工厂闭包在 `lib.rs::FlowPlugin::register()` 内 `res.get::<Arc<WorkflowVariables>>()` 提取（Task 5 的 deploy 注入到 SharedResources）。
脚本里通过 `vars.get/set/cas` 读写工作流声明的变量；详见 `crates/scripting/AGENTS.md` Rhai 全局对象节。

## 关联 ADR / RFC

- **ADR-0002** Rhai 脚本引擎（本 crate 的全部节点都依赖）
- **ADR-0008** 元数据通道（节点输出遵循此约定）
- **ADR-0011** 节点能力标签（能力分配见上表）
- **ADR-0010** Pin 声明系统（Phase 1：4 个分支节点已声明具体 output pin；输入端仍是默认 `Any`，详见引脚声明表）
- **ADR-0012** 工作流变量 — **已实施 Phase 1**（2026-04-27），5 节点工厂从 SharedResources 取 `Arc<WorkflowVariables>` 并注入 Rhai
- **ADR-0013** 子图与宏系统 — **已实施（子图核心）**（2026-04-28），本 crate 提供 `subgraphInput` / `subgraphOutput` passthrough 桥接节点
- **ADR-0019** AI 能力依赖反转 — 本 crate 已脱离 `ai` 依赖（2026-04-26）
