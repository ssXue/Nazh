# Cargo Clippy Workspace 修复计划

> **For agentic workers:** 按 checkbox 顺序执行；每完成一个批次同步勾选并运行对应验证命令。本文基于 2026-04-25 在仓库根目录执行的 clippy 输出整理。

**Goal:** 修复 `cargo clippy --workspace --all-targets -- -D warnings` 暴露的 workspace 级 lint 失败，确保 Tauri shell、Rust crates 与测试代码在统一 clippy 命令下通过。

**Architecture:** 先处理无行为风险的机械 lint，再拆分 `src-tauri` 中过长和参数过多的函数，最后修正文档中的 clippy 命令口径。保持 Ring 0 / Ring 1 / Tauri shell 边界不变，不引入新的运行时依赖。

**Tech Stack:** Rust 2024、Cargo workspace、Clippy pedantic lints、Tauri v2、Tokio、serde_json、rumqttc、serialport

---

## 当前诊断

### 已运行命令

```bash
cargo clippy --all-targets -- -D warnings
cargo clippy --workspace --all-targets -- -D warnings
```

### 结果

- `cargo clippy --all-targets -- -D warnings` 通过，但只检查到默认包路径，未完整覆盖 `src-tauri` 与 `crates/tauri-bindings`。
- `cargo clippy --workspace --all-targets -- -D warnings` 失败，主要集中在 `src-tauri/src/lib.rs` 与 `src-tauri/src/observability.rs`。
- 少量测试 lint 位于 `crates/core/src/event.rs`、`crates/core/src/plugin.rs`、`crates/ai/src/client.rs`。

### 主要 lint 类型

- `expect_used`
- `default_trait_access`
- `similar_names`
- `too_many_lines`
- `too_many_arguments`
- `needless_pass_by_value`
- `cast_sign_loss`
- `cast_possible_truncation`
- `collapsible_if`
- `manual_let_else`
- `assigning_clones`
- `needless_continue`
- `ptr_arg`
- `if_same_then_else`
- `match_same_arms`
- `implicit_clone`
- `manual_ignore_case_cmp`

---

## Task 1: 修复测试与小范围机械 lint

**Files:**
- Modify: `crates/core/src/event.rs`
- Modify: `crates/core/src/plugin.rs`
- Modify: `crates/ai/src/client.rs`
- Modify: `src-tauri/src/lib.rs`
- Modify: `src-tauri/src/observability.rs`

- [ ] **Step 1: 替换测试中的 `expect()`**

将测试内的：

```rust
serde_json::to_value(value).expect("...")
serde_json::from_value(value).expect("...")
```

改为 `let Ok(value) = ... else { panic!("..."); };`。保持测试失败信息为中文，不引入 `unwrap()` / `expect()`。

涉及位置：
- `crates/core/src/event.rs:266`
- `crates/core/src/event.rs:289`
- `crates/ai/src/client.rs:148`
- `crates/ai/src/client.rs:178`
- `crates/ai/src/client.rs:200`

- [ ] **Step 2: 明确 serde_json Map 默认值**

将 `Value::Object(Default::default())` 改为 `Value::Object(serde_json::Map::default())`。

涉及位置：
- `crates/core/src/plugin.rs:411`
- `src-tauri/src/lib.rs:2514`
- `src-tauri/src/lib.rs:2530`
- `src-tauri/src/lib.rs:3280`

- [ ] **Step 3: 修复局部表达式 lint**

按 clippy 建议逐项修改：
- `src-tauri/src/lib.rs:822`：`&mut Vec<PersistedDeploymentSession>` 改为 `&mut [PersistedDeploymentSession]`
- `src-tauri/src/lib.rs:1124`：改为 `let Ok(payload) = ... else { continue };`
- `src-tauri/src/lib.rs:1332`：改为 `clone_from`
- `src-tauri/src/lib.rs:1792`：合并相同 `usb-serial` 分支
- `src-tauri/src/lib.rs:1981`：`stringify_error` 改为接收 `&EngineError`，同步调整调用点；如调用点需要消费 error，则保留局部 `error.to_string()` 避免改变语义
- `src-tauri/src/lib.rs:2337`、`2794`：删除冗余 `continue`
- `src-tauri/src/lib.rs:2610`：改为 `eq_ignore_ascii_case`
- `src-tauri/src/lib.rs:2880`：`error.to_string()` 改为 `error.clone()`
- `src-tauri/src/lib.rs:2901`、`2910`：合并相同 match arms，并删除冗余 `continue`
- `src-tauri/src/lib.rs:3396`、`3401`：合并相同 match arms
- `src-tauri/src/lib.rs:3565-3576`：折叠嵌套 `if let`
- `src-tauri/src/observability.rs:552`：改为 `clone_into`
- `src-tauri/src/observability.rs:834`：删除冗余 `continue`

- [ ] **Step 4: 第一轮验证**

```bash
cargo fmt --all
cargo clippy --workspace --all-targets -- -D warnings
```

Expected: 机械 lint 数量下降，剩余问题主要为函数拆分类 lint。

---

## Task 2: 拆分 `ObservabilityStore::record_execution_event`

**Files:**
- Modify: `src-tauri/src/observability.rs`

- [ ] **Step 1: 避免 `state` / `stage` 相似命名**

将 `let mut state = self.state.lock().await;` 重命名为更明确的 `runtime_state`，或将 completed 分支里的 `stage` 改为 `node_stage`。优先选择能提升可读性的命名。

- [ ] **Step 2: 提取 duration 计算 helper**

新增小函数：

```rust
fn elapsed_ms_since(now: DateTime<Utc>, started_at: DateTime<Utc>) -> u64
```

内部使用 `.num_milliseconds().max(0).cast_unsigned()`，消除 `cast_sign_loss`。

- [ ] **Step 3: 将事件 entry 构建拆成 helper**

拆出以下 helper，降低 `record_execution_event` 行数：
- `build_started_execution_entry`
- `build_completed_execution_entry`
- `build_failed_execution_entry`
- `build_output_execution_entry`
- `build_finished_execution_entry`

helper 只负责构造 `ObservabilityEntry`，不持有 mutex，不写文件。

- [ ] **Step 4: 将 HTTP alert 写入移出锁范围**

在 completed 分支中只收集 `AlertDeliveryRecord`，释放 `runtime_state` 后再调用：

```rust
append_jsonl(self.root_dir.join(ALERTS_FILE), &alert).await
```

这样顺便减少锁内 async 文件写入风险。

- [ ] **Step 5: 第二轮验证**

```bash
cargo fmt --all
cargo clippy --workspace --all-targets -- -D warnings
```

Expected: `record_execution_event` 不再触发 `too_many_lines`、`similar_names`、`cast_sign_loss`、`collapsible_if`。

---

## Task 3: 收敛 `ObservabilityStore::build_entry` 参数

**Files:**
- Modify: `src-tauri/src/observability.rs`

- [ ] **Step 1: 新增 entry 草稿结构**

新增内部结构体，名称可为 `ObservabilityEntryDraft`：

```rust
struct ObservabilityEntryDraft {
    level: String,
    category: String,
    source: String,
    message: String,
    detail: Option<String>,
    trace_id: Option<String>,
    node_id: Option<String>,
    duration_ms: Option<u64>,
    data: Option<Value>,
    timestamp: DateTime<Utc>,
}
```

- [ ] **Step 2: 修改 `build_entry` 签名**

将 11 个参数收敛为：

```rust
fn build_entry(&self, draft: ObservabilityEntryDraft) -> ObservabilityEntry
```

必要时为 draft 增加小构造函数或 builder-like helper，但不要引入外部依赖。

- [ ] **Step 3: 更新所有调用点**

逐个替换 `record_execution_event`、`record_result`、`record_audit`、`record_external_failure` 等调用点。保持原有字段值完全一致。

- [ ] **Step 4: 第三轮验证**

```bash
cargo fmt --all
cargo clippy --workspace --all-targets -- -D warnings
```

Expected: `build_entry` 不再触发 `too_many_arguments`。

---

## Task 4: 拆分 trace summary 聚合逻辑

**Files:**
- Modify: `src-tauri/src/observability.rs`

- [ ] **Step 1: 将 `TraceAccumulator` 移到函数外**

把 `TraceAccumulator` 提升为私有结构体，保留在 `observability.rs` 内部。

- [ ] **Step 2: 为 accumulator 添加方法**

新增方法：
- `fn apply_entry(&mut self, entry: &ObservabilityEntry)`
- `fn apply_alert(&mut self, alert: &AlertDeliveryRecord)`
- `fn finish(self, trace_id: String) -> ObservabilityTraceSummary`

将状态计算、时间范围、node 计数、输出/失败计数从 `build_trace_summaries` 主体中移出。

- [ ] **Step 3: 精简 `build_trace_summaries`**

主函数只保留：
- 遍历 events
- 遍历 alerts
- 转换并排序 summaries
- 应用 limit

- [ ] **Step 4: 第四轮验证**

```bash
cargo fmt --all
cargo clippy --workspace --all-targets -- -D warnings
```

Expected: `build_trace_summaries` 不再触发 `too_many_lines`。

---

## Task 5: 拆分部署命令 `deploy_workflow`

**Files:**
- Modify: `src-tauri/src/lib.rs`

- [ ] **Step 1: 提取部署准备结构**

新增私有结构体，名称可为 `DeploymentPreparation`，收纳：
- normalized `WorkflowGraph`
- `workspace_dir`
- `workflow_id`
- `WorkflowRuntimePolicy`
- `RuntimeWorkflowMetadata`
- timer / serial / mqtt root specs
- node / edge counts
- optional `ObservabilityStore`

- [ ] **Step 2: 提取准备函数**

新增：

```rust
async fn prepare_workflow_deployment(...) -> Result<DeploymentPreparation, String>
```

负责输入大小校验、AST 解析、SQL writer 路径标准化、连接定义更新、observability store 创建、根触发 spec 收集。

- [ ] **Step 3: 提取 engine deployment 函数**

新增：

```rust
async fn deploy_engine_graph(...) -> Result<..., String>
```

负责调用 `deploy_workflow_graph`，并在失败时写入部署失败审计。

- [ ] **Step 4: 提取事件和结果转发任务**

新增：
- `spawn_workflow_event_forwarder`
- `spawn_workflow_result_forwarder`

保持事件名不变：
- `workflow://node-status-v2`
- `workflow://node-status`
- `workflow://result-v2`
- `workflow://result`

- [ ] **Step 5: 提取运行态登记与审计**

将 `state.workflows.insert(...)`、active workflow 更新、部署成功审计、`workflow://deployed` emit 收敛到独立 helper。

- [ ] **Step 6: 第五轮验证**

```bash
cargo fmt --all
cargo clippy --workspace --all-targets -- -D warnings
cargo check --manifest-path src-tauri/Cargo.toml
```

Expected: `deploy_workflow` 不再触发 `too_many_lines`，部署行为保持一致。

---

## Task 6: 拆分 MQTT 根订阅任务

**Files:**
- Modify: `src-tauri/src/lib.rs`

- [ ] **Step 1: 引入 MQTT 运行上下文**

新增私有结构体，名称可为 `MqttRootRuntime`，持有：
- `AppHandle`
- `WorkflowDispatchRouter`
- `SharedConnectionManager`
- `Option<SharedObservabilityStore>`
- `workflow_id`
- `MqttRootSpec`
- `Arc<AtomicBool>`

任务 spawn 时仍 clone 进入上下文，`run_mqtt_root_subscriber` 改为接收一个上下文参数，消除 `needless_pass_by_value` 与参数膨胀。

- [ ] **Step 2: 拆分连接与订阅步骤**

新增 helper：
- `build_mqtt_options`
- `wait_mqtt_connack`
- `subscribe_mqtt_topic`
- `handle_mqtt_publish`
- `record_mqtt_disconnect`

- [ ] **Step 3: 合并事件循环中空分支**

将 `Ok(Ok(_))` 与 timeout 分支中无动作的 `continue` 改为自然落到 match 末尾，避免 `needless_continue`。

- [ ] **Step 4: 第六轮验证**

```bash
cargo fmt --all
cargo clippy --workspace --all-targets -- -D warnings
```

Expected: `run_mqtt_root_subscriber` 不再触发 `too_many_lines`、`needless_pass_by_value`、`needless_continue`、`match_same_arms`。

---

## Task 7: 拆分 Serial 根读取任务

**Files:**
- Modify: `src-tauri/src/lib.rs`

- [ ] **Step 1: 引入串口运行上下文**

新增私有结构体，名称可为 `SerialRootRuntime`，持有：
- `AppHandle`
- `WorkflowDispatchRouter`
- `SharedConnectionManager`
- `Option<SharedObservabilityStore>`
- `workflow_id`
- `SerialRootSpec`
- `Arc<AtomicBool>`

`run_serial_root_reader` 改为接收该上下文，内部按引用使用字段。

- [ ] **Step 2: 安全转换串口连接耗时**

将：

```rust
connect_started_at.elapsed().as_millis() as u64
```

改为：

```rust
u64::try_from(connect_started_at.elapsed().as_millis()).unwrap_or(u64::MAX)
```

注意：这里的 `unwrap_or` 不是 `unwrap()`，不违反项目约束。

- [ ] **Step 3: 拆分串口读循环**

新增 helper：
- `open_serial_port`
- `handle_serial_read`
- `handle_serial_timeout`
- `record_serial_disconnect`

这些 helper 只围绕现有行为拆分，不改变 frame 拼接、delimiter、idle gap、heartbeat 语义。

- [ ] **Step 4: 收敛 `flush_idle_serial_frame` 参数**

新增 `SerialFrameSink` 或复用 `SerialRootRuntime` 的引用，令 `flush_idle_serial_frame` 参数不超过 7 个。

- [ ] **Step 5: 第七轮验证**

```bash
cargo fmt --all
cargo clippy --workspace --all-targets -- -D warnings
```

Expected: `run_serial_root_reader` 与 `flush_idle_serial_frame` 不再触发 `too_many_lines`、`too_many_arguments`、`needless_pass_by_value`、`cast_possible_truncation`。

---

## Task 8: 同步文档中的 clippy 命令

**Files:**
- Modify: `AGENTS.md`

- [ ] **Step 1: 更新 Build & Dev Commands**

将 lint 命令从：

```bash
cargo clippy --all-targets -- -D warnings
```

改为：

```bash
cargo clippy --workspace --all-targets -- -D warnings
```

- [ ] **Step 2: 更新 Code Review 章节**

同样将本地 review 要求中的 clippy 命令改为 workspace 版本，避免未来继续漏检 `src-tauri`。

- [ ] **Step 3: 检查是否还有旧命令**

```bash
rg "cargo clippy --all-targets" AGENTS.md README.md docs
```

Expected: 只保留历史计划中的旧命令，当前权威文档使用 workspace 版本。

---

## Task 9: 全量验证

- [ ] **Step 1: Rust 格式检查**

```bash
cargo fmt --all -- --check
```

- [ ] **Step 2: Workspace clippy**

```bash
cargo clippy --workspace --all-targets -- -D warnings
```

- [ ] **Step 3: Workspace 测试**

```bash
cargo test --workspace
```

- [ ] **Step 4: Tauri shell compile-check**

```bash
cargo check --manifest-path src-tauri/Cargo.toml
```

- [ ] **Step 5: 如触及 IPC 类型，确认不需要 ts-rs 导出**

本计划不应修改 IPC 边界类型。如实际执行时修改了 `#[ts(export)]` 类型，必须补跑：

```bash
cargo test -p tauri-bindings --features ts-export export_bindings
```

并提交 `web/src/generated/` diff。

---

## 建议提交拆分

- [ ] `fix: 修复 workspace clippy 机械 lint`
- [ ] `refactor: 拆分观测事件记录逻辑`
- [ ] `refactor: 拆分部署命令与根触发任务`
- [ ] `docs: 统一 workspace clippy 命令`

每个 commit 使用 `git commit -s`，提交信息使用中文并保持单一关注点。
