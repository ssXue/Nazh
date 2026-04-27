# crates/core — Nazh 引擎 Ring 0 内核

> **Ring**: Ring 0（内核层）
> **对外 crate 名**: `nazh-core`
> **职责**: 定义工作流运行时的最小类型集合与基础原语
>
> 根目录 `AGENTS.md` 的全部约束（Critical Coding Constraints / Design Principles /
> 语言与 git 约定）对本 crate **同样适用**；本文件只记录 crate 专属的设计与契约。

## 这个 crate 做什么

Ring 0 是 Nazh 分层内核的最内层（RFC-0002）。这里定义"引擎运行需要的最小抽象"：

- **执行**：`NodeTrait`、`NodeExecution`、`NodeOutput`、`NodeDispatch`、`NodeCapabilities`
- **调度**：`Plugin` / `PluginHost` / `NodeRegistry` / `RuntimeResources` / `SharedResources`
- **数据平面**：`DataStore` trait + `ArenaDataStore` 默认实现、`ContextRef`、`DataId`
- **控制平面**：`WorkflowContext`、`ExecutionEvent` 与 `CompletedExecutionEvent`
- **可靠性原语**：`EngineError`（统一错误）、`guard` 模块（panic/timeout 隔离）、`WorkflowNodeDefinition`（deploy-time 节点配置）

Ring 0 **不做**：具体节点实现、脚本引擎、协议驱动（HTTP / MQTT / Modbus / SQL）、
IPC 契约类型（由 `tauri-bindings` 承担，见 ADR-0017）、AI 协议实现（HTTP/SSE 客户端等由 `ai` crate 承担——但 `AiService` trait 与请求/响应类型自 ADR-0019 起住在 Ring 0 的 `ai` 模块）。

## 对外暴露

```text
crates/core/src/
├── lib.rs              # 公共 re-exports
├── ai.rs               # AiService trait + AiCompletionRequest/Response/...（ADR-0019）
├── context.rs          # WorkflowContext / ContextRef
├── data.rs             # DataStore trait + ArenaDataStore
├── error.rs            # EngineError
├── event.rs            # ExecutionEvent / CompletedExecutionEvent
├── guard.rs            # panic + timeout 隔离辅助
├── lifecycle.rs        # NodeLifecycleContext / NodeHandle / LifecycleGuard（ADR-0009）
├── node.rs             # NodeTrait / NodeCapabilities / NodeOutput
├── pin.rs              # PinDefinition / PinType / PinDirection（ADR-0010）
├── plugin.rs           # NodeRegistry / Plugin / RuntimeResources / WorkflowNodeDefinition
└── variables.rs        # WorkflowVariables / TypedVariable / TypedVariableSnapshot / VariableDeclaration（ADR-0012）
```

关键类型：
- `NodeTrait` — `src/node.rs`（`transform` + `on_deploy` 默认实现）
- `LifecycleGuard` / `NodeHandle` / `NodeLifecycleContext` — `src/lifecycle.rs`
- `sleep_or_cancel` (async) / `blocking_sleep_or_cancel` (sync) — `src/lifecycle.rs`，`on_deploy` 后台任务用的退避 sleep
- `NodeCapabilities` bitflags — `src/node.rs`
- `PinDefinition` / `PinType` / `PinDirection` — `src/pin.rs`（ADR-0010）
- `NodeRegistry::{register_with_capabilities, capabilities_of}` — `src/plugin.rs`
- `DataStore` trait — `src/data.rs`
- `ExecutionEvent` — `src/event.rs`
- `WorkflowNodeDefinition` — `src/plugin.rs`
- `CancellationToken` re-export from `tokio_util::sync`
- `AiService` trait + 请求/响应/错误类型 — `src/ai.rs`（ADR-0019 上移到 Ring 0；具体实现 `OpenAiCompatibleService` 仍在 `ai` crate）
- `WorkflowVariables` / `TypedVariable` / `TypedVariableSnapshot` / `VariableDeclaration` — `src/variables.rs`（ADR-0012；DashMap 后端、类型化写入、CAS、部署期 `from_declarations` 校验）

## 内部约定（本 crate 的契约）

以下约定只约束 Ring 0 以及实现 Ring 0 trait 的下游 crate。与根 `AGENTS.md` 的通用约束是**叠加**关系。

### 节点契约（`NodeTrait`）

1. **`transform(trace_id, payload) → NodeExecution` 是核心数据路径**。节点做 `(trace_id, payload) → (payload, metadata)` 的纯变换，不触碰 `DataStore`。Runner 负责读写 store。
2. **`on_deploy(ctx) → LifecycleGuard` 是触发器/长连接路径**（ADR-0009）。默认实现返回 `LifecycleGuard::noop()`——纯变换节点无需重写。触发器节点（MQTT 订阅 / Timer / Serial 监听）在 `on_deploy` 中 spawn 后台任务、通过 `ctx.handle.emit(payload, metadata)` 推数据进 DAG，监听 `ctx.shutdown` token 在撤销时退出。返回的 `LifecycleGuard` 由 Runner 持有，撤销时按逆部署序 `shutdown().await`。
3. **元数据走事件通道，不进 payload**（ADR-0008）。两条路径（`transform` 与 `NodeHandle::emit`）都遵守此约束——`with_metadata()` / `emit(payload, metadata)` 中的 metadata 通过 `ExecutionEvent::Completed` 独立传递。
4. **Panic 隔离由 Runner 负责**。节点可以 panic，但 Runner 会用 `AssertUnwindSafe + catch_unwind + timeout` 包裹；节点内部不必自行 catch_unwind。

### 节点能力标签契约（`NodeCapabilities`，ADR-0011）

标签属于**类型级别**的契约——同类型的所有实例、所有 config 组合都必须满足。若某能力只在特定 config 下成立（如 `mqttClient` 仅在 `subscribe` 模式才是触发器），**不要**在类型级别声明，保守空着。

**位分配**（锁死于 `node::tests::node_capabilities_位分配与_adr_0011_一致`）：

| 位 | 名字 | 含义速记 |
|---|------|----------|
| `0b0000_0001` | `PURE` | 同输入必得同输出 |
| `0b0000_0010` | `NETWORK_IO` | HTTP / MQTT / Kafka |
| `0b0000_0100` | `FILE_IO` | sqlite / 本地文件 |
| `0b0000_1000` | `DEVICE_IO` | Modbus / 串口 / OPC-UA |
| `0b0001_0000` | `TRIGGER` | 由外部时钟/事件驱动 |
| `0b0010_0000` | `BRANCHING` | `NodeDispatch::Route` |
| `0b0100_0000` | `MULTI_OUTPUT` | 一次 transform 出多条 |
| `0b1000_0000` | `BLOCKING` | 需 Runner 包 `spawn_blocking` |

每个位的**契约 / 反例 / 消费者**细节见 `src/node.rs:26` 的 rustdoc；那里是语义 source of truth。

**内置节点的标签对应表**见 `crates/nodes-flow/AGENTS.md` 与 `crates/nodes-io/AGENTS.md`。这张表由 `src/registry.rs` 的契约单测守住，不要跳过测试直接改代码。

### 为什么 `NodeTrait` **没有** `capabilities()` 方法（有意缺席）

能力只存在于**注册表**（类型级别，由 `register_with_capabilities` 声明），**没有**对应的 `NodeTrait::capabilities()` 方法——这是深思熟虑的取舍，不是遗漏。

**反面教训**：ADR-0011 首次实施时两处都加了——trait 默认方法 + 注册表参数，11 个节点都覆盖了 `fn capabilities(&self)`。后来 code review 发现：

- trait 方法**零消费者**（所有消费者走 `registry.capabilities_of(kind)`），11 个 override 全是类型级值的复读
- 两处声明 = 双倍维护成本 + 漂移风险 + 给读者"哪个才是真的？"的困惑
- 所以砍掉了 trait 方法和所有 override。

**未来若真需要实例级能力精化**（典型场景：`mqttClient` 按 `subscribe`/`publish` 返回不同 bits，或 `code` 节点根据脚本分析结果动态声明 PURE），**不要**恢复 `fn capabilities(&self) -> NodeCapabilities`。推荐加新方法：

```rust
// 签名表达"在类型级基础上精化"的意图
fn instance_capabilities(&self, type_caps: NodeCapabilities) -> NodeCapabilities {
    type_caps   // 默认：实例=类型，无精化
}
```

消费者显式传入 `registry.capabilities_of(node.kind())` 再问节点"你要精化吗"，语义清晰：
- 类型级 caps 在注册表，单一事实源
- 实例级精化是显式 opt-in，不覆盖就等于"和类型级相同"

目前 (2026-04-24) 没有这个需求，所以**不要预留**。YAGNI。

### 引脚声明系统（`PinDefinition` / `PinType`，ADR-0010）

引脚是**实例级**契约——与类型级的 `NodeCapabilities` 互补，不要互相替代。`switch` 节点的输出引脚由 `branches` config 决定，必须读 `&self` 才能给出，因此 [`NodeTrait::output_pins`] 是实例方法不是 `'static` 表。

**默认行为**：trait 默认实现把节点声明为单 `Any` 输入 + 单 `Any` 输出。存量节点不重写就能通过部署期校验，渐进式迁移到具体 pin。

**Phase 1 已迁移**：`if` / `switch` / `loop` / `tryCatch` 四个分支节点声明真实 pin（详见 `crates/nodes-flow/AGENTS.md`）；其余 14 个节点仍是默认 Any/Any。

**`PinType` 兼容矩阵**（`is_compatible_with` 实现，对应 ADR-0010 决策章节）：

| 上游 \ 下游 | `Any` | 同名标量 | `Array(T)` | `Json` | `Custom(s)` |
|------------|-------|---------|------------|--------|-------------|
| `Any` | ✓ | ✓ | ✓ | ✓ | ✓ |
| 标量 `T` | ✓ | ✓ if T==U | ✗ | ✗ | ✗ |
| `Array(T)` | ✓ | ✗ | 递归判内层 | ✗ | ✗ |
| `Json` | ✓ | ✗ | ✗ | ✓ | ✗ |
| `Custom(s)` | ✓ | ✗ | ✗ | ✗ | ✓ if s==t |

**`PinType::Json` 不带 schema payload**（Phase 1 决策）。结构校验留待未来独立 ADR 评估 `schemars` / `jsonschema` 引入；当前 `Json` 仅声明"该端口流通的是任意 JSON"。

**新增 `PinType::Custom("xxx")` 类型时的 review checklist：**
1. 名字唯一吗？协议级类型用 `protocol-thing` 形式（如 `modbus-register`、`opc-tag`），避免与未来标量冲突
2. 跨 crate 共用吗？若是协议级 + 多节点共享，考虑是否升格为 `PinType` 一等成员（需新 ADR）
3. 配套节点 pin 声明都同步收紧了吗？只在某节点的 output 声明 `Custom("foo")` 但下游 input 仍写 `Any` 等于没用

### 注册表契约（`NodeRegistry`）

1. **Ring 0 无硬编码节点**。`NodeRegistry` 只是工厂 + 能力 map 的壳，全部节点由 Ring 1 的 `Plugin::register()` 注入。facade 的 `standard_registry()` 是组合策略，不属于 Ring 0。
2. **注册时必须声明能力标签**。所有节点统一走 `register_with_capabilities`；确实没有特殊能力时显式传 `NodeCapabilities::empty()`。
3. **`capabilities_of()` 的返回值语义**：`None` = 未注册；`Some(empty())` = 注册了但显式声明空集合。不要把二者混为一谈。

### 数据平面契约（`DataStore` / `ContextRef`）

1. **节点不直接接触 `DataStore`**。Runner 负责读写；节点只看到 `payload: Value`。
2. **`ContextRef` 是轻量指针（≈64 字节）**，payload 存在 `DataStore` 里，MPSC 只传 `ContextRef`。
3. **默认存储是 `ArenaDataStore`**（DashMap + `Arc<Value>`）。其他后端（分级/持久化）以后通过实现 `DataStore` trait 在 Ring 1 提供，本 crate 不关心。

### 错误与事件契约

1. **所有错误统一走 `EngineError`**（`thiserror`），不允许 `.unwrap()` / `.expect()` / `panic!()`（测试除外）。
2. **`ExecutionEvent::Completed` 携带 `metadata: Option<Map>`**。所有协议/执行元数据用此字段；Failed/Started/Output 不承载业务 payload。

## 依赖约束

**允许**的依赖：`tokio`、`serde`、`serde_json`、`thiserror`、`chrono`、`dashmap`、`async-trait`、`futures-util`、`tracing`、`uuid`、`bitflags`。

**禁止**的依赖：
- **协议 crate**（`reqwest`、`rumqttc`、`rusqlite`、`tokio-modbus` 等）——它们属于 Ring 1。
- **`ts-rs` 作为硬依赖**——`ts-rs` 只能 `optional = true` 且由 `ts-export` feature 门控（ADR-0017）。生产编译绝不携带 `ts-rs`。
- **任何工作区内的 crate**——Ring 0 不依赖 Ring 1。单向箭头。

新增依赖前自问：Ring 0 真的需要吗？不能靠下游 crate 通过 trait 注入解决吗？如有疑问，先开 ADR。

## 修改本 crate 时

以下动作需要同步更新对应位置：

| 改动 | 必须同步 |
|------|----------|
| 改 `NodeTrait` 签名 | 所有 Ring 1 `NodeTrait` 实现 + `tests/workflow.rs` + 根 AGENTS.md 的 NodeTrait 章节 |
| 改 `NodeCapabilities` 位值或新增位 | 本 crate 的位分配单测 + `src/node.rs` 的 rustdoc + `web/src/lib/node-capabilities.ts` 前端常量表 + `src/registry.rs` 契约测试 + ADR-0011 的实施记录表 |
| 给 `PinType` 加新变体 | 本 crate `pin.rs` 兼容矩阵单测 + `crates/nodes-flow/AGENTS.md` pin 表格（如有相关节点）+ ADR-0010 实施记录 |
| 改 `ExecutionEvent` / `NodeOutput` 结构 | `web/src/generated/` 重新生成（`cargo test -p tauri-bindings --features ts-export export_bindings`）+ 前端事件解析器 |
| 改 `WorkflowNodeDefinition` 字段 | ts-rs 重新生成 + `src/graph/` 的部署路径 + 前端图解析 |
| 改 `NodeRegistry` 公共 API | 所有 `Plugin::register` 调用点（至少 `nodes-flow` / `nodes-io`）+ `tauri-bindings::list_node_types_response` |
| 改 `WorkflowVariables` 公共 API | `crates/scripting/src/lib.rs`（Rhai 注入点）+ `src/graph/variables_init.rs`（构造调用）+ IPC `snapshot_workflow_variables`（如已实施）+ ts-rs 重新生成（`TypedVariableSnapshot` / `VariableDeclaration` 带 `ts-export`）+ ADR-0012 |
| 新增 Ring 0 依赖 | 先过依赖约束 checklist；必要时开 ADR |

测试指令：
```bash
cargo test -p nazh-core                                         # 本 crate 单元测试
cargo test -p tauri-bindings --features ts-export export_bindings   # 若改了带 ts-export 的类型
```

## 关联 ADR / RFC

- **RFC-0002** 分层内核与插件架构（本 crate 是 Ring 0）
- **ADR-0001** Tokio MPSC DAG 调度（决定了 `NodeTrait` 的 async 取向）
- **ADR-0004** 统一执行事件模型（定义了 `ExecutionEvent`）
- **ADR-0006** 节点注册表演进方向（`NodeRegistry` 的设计）
- **ADR-0008** 节点输出元数据通道（payload/metadata 分离的根源）
- **ADR-0009** 节点生命周期钩子（`on_deploy` + `LifecycleGuard` + `NodeHandle` 来源）
- **ADR-0010** Pin 声明系统（`PinDefinition` / `PinType` / `PinDirection` 的来源）
- **ADR-0011** 节点能力标签（`NodeCapabilities` 的来源）
- **ADR-0012** 工作流变量（`WorkflowVariables` / `TypedVariable` / `TypedVariableSnapshot` / `VariableDeclaration` 的来源）
- **ADR-0017** IPC + ts-rs 迁出 Ring 0（本 crate 依赖约束的直接触发）
