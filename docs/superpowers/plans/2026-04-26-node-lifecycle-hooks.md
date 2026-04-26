# ADR-0009 节点生命周期钩子实施计划

> **Status:** 启动中（2026-04-26 立项，对应 ADR-0009 状态升级到"已接受"）
> 实施完成后请同步：ADR-0009 → "已实施"、`docs/adr/README.md` 索引、`AGENTS.md` Project Status 章节、`AGENTS.md` ADR Execution Order 表。

**Goal:** 在 `NodeTrait` 上引入 `on_deploy` 钩子与 `LifecycleGuard` RAII 清理，把 Timer / Serial / MQTT 三类长连接根触发任务从 Tauri 壳层（`src-tauri/src/lib.rs:2484-3110` 约 626 行）迁回 Ring 1 节点实现，使壳层回归"IPC 桥接 + UI 集成"职责。

**Architecture:**
- **Ring 0 (`crates/core`)** 引入 `LifecycleGuard` / `NodeHandle` / `NodeLifecycleContext` 三个类型 + `NodeTrait::on_deploy` 默认空实现。新增 `tokio-util` 依赖（`CancellationToken`）。
- **Runner (`src/graph/deploy.rs` + `src/graph/types.rs`)** 在 spawn `run_node` 之前按**拓扑序**调用 `on_deploy`；`WorkflowDeployment` 持有 `Vec<LifecycleGuard>`，撤销时按**逆拓扑序** `shutdown().await`，未显式 shutdown 的依赖 `Drop` 兜底。
- **迁移顺序**（ADR-0009 备注章节明确建议）：Timer（最简单，无外部连接） → Serial（本地 I/O） → MQTT（网络连接 + QoS）。每迁完一类做一次 E2E 验证再做下一类。
- **取消语义**：壳层旧代码用 `Arc<AtomicBool>` + `sleep_with_cancel`；新引擎路径全部统一到 `tokio_util::sync::CancellationToken`。迁移完成后才能删除 `sleep_with_cancel`。

**Tech Stack:** Rust 2024、`tokio_util::sync::CancellationToken`、Tokio MPSC、`async-trait`、`AssertUnwindSafe + catch_unwind + timeout`

> **范围扩展（2026-04-26 偏差评估补充）**：原 plan 起草时仅考虑"trait + Runner + 三类节点迁移 + 删壳层代持函数"。结合 `2026-04-25-cargo-clippy-workspace-fixes.md` 的实施偏差评估，发现壳层为三家代持服务**专门发明了一整套抽象**（`*RootSpec` 数据 struct / `DesktopTriggerTask` / `TriggerJoinHandle` / `DesktopWorkflow.trigger_tasks` / `abort_triggers()` / `sleep_with_cancel`），ADR-0009 实施时这些**必须一并清掉**才算干净。本计划 Task 5 因此扩展为 8 步，覆盖：
> 1. 删除 3 个 `*RootSpec` 数据 struct（lib.rs:736-757）
> 2. 删除 `DesktopTriggerTask` / `TriggerJoinHandle` 抽象（lib.rs:561-569）
> 3. 改造 `DesktopWorkflow` 持有 `Option<WorkflowDeployment>`（lib.rs:548-619）
> 4. `abort_triggers` → `shutdown_runtime` 改造 + IPC 契约 `UndeployResponse.aborted_timer_count` 决策（推荐保留字段名）
> 5-8. 原计划范围 + clippy 收支表验证
>
> 同时 Task 4（MQTT）补一步"复刻 `collect_mqtt_root_specs` 的连接元数据校验"，避免 broker 元数据回退或健康度统计漂移。

---

## 当前壳层债务清单（ADR-0009 背景章节）

| 节点类型 | 壳层代持函数 | 文件:行号 | 行数 |
|----------|-------------|-----------|------|
| `timer` | `collect_timer_root_specs` | `src-tauri/src/lib.rs:2376` | ~28 |
| `timer` | `spawn_timer_root_tasks` | `src-tauri/src/lib.rs:2484` | ~46 |
| `serial`（监听） | `collect_serial_root_specs` | `src-tauri/src/lib.rs:2415` | ~69 |
| `serial`（监听） | `spawn_serial_root_tasks` | `src-tauri/src/lib.rs:2530` | ~37 |
| `serial`（监听） | `run_serial_root_reader` | `src-tauri/src/lib.rs:2914` | ~196 |
| `serial`（监听） | `flush_idle_serial_frame` / `drain_serial_delimited_frame` / `submit_serial_frame` | `src-tauri/src/lib.rs:3110-3174` | ~64 |
| `mqttClient`（订阅） | `collect_mqtt_root_specs` | `src-tauri/src/lib.rs:2569` | ~96 |
| `mqttClient`（订阅） | `spawn_mqtt_root_tasks` | `src-tauri/src/lib.rs:2667` | ~40 |
| `mqttClient`（订阅） | `run_mqtt_root_subscriber` | `src-tauri/src/lib.rs:2706` | ~207 |
| 共享 | `emit_serial_trigger_failure` / `emit_mqtt_trigger_failure` / `emit_trigger_failure` | `src-tauri/src/lib.rs:3175-3240` | ~65 |
| 共享 | `sleep_with_cancel` | `src-tauri/src/lib.rs:3258` | ~12 |
| 抽象 | `DesktopTriggerTask` / `TriggerJoinHandle` | `src-tauri/src/lib.rs`（搜索定义） | ~30 |

完整迁完后预计删除约 600+ 行壳层代码。

---

## Task 0: 在 Ring 0 引入 LifecycleGuard / NodeHandle / NodeLifecycleContext 骨架

**Files:**
- Modify: `crates/core/Cargo.toml`（加 `tokio-util` 依赖）
- Modify: `crates/core/src/lib.rs`（pub use 新类型）
- Modify: `crates/core/src/node.rs`（NodeTrait 加 `on_deploy` 默认实现）
- Create: `crates/core/src/lifecycle.rs`（新模块）
- Modify: `crates/core/AGENTS.md`（同步契约说明）

- [ ] **Step 1: 加 tokio-util 依赖**

```toml
# crates/core/Cargo.toml
[dependencies]
tokio-util = { version = "0.7", default-features = false, features = ["sync"] }
```

注意 Ring 0 严格依赖纪律：只开 `sync` feature 拿 `CancellationToken`，不开 `io` / `net` / `compat` 之类拖入 reqwest/hyper 的特性。

- [ ] **Step 2: 创建 lifecycle 模块**

新文件 `crates/core/src/lifecycle.rs`：

```rust
//! 节点生命周期钩子（ADR-0009）：长连接节点的部署/撤销 RAII 抽象。

use std::sync::Arc;
use serde_json::{Map, Value};
use tokio::task::JoinHandle;
use tokio::time::Duration;
use tokio_util::sync::CancellationToken;

use crate::{ContextRef, DataStore, EngineError, ExecutionEvent, SharedResources};

/// 部署钩子可用的受限上下文。
pub struct NodeLifecycleContext {
    pub resources: SharedResources,
    pub handle: NodeHandle,
    pub shutdown: CancellationToken,
}

/// 允许长连接型节点把外部消息"喂"进 DAG 数据通道。
#[derive(Clone)]
pub struct NodeHandle {
    // Runner 在创建上下文时填好这些通道
    // store + 下游 senders + event_tx + node_id + trace_id 工厂
}

impl NodeHandle {
    pub async fn emit(&self, payload: Value, metadata: Map<String, Value>)
        -> Result<(), EngineError> { /* ... */ }
}

/// RAII 句柄。Drop 时取消 + 释放协议资源；shutdown() 提供显式异步等待。
pub struct LifecycleGuard {
    inner: Option<LifecycleGuardInner>,
}

struct LifecycleGuardInner {
    token: CancellationToken,
    join: Option<JoinHandle<()>>,
    shutdown_timeout: Duration,
}

impl LifecycleGuard {
    pub fn noop() -> Self { Self { inner: None } }
    pub fn from_task(token: CancellationToken, join: JoinHandle<()>) -> Self { /* ... */ }
    pub async fn shutdown(mut self) { /* cancel + await timeout */ }
}

impl Drop for LifecycleGuard {
    fn drop(&mut self) {
        if let Some(inner) = self.inner.take() {
            inner.token.cancel();
            // JoinHandle 在 Drop 时不会 await；让 Tokio 自然回收。
            // 显式同步等待请用 shutdown().await。
        }
    }
}
```

- [ ] **Step 3: NodeTrait 加 `on_deploy` 默认实现**

`crates/core/src/node.rs`：

```rust
#[async_trait]
pub trait NodeTrait: Send + Sync {
    fn id(&self) -> &str;
    fn kind(&self) -> &'static str;
    async fn transform(&self, trace_id: Uuid, payload: Value)
        -> Result<NodeExecution, EngineError>;

    /// 节点部署时调用，早于任何 transform。默认返回 noop guard。
    async fn on_deploy(
        &self,
        _ctx: NodeLifecycleContext,
    ) -> Result<LifecycleGuard, EngineError> {
        Ok(LifecycleGuard::noop())
    }
}
```

- [ ] **Step 4: 单测覆盖 LifecycleGuard 的 RAII 行为**

新测试位于 `crates/core/src/lifecycle.rs` 的 `#[cfg(test)] mod tests`：
- `noop_guard_drop_不 panic`
- `guard_drop_触发 cancel`
- `shutdown_等待 join 完成`
- `shutdown_超时则强制返回`

- [ ] **Step 5: 验证**

```bash
cargo test -p nazh-core
cargo clippy -p nazh-core --all-targets -- -D warnings
```

---

## Task 1: Runner 部署/撤销路径改造

**Files:**
- Modify: `src/graph/types.rs`（WorkflowDeployment 增加 guards）
- Modify: `src/graph/deploy.rs`（按拓扑序 on_deploy + 失败回滚）
- Modify: `src/graph/runner.rs`（NodeHandle::emit 与 run_node apply_output 共享路径）

- [ ] **Step 1: WorkflowDeployment 携带 guards**

```rust
pub struct WorkflowDeployment {
    pub ingress: WorkflowIngress,
    pub streams: WorkflowStreams,
    pub(crate) lifecycle_guards: Vec<(String, LifecycleGuard)>, // (node_id, guard) 部署顺序
}

impl WorkflowDeployment {
    /// 按逆拓扑序 shutdown。
    pub async fn shutdown(self) {
        for (_, guard) in self.lifecycle_guards.into_iter().rev() {
            guard.shutdown().await;
        }
    }
}
```

- [ ] **Step 2: deploy_workflow_with_ai 加 on_deploy 阶段**

在 `src/graph/deploy.rs:83`（`for (node_id, node_definition) in &graph.nodes` 节点循环）**之前**新增按 `topology.execution_order`（如果存在；否则用 `topology.root_nodes` + BFS）的 `on_deploy` 调用阶段。

伪代码：

```rust
let mut guards: Vec<(String, LifecycleGuard)> = Vec::new();
let shutdown_token = CancellationToken::new();

for node_id in topology.deployment_order() {
    let node = registry.create(&graph.nodes[node_id], shared_resources.clone())?;
    let handle = NodeHandle::for_node(node_id, &senders, &topology, &store, &event_tx);
    let ctx = NodeLifecycleContext {
        resources: shared_resources.clone(),
        handle,
        shutdown: shutdown_token.child_token(),
    };
    match wrap_with_timeout_and_unwind(node.on_deploy(ctx), Duration::from_secs(10)).await {
        Ok(guard) => guards.push((node_id.clone(), guard)),
        Err(error) => {
            // 已注册 guards 按逆序 drop（RAII 兜底），返回错误
            return Err(error);
        }
    }
}
// 然后才进入现有的 spawn run_node 循环
```

注意：`topology.deployment_order()` 可能需要新增方法（拓扑序 `Vec<String>`），现有 `Topology` 结构需要检查 — 如果只暴露 `root_nodes` + `downstream` 邻接表，需要在 `topology.rs` 加一个 `topological_order()` 公共方法（Kahn 算法已有，复用）。

- [ ] **Step 3: 失败回滚的隔离测试**

新增 `tests/lifecycle.rs`：
- `on_deploy_失败时按逆序释放已部署节点的 guard`
- `on_deploy_panic_被 catch_unwind 隔离`
- `on_deploy_超时被强制取消`
- `shutdown_按逆拓扑序执行`

- [ ] **Step 4: NodeHandle::emit 与 run_node apply_output 共享路径**

`run_node` 内部已有把 `NodeOutput` 写 store + 广播 ContextRef + 发 Completed 事件的逻辑（`apply_output` 或类似名）。把这段抽成 `pub(crate) fn dispatch_node_output(...)`，让 `NodeHandle::emit` 复用。**禁止双份元数据合并逻辑**（ADR-0009 风险章节强调）。

- [ ] **Step 5: 验证**

```bash
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
```

---

## Task 2: 迁移 Timer 节点（最简单）

**Files:**
- Locate / Create: `crates/nodes-io/src/timer.rs`（如果不存在）
- Modify: `crates/nodes-io/src/lib.rs`（注册 timer 节点）
- Modify: `src-tauri/src/lib.rs`（删除 `spawn_timer_root_tasks` 调用 + 函数）

- [ ] **Step 1: 定位现有 timer 节点实现**

```bash
rg -n 'fn kind.*"timer"|"timer"\s*=>|TimerNode' crates/
```

确认 timer 节点的 transform 体（如已存在）目前是空 / `unreachable!` —— 因为它实际触发由壳层代持。新版会把触发逻辑搬进 `on_deploy`。

- [ ] **Step 2: 在 timer 节点实现 on_deploy**

```rust
#[async_trait]
impl NodeTrait for TimerNode {
    async fn on_deploy(&self, ctx: NodeLifecycleContext) -> Result<LifecycleGuard, EngineError> {
        let interval = Duration::from_millis(self.interval_ms);
        let immediate = self.immediate;
        let handle = ctx.handle;
        let token = ctx.shutdown;
        let join = tokio::spawn(async move {
            if immediate { let _ = handle.emit(Value::Object(Default::default()), Map::new()).await; }
            loop {
                tokio::select! {
                    _ = token.cancelled() => break,
                    () = tokio::time::sleep(interval) => {
                        let _ = handle.emit(Value::Object(Default::default()), Map::new()).await;
                    }
                }
            }
        });
        Ok(LifecycleGuard::from_task(token, join))
    }

    async fn transform(&self, _trace_id: Uuid, payload: Value)
        -> Result<NodeExecution, EngineError>
    {
        Ok(NodeExecution::broadcast(payload))
    }
}
```

- [ ] **Step 3: 删除壳层 timer 代持**

从 `src-tauri/src/lib.rs` 删除：
- `collect_timer_root_specs`（line 2376）
- `spawn_timer_root_tasks`（line 2484）
- `deploy_workflow` 中调用这两者的代码段
- 如果 `TimerRootSpec` / `DesktopTriggerTask` 仅服务 timer / serial / mqtt 三者，先标记，待 Task 5 集中清理

- [ ] **Step 4: 测试 + 手动 E2E**

```bash
cargo test --workspace
# 启动 dev 模式
cd src-tauri && ../web/node_modules/.bin/tauri dev --no-watch
# 部署一个含 timer 节点的工作流；观察是否按设定 interval 触发
# 撤销工作流；观察 timer 是否真的停（用 ps / 日志）
# 重部署；验证不存在残留任务
```

---

## Task 3: 迁移 Serial 节点

**Files:**
- Locate / Create: `crates/nodes-io/src/serial_*.rs`
- Modify: `crates/nodes-io/src/lib.rs`
- Modify: `src-tauri/src/lib.rs`

- [ ] **Step 1: 调研当前 serial 节点位置**

```bash
rg -n 'serial' crates/nodes-io/src/lib.rs | head
rg -n 'NodeTrait.*for.*Serial' crates/
```

- [ ] **Step 2: 把 run_serial_root_reader 的逻辑搬入 on_deploy**

注意点：
- 串口阻塞读用 `std::thread::spawn`（非 `tokio::spawn`），需要包一层 `tokio::sync::oneshot` 或用 `tokio::task::spawn_blocking`。`LifecycleGuard::from_task` 可能需要新增重载 `from_blocking_task` 接受 `std::thread::JoinHandle`。
- 帧拼接、delimiter、idle gap、heartbeat 语义**完全保留**——把 `flush_idle_serial_frame` / `drain_serial_delimited_frame` / `submit_serial_frame` 迁过来，作为节点 impl 的私有 helper。

- [ ] **Step 3: 删除壳层 serial 代持**

从 `src-tauri/src/lib.rs` 删除：
- `collect_serial_root_specs`、`spawn_serial_root_tasks`、`run_serial_root_reader`
- `flush_idle_serial_frame`、`drain_serial_delimited_frame`、`submit_serial_frame`
- `emit_serial_trigger_failure`（功能由 `NodeHandle::emit` + 错误事件路径替代）

- [ ] **Step 4: 测试 + 手动 E2E**

需要真实串口或 com0com / socat 虚拟端口。无硬件可用时退而求其次：单元测试覆盖 frame 拼接逻辑（迁移过来的 helper），E2E 留给硬件验收阶段。

---

## Task 4: 迁移 MQTT 订阅节点

**Files:**
- Modify: `crates/nodes-io/src/mqtt_client.rs`（或类似路径）
- Modify: `crates/nodes-io/src/lib.rs`
- Modify: `src-tauri/src/lib.rs`

- [ ] **Step 1: 在现有 MqttClientNode 上实现 on_deploy**

判断是否 `subscribe` 模式：

```rust
async fn on_deploy(&self, ctx: NodeLifecycleContext) -> Result<LifecycleGuard, EngineError> {
    if self.config.mode != "subscribe" {
        return Ok(LifecycleGuard::noop()); // publish 模式不需要 on_deploy
    }
    // 借连接、连接 broker、订阅 topic、起事件循环
    // 每条收到的消息走 handle.emit() 推进 DAG
}
```

- [ ] **Step 2: 复刻 `collect_mqtt_root_specs` 中的连接元数据校验**

`src-tauri/src/lib.rs:2569-2664` `collect_mqtt_root_specs` 不只是"读 graph"——它在借连接后还做了一组校验：
- 从 `ConnectionGuard::metadata()` 读 `host` / `port` / `topic` 默认值
- topic 为空 → `mark_failure` + 返回 `node_config` 错误
- host 为空 → `mark_failure` + 返回 `node_config` 错误
- 校验通过 → `mark_success`

这套**必须在 `on_deploy` 内复刻**——通过 `ctx.resources.get::<SharedConnectionManager>()` 拿连接管理器，按相同顺序借出 / 校验 / 返回 / mark。任何遗漏都会让 broker 元数据回退缺失或连接健康度统计漂移。

- [ ] **Step 3: 处理 broker 重连 / QoS / 心跳**

把 `run_mqtt_root_subscriber` 中的重连退避、心跳上报、QoS 处理逻辑全部搬过来。重连过程中如果 token cancel，立刻退出。

- [ ] **Step 4: 删除壳层 MQTT 代持**

从 `src-tauri/src/lib.rs` 删除：
- `collect_mqtt_root_specs`、`spawn_mqtt_root_tasks`、`run_mqtt_root_subscriber`
- `emit_mqtt_trigger_failure`

- [ ] **Step 5: 测试 + 手动 E2E**

```bash
# 起一个本地 MQTT broker（mosquitto / emqx）
mosquitto -p 1883
# 部署含 mqttClient 订阅节点的工作流
# 用 mosquitto_pub 发一条消息，看是否触发下游
mosquitto_pub -h localhost -t test/topic -m "{}"
# 撤销 → 用 ss / netstat 看 TCP 连接是否断
# 重部署 → 看是否能再次接收
```

---

## Task 5: 删除壳层 trigger 抽象 + 共享辅助

> **范围扩展说明**（2026-04-26 偏差评估补充）：原计划这一节只列了"删除 helper"。实际偏差评估发现整套 `*RootSpec` 数据 struct + `DesktopTriggerTask` / `TriggerJoinHandle` 抽象 + `DesktopWorkflow.trigger_tasks` 字段 + `abort_triggers()` 方法都是**为三家代持服务专门发明的**，必须一并清掉，否则就是"删了房子留地基"。涉及 IPC 契约 `UndeployResponse.aborted_timer_count` —— 见 Step 4。

**Files:**
- Modify: `src-tauri/src/lib.rs`（核心改造）
- Modify: `crates/tauri-bindings/src/lib.rs`（IPC 契约：`UndeployResponse`）
- Modify: `web/src/generated/`（ts-rs 重新导出）
- Modify: `web/src/lib/tauri.ts` 与调用方（如改了 `aborted_timer_count` 字段名）
- Modify: `src-tauri/Cargo.toml`（如可移除 rumqttc / serialport 依赖）

- [ ] **Step 1: 删除 `*RootSpec` 数据 struct**

`src-tauri/src/lib.rs:736-757` 三个数据 struct 是壳层 collect 阶段的中间产物，迁移完成后已无产生方与消费方：
- `TimerRootSpec`（line 736）
- `SerialRootSpec`（line 743）
- `MqttRootSpec`（line 750）

迁后参数源头变为节点自身 `self.config` + `ctx.resources` 取连接管理器。**全部删除**。

- [ ] **Step 2: 删除 `DesktopTriggerTask` / `TriggerJoinHandle` 抽象**

`src-tauri/src/lib.rs:561-569`：

```rust
enum TriggerJoinHandle { Async(...), Thread(...) }
struct DesktopTriggerTask { cancel: Arc<AtomicBool>, join: TriggerJoinHandle }
```

整套被引擎层 `LifecycleGuard` 取代（RAII + `CancellationToken`）。**整体删除**，不留 type alias / fallback。

- [ ] **Step 3: 改造 `DesktopWorkflow` 结构**

`src-tauri/src/lib.rs:548-619`：

```rust
struct DesktopWorkflow {
    workflow_id: String,
    metadata: RuntimeWorkflowMetadata,
    policy: WorkflowRuntimePolicy,
    dispatch_router: WorkflowDispatchRouter,
    observability: Option<SharedObservabilityStore>,
    node_count: usize,
    edge_count: usize,
    root_nodes: Vec<String>,
    trigger_tasks: Vec<DesktopTriggerTask>,        // ← 删除
    runtime_tasks: Vec<tauri::async_runtime::JoinHandle<()>>,
    deployment: Option<WorkflowDeployment>,         // ← 新增（持有引擎 lifecycle_guards）
}
```

注意 `WorkflowDeployment::shutdown(self)` 是消费 self 的方法（见 Task 1 Step 1），所以 `deployment` 必须是 `Option<WorkflowDeployment>`，shutdown 时 `take()` 出来再 `.shutdown().await`。

`abort_triggers()` (line 573-600) 改造为 `shutdown_runtime()`：
```rust
async fn shutdown_runtime(&mut self) -> usize {
    for task in &self.runtime_tasks { task.abort(); }
    let count = self.deployment_lifecycle_count();
    if let Some(deployment) = self.deployment.take() {
        deployment.shutdown().await;
    }
    count
}
```

- [ ] **Step 4: 改造 `abort_triggers` 调用点 + IPC 契约**

调用点 2 处（已确认）：
- `src-tauri/src/lib.rs:1081` `deploy_workflow` 中替换已存在的工作流时 → 改为 `existing.shutdown_runtime().await`
- `src-tauri/src/lib.rs:1307` `undeploy_workflow` 命令中 → 改为 `workflow.shutdown_runtime().await`

**IPC 契约 `UndeployResponse.aborted_timer_count` 决策**（二选一，写明决策）：
- **方案 A**（推荐）：保留字段名，语义改为"shutdown 的 lifecycle guards 数量"。无 IPC 契约破坏，前端无需改。在 `crates/tauri-bindings/src/lib.rs` 该字段加 `#[deprecated]` doc，下个 ADR 再改名。
- **方案 B**：改名为 `shutdown_lifecycle_count`，需要：改 `crates/tauri-bindings/src/lib.rs` → 跑 `cargo test -p tauri-bindings --features ts-export export_bindings` → 改 `web/src/lib/tauri.ts` 与所有读取该字段的前端代码。

选 A 即可，不要为了"语义干净"动 IPC 契约。

- [ ] **Step 5: 删除共享 helper**

确认无引用后删除：
- `emit_trigger_failure`（line 3209）— 触发失败由 `LifecycleGuard` 内部 emit + Runner 路径处理
- `sleep_with_cancel`（line 3258）— `CancellationToken::cancelled().await` 替代

- [ ] **Step 6: 检查依赖收缩**

`src-tauri/Cargo.toml` 中的 `rumqttc`、`serialport`、`tokio-modbus` 现在应该只在 `crates/nodes-io` 中需要。如果 `src-tauri` 已不直接 use，可以删依赖。`Arc<AtomicBool>` / `Ordering` 的直接 import 也应可以删除（如果没有其他使用）。

- [ ] **Step 7: 清理 `deploy_workflow` Tauri command 入口**

`src-tauri/src/lib.rs:952` 中删除：
- `collect_timer_root_specs` / `collect_serial_root_specs` / `collect_mqtt_root_specs` 三连调用（line 1028-1034）
- `spawn_timer_root_tasks` / `spawn_serial_root_tasks` / `spawn_mqtt_root_tasks` 三连调用（line 1084-1100）
- 在 `state.workflows.insert(...)` 时把 `WorkflowDeployment` 传入新增的 `deployment` 字段（替代原 `trigger_tasks`）

> **关于 `#[allow(clippy::too_many_lines)]`**：删完后 `deploy_workflow` 从 ~261 行 → ~237 行，**仍超过 100 行阈值**，`#[allow]` 必须保留。如要彻底拆分，留待后续独立 PR（不放在本计划范围）。

- [ ] **Step 8: 验证**

```bash
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo check --manifest-path src-tauri/Cargo.toml
# 如选了方案 B 改了 IPC 契约：
cargo test -p tauri-bindings --features ts-export export_bindings
npm --prefix web run test
```

---

## Task 6: 文档与状态同步

**Files:**
- Modify: `docs/adr/0009-节点生命周期钩子.md`（状态：已接受 → 已实施）
- Modify: `docs/adr/README.md`（索引行）
- Modify: `AGENTS.md`（Project Status / ADR Execution Order / 已知技术债）
- Modify: `crates/core/AGENTS.md`（NodeTrait 契约 + lifecycle 模块说明）
- Modify: `crates/nodes-io/AGENTS.md`（timer / serial / mqtt 现已自持生命周期）
- Modify: `README.md`（如有 IPC 表格涉及触发器，同步注释）

- [ ] **Step 1: ADR-0009 状态升级**

Front-matter `状态: 提议中` → `状态: 已实施`。在 `## 备注` 末尾追加一行实施日期与对应 commit。

- [ ] **Step 2: docs/adr/README.md 索引更新**

把 ADR-0009 的状态列改为"已实施"。

- [ ] **Step 3: AGENTS.md 同步**

- "## Project Status" 章节："Immediate known tech debt" 删掉 "MQTT subscriber / Timer / Serial root lifecycle is owned by the Tauri shell..." 那一条
- "ADR Execution Order" 表格：把 ADR-0009 标 ✅
- 加一条新的"Recent batch of ADRs"日期 / 状态记录

- [ ] **Step 4: 各 crate AGENTS.md 同步**

- `crates/core/AGENTS.md`：NodeTrait 增加 `on_deploy` + lifecycle 模块说明，引用 ADR-0009
- `crates/nodes-io/AGENTS.md`：timer / serial / mqtt 节点的"是否持有 on_deploy"列加 ✅，移除"由壳层代持"备注

---

## Task 7: 全量验证 + 手动 E2E 矩阵

- [ ] **Step 1: 自动化检查**

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo check --manifest-path src-tauri/Cargo.toml
cargo test -p tauri-bindings --features ts-export export_bindings  # 若 IPC 类型变更
```

- [ ] **Step 2: 手动 E2E 矩阵**

部署 → 触发 → 撤销 → 重部署，三类节点各一遍：

| 节点 | 部署 | 触发 | 撤销 | 重部署 |
|------|------|------|------|--------|
| timer | ☐ | ☐ | ☐ | ☐ |
| serial（如有硬件）| ☐ | ☐ | ☐ | ☐ |
| mqttClient subscribe | ☐ | ☐ | ☐ | ☐ |

撤销后用 `ss -tnp | grep <broker_ip>` 验证 TCP 连接确实断开。

- [ ] **Step 3: 失败注入**

- 部署时 broker 不可达：on_deploy 应返回 Err，整图 DeployFailed
- 部署中途 N 个 on_deploy 成功、N+1 失败：前 N 个的 guard 必须被 RAII 释放
- 运行中 token cancel：所有节点循环必须在 5s 内退出

- [ ] **Step 4: 核对 `#[allow(clippy::too_many_lines)]` 收支表**

完成本计划后，4 个 `#[allow]` 中应有 2 个**自动消失**（函数被删）、2 个**保留**（与本计划无关）：

| `#[allow]` 位置 | 本计划完成后状态 | 原因 |
|---|---|---|
| `run_mqtt_root_subscriber` (lib.rs:2706) | ✅ 自动消失 | 函数整体迁回 `MqttClientNode::on_deploy`，原位置删除 |
| `run_serial_root_reader` (lib.rs:2913) | ✅ 自动消失 | 函数整体迁回 `SerialNode::on_deploy`，原位置删除 |
| `deploy_workflow` (lib.rs:951) | 🟡 保留 | 删 ~24 行后仍 ~237 行，超 100 行阈值。彻底拆分留待独立 PR |
| `record_execution_event` (observability.rs:229) | 🟡 保留 | 与本计划完全无关 |

如果 `run_mqtt_root_subscriber` / `run_serial_root_reader` 的 `#[allow]` 没消失，说明壳层代码没删干净（Task 5 漏做）——这是验证本计划落地的硬指标。

---

## 建议提交拆分

- [ ] `feat(core): 引入 NodeTrait::on_deploy 与 LifecycleGuard 骨架`（Task 0）
- [ ] `refactor(graph): 部署路径加入 on_deploy 阶段与逆序撤销`（Task 1）
- [ ] `refactor(nodes-io): timer 节点持有自身生命周期`（Task 2）
- [ ] `refactor(nodes-io): serial 节点持有自身生命周期`（Task 3）
- [ ] `refactor(nodes-io): mqttClient 订阅模式持有自身生命周期`（Task 4）
- [ ] `refactor(tauri): 删除 src-tauri 中 timer/serial/mqtt 代持代码`（Task 5）
- [ ] `docs: ADR-0009 标记为已实施 + 同步 AGENTS.md 与 README`（Task 6）

每个 commit 用 `git commit -s`，提交信息中文并保持单一关注点。Task 2/3/4 各自独立后**立刻**做 E2E 验证；不要把三类堆到最后一起验证（撤销竞态最容易藏在多类节点交互里）。

---

## 风险登记（来自 ADR-0009 §风险章节）

- **R1 部署时序错乱**：`NodeHandle::emit` 在 `on_deploy` 阶段（所有节点都还没 spawn run_node）就调用——必须等所有 on_deploy 完成后再开放 `dispatch` 入口与 transform 任务。Task 1 Step 2 中 `for node_id in topology.deployment_order()` 之后才进入 spawn run_node 循环。
- **R2 撤销竞态**：cancel 后 emit 应立即返回 `Cancelled`；订阅循环用 `tokio::select! { _ = token.cancelled() => break, ... }` 确保第一时间退出。
- **R3 回滚不干净**：guards 容器持有所有已部署 guard，错误返回前不要让它早 drop。Task 1 Step 3 用集成测试覆盖。
- **R4 双轨残留**：本计划要求"同 PR 完成迁移与删除"，禁止 fallback 路径。Task 2/3/4 与 Task 5 必须**同分支**合并；不能"先 PR 加 on_deploy 再另一个 PR 删壳层"。
