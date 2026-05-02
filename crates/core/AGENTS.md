# crates/core — Nazh 引擎 Ring 0 内核

> **Ring**: Ring 0（内核层）
> **对外 crate 名**: `nazh-core`
> **职责**: 定义工作流运行时的最小类型集合与基础原语
>
> 根目录 `AGENTS.md` 的全部约束（Critical Coding Constraints / Design Principles /
> 语言与 git 约定）对本 crate **同样适用**；本文件只记录 crate 专属的设计与契约。

## 定位

Ring 0 是 Nazh 分层内核的最内层（RFC-0002）。这里定义"引擎运行需要的最小抽象"：

- **执行**：`NodeTrait`、`NodeExecution`、`NodeOutput`、`NodeDispatch`、`NodeCapabilities`
- **调度**：`Plugin` / `PluginHost` / `NodeRegistry` / `RuntimeResources` / `SharedResources`
- **数据平面**：`DataStore` trait + `ArenaDataStore` 默认实现、`ContextRef`、`DataId`
- **控制平面**：`WorkflowContext`、`ExecutionEvent` 与 `CompletedExecutionEvent`
- **引脚系统**：`PinDefinition` / `PinType` / `PinDirection` / `PinKind` / `EmptyPolicy`（ADR-0010 / ADR-0014）
- **输出缓存**：`OutputCache` / `CachedOutput`（ADR-0014 Data 引脚缓存槽）
- **工作流变量**：`WorkflowVariables` / `TypedVariable` / `TypedVariableSnapshot` / `VariableDeclaration`（ADR-0012）
- **AI 服务**：`AiService` trait + 请求/响应/错误类型（ADR-0019，上移到 Ring 0）
- **可靠性原语**：`EngineError`（统一错误）、`guard` 模块（panic/timeout 隔离）、`WorkflowNodeDefinition`（deploy-time 节点配置）
- **生命周期**：`NodeHandle` / `LifecycleGuard` / `NodeLifecycleContext`（ADR-0009）

Ring 0 **不做**：具体节点实现、脚本引擎、协议驱动（HTTP / MQTT / Modbus / SQL）、IPC 契约类型（由 `tauri-bindings` 承担，见 ADR-0017）、AI 协议实现（HTTP/SSE 客户端等由 `ai` crate 承担）。

## 对外暴露

```text
crates/core/src/
├── lib.rs              # 公共 re-exports + ts-rs export_bindings 入口
├── ai.rs               # AiService trait + AiCompletionRequest/Response/...（ADR-0019）
├── cache.rs            # OutputCache / CachedOutput（ADR-0014 Data 引脚缓存槽）
├── context.rs          # WorkflowContext / ContextRef（含 source_node 字段）
├── data.rs             # DataStore trait + ArenaDataStore
├── error.rs            # EngineError（含 Pin/Variable/Data 引脚相关变体）
├── event.rs            # ExecutionEvent / CompletedExecutionEvent / EdgeTransmitSummary / BackpressureDetected
├── guard.rs            # panic + timeout 隔离辅助（guarded_execute）
├── lifecycle.rs        # NodeLifecycleContext / NodeHandle / LifecycleGuard（ADR-0009）
├── node.rs             # NodeTrait / NodeCapabilities / NodeOutput / is_pure_form / into_payload_map
├── pin.rs              # PinDefinition / PinType / PinDirection / PinKind / EmptyPolicy（ADR-0010 + ADR-0014）
├── plugin.rs           # NodeRegistry / Plugin / PluginHost / RuntimeResources / WorkflowNodeDefinition
└── variables.rs        # WorkflowVariables / TypedVariable / TypedVariableSnapshot / VariableDeclaration（ADR-0012）
```

关键类型：
- `NodeTrait` — `src/node.rs`（`transform` + `on_deploy` + `input_pins` + `output_pins` 默认实现）
- `LifecycleGuard` / `NodeHandle` / `NodeLifecycleContext` — `src/lifecycle.rs`
- `sleep_or_cancel` / `blocking_sleep_or_cancel` — `src/lifecycle.rs`
- `NodeCapabilities` bitflags — `src/node.rs`（8 位，位分配锁死于单测）
- `PinDefinition` / `PinType` / `PinKind` / `EmptyPolicy` — `src/pin.rs`
- `OutputCache` / `CachedOutput` — `src/cache.rs`（Data 引脚缓存槽，watch channel 后端）
- `NodeRegistry::{register_with_capabilities, capabilities_of}` — `src/plugin.rs`
- `DataStore` trait + `ArenaDataStore` — `src/data.rs`
- `ExecutionEvent` — `src/event.rs`（Started / Completed / Failed / Output / Finished / VariableChanged / VariableDeleted / EdgeTransmitSummary / BackpressureDetected）
- `WorkflowNodeDefinition` — `src/plugin.rs`（字段私有，getter 访问，`probe` 工厂方法）
- `CancellationToken` re-export from `tokio_util::sync`
- `AiService` trait + 请求/响应/错误类型 — `src/ai.rs`
- `WorkflowVariables` / `TypedVariable` / `TypedVariableSnapshot` / `VariableDeclaration` — `src/variables.rs`
  - `set_event_sender` — 注入事件通道（OnceCell 仅设一次）
  - `subscribe(name)` — 变更通知 watch receiver（ADR-0015 Phase 2）
  - `reset(name, updated_by)` — 恢复到声明初值（ADR-0012 Phase 3）
  - `remove(name)` — 移除变量并通知订阅者（ADR-0012 Phase 3）
- `is_pure_form(node)` — 判断节点是否为 pure-form（全 Data 引脚，不参与 Exec 触发链）
- `into_payload_map(payload)` — JSON payload → Map 包装
- `impl_node_meta!` — 为持有 `id` 字段的节点生成 `id()` / `kind()` 方法
- `emit_event` / `emit_failure` — `src/event.rs`，`try_send` 非阻塞事件发送

## 内部约定

### 节点契约（`NodeTrait`）

1. **`transform(trace_id, payload) → NodeExecution`** 是核心数据路径。节点做 `(trace_id, payload) → (payload, metadata)` 的纯变换，不触碰 `DataStore`。
2. **`on_deploy(ctx) → LifecycleGuard`** 是触发器/长连接路径（ADR-0009）。默认 `LifecycleGuard::noop()`。
3. **`input_pins(&self)` / `output_pins(&self)`** 声明引脚（ADR-0010）。默认单 `Any` 输入 + 单 `Any` 输出。
4. **元数据走事件通道，不进 payload**（ADR-0008）。`transform` 与 `NodeHandle::emit` 均遵守。
5. **Panic 隔离由 Runner 负责**——`guarded_execute`（`src/guard.rs`）用 `AssertUnwindSafe + catch_unwind + timeout`。

### 节点能力标签（`NodeCapabilities`，ADR-0011）

标签属于**类型级别**契约——同类型所有实例、所有 config 组合都必须满足。

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

位分配由 `node_capabilities_位分配与_adr_0011_一致` 单测锁死。**`NodeTrait` 没有 `capabilities()` 方法**——能力只存在于注册表。详见 `src/node.rs` rustdoc。

### 引脚系统（ADR-0010 + ADR-0014）

- `PinType` 是数据形状（Any / Bool / Integer / Float / String / Json / Binary / Array / Custom）
- `PinKind` 是求值语义（Exec / Data / Reactive），正交于 `PinType`
- `EmptyPolicy` 是 Data 输入引脚缓存空时的兜底策略（BlockUntilReady / DefaultValue / Skip）
- 引脚声明是**实例级**的（`&self` 方法），与类型级的 `NodeCapabilities` 互补

### 数据平面（`DataStore` / `ContextRef` / `OutputCache`）

1. 节点不直接接触 `DataStore`。Runner 负责读写。
2. `ContextRef` 是轻量指针（~64 字节 + `source_node`），payload 存在 `DataStore`。
3. `OutputCache` 是 Data 引脚的缓存槽（`watch` channel 后端），Runner 写、下游拉取或订阅。

### 错误与事件

1. 所有错误统一走 `EngineError`（`thiserror`）。
2. `ExecutionEvent::Completed` 携带 `metadata: Option<Map>`。
3. `ExecutionEvent::VariableChanged` — ADR-0012 Phase 2，write-on-change。
4. `ExecutionEvent::VariableDeleted` — ADR-0012 Phase 3。
5. `ExecutionEvent::EdgeTransmitSummary` / `BackpressureDetected` — ADR-0016。
6. `emit_event` 使用 `try_send`，通道满/关闭时 `tracing::error!`。

## 依赖约束

**允许**：`tokio`、`tokio-util`、`serde`、`serde_json`、`thiserror`、`chrono`、`dashmap`、`async-trait`、`futures-util`、`tracing`、`uuid`、`bitflags`。

**禁止**：
- 协议 crate（`reqwest`、`rumqttc`、`rusqlite`、`tokio-modbus` 等）——属于 Ring 1。
- `ts-rs` 作为硬依赖——只能 `optional = true` 且由 `ts-export` feature 门控（ADR-0017）。
- 任何工作区内的 crate——Ring 0 不依赖 Ring 1。

## 修改本 crate 时

| 改动 | 必须同步 |
|------|----------|
| 改 `NodeTrait` 签名 | 所有 Ring 1 `NodeTrait` 实现 + `tests/workflow.rs` + 根 AGENTS.md |
| 改 `NodeCapabilities` 位值或新增位 | 位分配单测 + `src/node.rs` rustdoc + `web/src/lib/node-capabilities.ts` + `src/registry.rs` 契约测试 + ADR-0011 |
| 给 `PinType` / `PinKind` 加新变体 | 兼容矩阵单测 + 对应 crate AGENTS.md + ADR 记录 |
| 改 `ExecutionEvent` / `NodeOutput` 结构 | ts-rs 重新生成 + 前端事件解析器；若涉及 `VariableChanged/VariableDeleted`，同步 `crates/tauri-bindings` |
| 改 `WorkflowNodeDefinition` 字段 | ts-rs 重新生成 + `crates/graph/` 部署路径 + 前端图解析 |
| 改 `NodeRegistry` 公共 API | 所有 `Plugin::register` 调用点 + `tauri-bindings::list_node_types_response` |
| 改 `WorkflowVariables` 公共 API | `crates/scripting/src/lib.rs` + `crates/graph/variables_init.rs` + IPC 层 + ts-rs 重新生成 + ADR-0012 |
| 改 `OutputCache` 公共 API | `crates/graph/` Runner 中 Data 引脚写入/拉取路径 + ADR-0014 |
| 新增 Ring 0 依赖 | 先过依赖约束 checklist；必要时开 ADR |

测试：
```bash
cargo test -p nazh-core                                         # 本 crate 单元测试
cargo test -p tauri-bindings --features ts-export export_bindings   # 若改了带 ts-export 的类型
```

## 关联 ADR / RFC

- **RFC-0002** 分层内核与插件架构
- **ADR-0001** Tokio MPSC DAG 调度
- **ADR-0004** 统一执行事件模型
- **ADR-0006** 节点注册表演进
- **ADR-0008** 节点输出元数据通道（payload/metadata 分离）
- **ADR-0009** 节点生命周期钩子
- **ADR-0010** Pin 声明系统
- **ADR-0011** 节点能力标签
- **ADR-0012** 工作流变量
- **ADR-0014** 引脚求值语义二分（PinKind + OutputCache + EmptyPolicy）
- **ADR-0015** 反应式数据引脚（PinKind::Reactive + watch channel）
- **ADR-0016** 边级可观测性（EdgeTransmitSummary / BackpressureDetected）
- **ADR-0017** IPC + ts-rs 迁出 Ring 0
- **ADR-0019** AI 能力依赖反转（AiService trait 上移）
