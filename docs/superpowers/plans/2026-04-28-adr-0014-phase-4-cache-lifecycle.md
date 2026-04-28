# ADR-0014 Phase 4 实施计划：缓存生命周期与策略 + 用例 4 旁路 fan-out

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 把 ADR-0014 spec 第九章 Phase 4 的三件事落地——（1）引脚级"空槽兜底"声明（`block_until_ready` / `default_value` / `skip` 三选一）；（2）引脚级 TTL（最近 N 秒未更新视为过期）；（3）`NodeCapabilities::PURE` 与引脚二分协同：PURE + Data 输出 = 输入哈希记忆缓存（同一 trace 内多次 pull 同 input → 命中），并跑通 spec 用例 4「modbusRead.latest 旁路 fan-out 给 dashboard / historySampler / alert 三种节奏消费者」。**前置条件**：Phase 3（pull 路径）已落地。

**Architecture:**
- **`PinDefinition` 加两个新字段**（仅对 `kind: Data` 输入引脚有意义）：
  - `empty_policy: EmptyPolicy`（`enum { BlockUntilReady, DefaultValue(Value), Skip }`，默认 `BlockUntilReady` ——保留 Phase 3 的"槽空 → 报错" 行为，Phase 4 把"报错"重塑为"等"，并提供两条 escape valve）
  - `ttl_ms: Option<u64>`（None = 永久；Some(n) = 缓存值在 `produced_at + n` 之后视为过期，等价于槽空）
- **拍板 Spec 第十一章决策 1（缓存空槽默认行为）**：默认 `BlockUntilReady`，超时 `EngineError::DataPinPullTimeout`（新 variant）。理由：工业场景下"延迟一拍"通常比"用陈旧默认值"安全；但允许节点作者按 pin 级别 escape。
- **拍板 Spec 第十一章决策 2（是否引入 TTL）**：引入，但默认 `None`。理由：与 `BlockUntilReady` 协同——某些场景下我们不仅希望"非空"，还希望"非陈旧"，TTL 让此意图可声明。
- **`pull_data_inputs` 升级**：把 Phase 3 的"读 cache 失败 → 立刻 `EngineError::DataPinCacheEmpty`"改为按 `empty_policy` 分支：
  - `BlockUntilReady` → 在 `tokio::sync::Notify`-based wakeup 上等，超时（per-pin `block_timeout_ms`，默认 5000）后 `DataPinPullTimeout`
  - `DefaultValue(v)` → 直接返回 v
  - `Skip` → 返回 `Value::Null`，由节点作者自行处理 null（pin tooltip 提示）
  - TTL：读出 `CachedOutput` 后检查 `Utc::now() - produced_at > ttl_ms`，过期则按 `empty_policy` 降级（与槽空同路径）
- **`OutputCache` 升级**：每个 slot 关联一个 `Arc<Notify>`（`tokio::sync::Notify`），`store` 时 `notify_waiters()`，`BlockUntilReady` 等待方 `notified().await`。
- **PURE 输入哈希记忆**（与 ADR-0011 PURE capability 协同）：当 pull 上游为"pure-form 节点 ∧ 该节点带 PURE capability"时，pull collector 维护 per-trace 缓存：以"上游节点 id + 上游 pin id + 序列化输入哈希"为键，命中则跳过 transform 直接返回缓存值。`tracing` 记录命中率。
- **用例 4 demonstration**：仍用 stub 方式——拓展 Phase 3 集成测试模板，加 dashboard / historySampler / alertGate 三个消费者并发拉同一份 `modbusRead.latest`-like cache 槽，断言：（a）modbusRead transform 仅被调用 1 次（生产侧 IOPS 不爆涨）；（b）三个消费者各自得到一致快照。
- **前端**：
  - `PinDefinition` ts-rs 自动同步 `empty_policy` / `ttl_ms`
  - `pin-schema-cache` / pin tooltip 显示 "空槽策略：等待 / 默认值 / 跳过"
  - settings panel 给 Data 输入引脚提供策略选择控件（保守：仅暴露 `BlockUntilReady` + 自定义超时；高级用户通过 JSON config 设 `DefaultValue` / `Skip`）

**Tech Stack:** Rust（`crates/core/src/pin.rs` 加字段 + `cache.rs` 加 Notify、`crates/core/src/error.rs` 加 `DataPinPullTimeout`、`src/graph/pull.rs` 重写 wait/timeout/TTL 路径），TypeScript / React（pin tooltip + settings panel），ts-rs（自动覆盖新字段），Vitest，Playwright。

---

## File Structure

| 操作 | 路径 | 责任 |
|------|------|------|
| 修改 | `crates/core/src/pin.rs` | `EmptyPolicy` enum + `PinDefinition.empty_policy` + `ttl_ms` + 单测 |
| 修改 | `crates/core/src/cache.rs` | `OutputCache` 每 slot 加 `Arc<Notify>`；`store_*` 时 `notify_waiters`；`read` 增 `is_expired` 检查 |
| 修改 | `crates/core/src/error.rs` | `DataPinPullTimeout { upstream, pin, timeout_ms }` |
| 修改 | `crates/core/src/lib.rs` | re-export `EmptyPolicy` |
| 修改 | `src/graph/pull.rs` | 引入 `wait_for_value`（Notify + tokio::time::timeout）；按 `empty_policy` 分支；TTL 失效降级 |
| 修改 | `src/graph/pull.rs` | PURE 输入哈希记忆缓存（per-trace `DashMap<(node_id, pin_id, hash), Value>`）|
| 修改 | `crates/core/src/node.rs` | rustdoc：在 PURE capability 段加"+ pure-form + Data 输出 → 输入哈希记忆"协同章节 |
| 创建 | `tests/fixtures/empty_policy_matrix.jsonc` | 4 case：each policy × (槽空 / 已写) |
| 创建 | `crates/core/tests/empty_policy_contract.rs` | Rust 端 fixture 消费 |
| 修改 | `web/src/lib/__tests__/pin-compat.test.ts` 或新文件 | TS 端同 fixture 消费（policy 字段验证） |
| 创建 | `tests/pin_kind_phase4_fanout.rs` | 用例 4 集成：单上游 + 3 节奏不同消费者，断言上游 transform 只调 1 次 |
| 创建 | `tests/pin_kind_phase4_pure_memo.rs` | PURE 输入哈希命中：同 trace 内多次 pull 同 pure 节点 → transform 调用次数验证 |
| 创建 | `tests/pin_kind_phase4_block_timeout.rs` | `BlockUntilReady` 超时返回 `DataPinPullTimeout` |
| 创建 | `tests/pin_kind_phase4_ttl_expiry.rs` | TTL 过期降级到 `default_value` |
| 修改 | `web/src/lib/pin-compat.ts` | re-export `EmptyPolicy` 类型（来自 generated/） |
| 修改 | `web/src/components/flowgram/get-port-tooltip.ts`（或现有 tooltip helper） | tooltip 加 "空槽策略：..." 行 + "TTL：..." 行 |
| 修改 | `web/src/components/flowgram/FlowgramNodeSettingsPanel.tsx` | Data 输入引脚 settings 加 `EmptyPolicySelector` 子组件（默认 `BlockUntilReady` + 超时数字输入；高级用户切换到 `DefaultValue` / `Skip`） |
| 修改 | `crates/core/AGENTS.md` | "OutputCache + EmptyPolicy" 小节追加；空槽行为约定写明 |
| 修改 | `docs/adr/0014-执行边与数据边分离.md` | 实施进度 + spec 第十一章决策 1/2 拍板 |
| 修改 | `AGENTS.md` | ADR-0014 状态行 + 执行顺序同步 |

---

## Out of scope

1. **跨工作流 / 跨进程的缓存持久化**——Phase 4 仅 in-memory；持久化等到 ADR-0012 Phase 3 候选项或新 ADR
2. **全局 PURE 记忆缓存**（跨 trace，跨工作流）——本 Phase 仅 per-trace 命中。跨 trace 命中需要更严格的输入哈希定义和 LRU 淘汰，独立 ADR
3. **空槽时阻塞的精细 backpressure**（限制并发等待者数）——本 Phase 用 `Notify::notify_waiters` 一次性唤醒所有等待者，单 slot 写一次最多激活 N 个消费者。如果 N 极大需要 backpressure，归 ADR-0016 边级可观测性 / Phase 6 EventBus
4. **TTL 单位扩展**（除毫秒外的秒/分钟）——本 Phase 仅 `ttl_ms`；前端可派生显示
5. **空槽策略的部署期类型校验**（如 `Skip` 但下游期望 `Integer` 输入）——`Skip` 让 Data pin 可能见到 `Value::Null`，节点作者必须显式处理。Phase 4 在 pin tooltip 提示但不在部署期校验拒绝。

---

## Task 1: `EmptyPolicy` enum + `PinDefinition` 字段扩展

**Files:**
- Modify: `crates/core/src/pin.rs`
- Modify: `crates/core/src/lib.rs`

- [ ] **Step 1: 在 `crates/core/src/pin.rs` `PinKind` 之后加 `EmptyPolicy`**

```rust
/// Data 输入引脚在缓存槽空 / 过期时的兜底策略（ADR-0014 Phase 4）。
///
/// 仅对 `kind: PinKind::Data` 输入引脚有意义；输出引脚 / Exec 引脚的 `empty_policy`
/// 字段允许存在但被部署期校验忽略。
///
/// **默认 `BlockUntilReady`**：上游一定会写槽位的工业场景安全选择——延迟一拍
/// 比用陈旧默认值更接近"现场实际"。两条 escape valve：
/// - `DefaultValue(v)` 立即返回 v（dashboard 类容忍空白显示场景）
/// - `Skip` 返回 `Value::Null`，节点作者显式分支（高级用法）
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-export", derive(TS), ts(export))]
#[serde(tag = "kind", content = "value", rename_all = "snake_case")]
pub enum EmptyPolicy {
    /// 阻塞等待上游写值（默认行为，与 Phase 3 语义最接近）。
    /// 超时由 `block_timeout_ms` 字段控制（pin 级，未声明取 `DEFAULT_BLOCK_TIMEOUT_MS`）。
    BlockUntilReady,
    /// 立即返回声明的默认值——never blocks，never errors。
    DefaultValue(serde_json::Value),
    /// 立即返回 `Value::Null`，下游节点显式处理。
    Skip,
}

impl Default for EmptyPolicy {
    fn default() -> Self {
        Self::BlockUntilReady
    }
}

/// 默认阻塞等待超时（毫秒）。Phase 4 决策：5 秒。
/// 工业触发器节奏典型在亚秒到分钟，5 秒足够覆盖"上一帧未到"的瞬时空槽，
/// 又不至于把工作流卡死。pin 级 `block_timeout_ms` 可覆盖。
pub const DEFAULT_BLOCK_TIMEOUT_MS: u64 = 5_000;
```

- [ ] **Step 2: `PinDefinition` 加新字段**

定位 `pub struct PinDefinition {`，在 `description` 之后加：

```rust
    /// 空槽兜底策略（仅 Data 输入引脚有意义）。Exec 引脚此字段被忽略。
    #[serde(default)]
    pub empty_policy: EmptyPolicy,
    /// `BlockUntilReady` 模式下的等待超时毫秒数；None 取 [`DEFAULT_BLOCK_TIMEOUT_MS`].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts-export", ts(optional))]
    pub block_timeout_ms: Option<u64>,
    /// 缓存值 TTL 毫秒；None 永久。`Some(n)` 时 `produced_at + n` 后视为过期，
    /// 走 `empty_policy` 兜底。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts-export", ts(optional))]
    pub ttl_ms: Option<u64>,
```

- [ ] **Step 3: 全部 `PinDefinition::*` 工厂方法补默认 `empty_policy: EmptyPolicy::default()` + `block_timeout_ms: None` + `ttl_ms: None`**

`default_input` / `default_output` / `required_input` / `output` / `output_named_data` 五个工厂方法都加这三个字段。

- [ ] **Step 4: 加单测**

```rust
#[test]
fn empty_policy_默认是_block_until_ready() {
    assert_eq!(EmptyPolicy::default(), EmptyPolicy::BlockUntilReady);
}

#[test]
fn empty_policy_默认值序列化为_block_until_ready() {
    let v = serde_json::to_value(EmptyPolicy::default()).unwrap();
    assert_eq!(v, serde_json::json!({"kind": "block_until_ready"}));
}

#[test]
fn empty_policy_default_value_序列化携带_value() {
    let p = EmptyPolicy::DefaultValue(serde_json::json!(42));
    let v = serde_json::to_value(&p).unwrap();
    assert_eq!(v, serde_json::json!({"kind": "default_value", "value": 42}));
}

#[test]
fn pin_definition_缺_empty_policy_反序列化为默认() {
    let json = r#"{"id":"x","label":"x","pin_type":{"kind":"any"},"direction":"input","required":true,"kind":"data"}"#;
    let pin: PinDefinition = serde_json::from_str(json).unwrap();
    assert_eq!(pin.empty_policy, EmptyPolicy::BlockUntilReady);
    assert!(pin.block_timeout_ms.is_none());
    assert!(pin.ttl_ms.is_none());
}
```

- [ ] **Step 5: re-export `EmptyPolicy` + `DEFAULT_BLOCK_TIMEOUT_MS`**

```bash
grep -n "pub use pin" crates/core/src/lib.rs
```

把 `EmptyPolicy` / `DEFAULT_BLOCK_TIMEOUT_MS` 加入 re-export。

- [ ] **Step 6: 跑测试 + commit**

```bash
cargo test -p nazh-core pin
cargo test -p tauri-bindings --features ts-export export_bindings
git diff web/src/generated/  # 应有 EmptyPolicy.ts 等新文件
git add crates/core/src/pin.rs crates/core/src/lib.rs web/src/generated/
git commit -s -m "feat(core): ADR-0014 Phase 4 EmptyPolicy + TTL 字段"
```

---

## Task 2: `OutputCache` 加 `Arc<Notify>` + 过期检查

**Files:**
- Modify: `crates/core/src/cache.rs`
- Modify: `crates/core/src/error.rs` — `DataPinPullTimeout`

- [ ] **Step 1: 加 `DataPinPullTimeout` error variant**

```rust
    /// ADR-0014 Phase 4：`BlockUntilReady` 模式等上游写槽位超时。
    #[error("拉取上游 `{upstream}` 引脚 `{pin}` 超时（{timeout_ms} ms）——上游可能未执行")]
    DataPinPullTimeout {
        upstream: String,
        pin: String,
        timeout_ms: u64,
    },
```

- [ ] **Step 2: `OutputCache` 升级**

```rust
use tokio::sync::Notify;

#[derive(Debug)]
struct Slot {
    value: ArcSwap<Option<CachedOutput>>,
    notify: Arc<Notify>,
}

#[derive(Debug, Default)]
pub struct OutputCache {
    slots: DashMap<String, Arc<Slot>>,
}

impl OutputCache {
    pub fn new() -> Self { Self::default() }

    pub fn prepare_slot(&self, pin_id: &str) {
        if !self.slots.contains_key(pin_id) {
            self.slots.insert(
                pin_id.to_owned(),
                Arc::new(Slot {
                    value: ArcSwap::from_pointee(None),
                    notify: Arc::new(Notify::new()),
                }),
            );
        }
    }

    pub fn write(&self, pin_id: &str, output: CachedOutput) {
        if let Some(slot) = self.slots.get(pin_id) {
            slot.value.store(Arc::new(Some(output)));
            slot.notify.notify_waiters();
        }
    }

    pub fn write_now(&self, pin_id: &str, value: serde_json::Value, trace_id: uuid::Uuid) {
        self.write(pin_id, CachedOutput {
            value,
            produced_at: chrono::Utc::now(),
            trace_id,
        });
    }

    /// 读取最新值；若 `ttl_ms` 给出且值已过期则视为空。
    pub fn read(&self, pin_id: &str, ttl_ms: Option<u64>) -> Option<CachedOutput> {
        let slot = self.slots.get(pin_id)?;
        let snapshot = slot.value.load_full();
        let cached = (*snapshot).clone()?;
        if let Some(ttl) = ttl_ms {
            let age = chrono::Utc::now() - cached.produced_at;
            if age.num_milliseconds() as u64 > ttl {
                return None;
            }
        }
        Some(cached)
    }

    /// 拿到 slot 的 Notify 句柄——pull collector 在 `BlockUntilReady` 模式下
    /// `notified().await`。
    pub fn notify_handle(&self, pin_id: &str) -> Option<Arc<Notify>> {
        self.slots.get(pin_id).map(|slot| Arc::clone(&slot.notify))
    }

    pub fn slot_ids(&self) -> Vec<String> {
        self.slots.iter().map(|e| e.key().clone()).collect()
    }
}
```

> **注**：现有 `read(&self, pin_id) -> Option<CachedOutput>` 签名变了（加 `ttl_ms`）。所有调用点（包括 Phase 3 `pull.rs` 与现有测试）都需要传 `None`。先编译看错误，再批量改。

- [ ] **Step 3: 修补现有 `cache.rs` 单测**

`read("...")` → `read("...", None)`。

- [ ] **Step 4: 加 TTL 单测**

```rust
#[tokio::test]
async fn ttl_过期值视为空() {
    let cache = OutputCache::new();
    cache.prepare_slot("latest");
    cache.write("latest", CachedOutput {
        value: serde_json::json!(1),
        produced_at: chrono::Utc::now() - chrono::Duration::milliseconds(200),
        trace_id: uuid::Uuid::nil(),
    });

    // ttl=100ms 已过期（200ms 前写的）
    assert!(cache.read("latest", Some(100)).is_none());
    // ttl=300ms 未过期
    assert!(cache.read("latest", Some(300)).is_some());
    // 无 ttl 永远有效
    assert!(cache.read("latest", None).is_some());
}

#[tokio::test]
async fn write_唤醒等待者() {
    use tokio::time::{timeout, Duration};
    let cache = Arc::new(OutputCache::new());
    cache.prepare_slot("latest");
    let notify = cache.notify_handle("latest").unwrap();

    let cache2 = Arc::clone(&cache);
    let waiter = tokio::spawn(async move {
        notify.notified().await;
        cache2.read("latest", None)
    });

    tokio::time::sleep(Duration::from_millis(50)).await;
    cache.write_now("latest", serde_json::json!(42), uuid::Uuid::nil());

    let got = timeout(Duration::from_secs(1), waiter).await.unwrap().unwrap();
    assert!(got.is_some());
    assert_eq!(got.unwrap().value, serde_json::json!(42));
}
```

- [ ] **Step 5: 跑测试 + commit**

```bash
cargo test -p nazh-core cache
cargo test --workspace  # 验证调用点 read(_, None) 都改对了
git add crates/core/src/cache.rs crates/core/src/error.rs
git commit -s -m "feat(core): ADR-0014 Phase 4 OutputCache 加 Notify + TTL 检查"
```

---

## Task 3: `pull_data_inputs` 按 `empty_policy` 分支 + Notify 等待

**Files:**
- Modify: `src/graph/pull.rs`

- [ ] **Step 1: `pull_one` 替换"`cache.read` → 失败立即报错"逻辑**

定位 `pull_one` 函数（Phase 3 写的），把"非 pure-form 上游 / 读 cache" 分支替换为：

```rust
        } else {
            // 非 pure：按 empty_policy 分支
            let cache = output_caches_index.get(upstream_node_id).ok_or_else(|| {
                EngineError::invalid_graph(format!(
                    "上游 Exec 节点 `{upstream_node_id}` 在 output_caches_index 缺失"
                ))
            })?;
            // 找消费者声明（input pin）的 empty_policy / ttl_ms / block_timeout_ms
            // —— 此处 consumer 信息还在调用栈里，需要把这三个字段从 caller 传下来。
            // 设计：pull_one 签名加 (empty_policy, ttl_ms, block_timeout_ms) 参数，
            // 由 pull_data_inputs 在循环里查 consumer.input_pins() 后传入。
            let cached = cache.read(upstream_output_pin_id, ttl_ms);
            match cached {
                Some(c) => Ok(c.value),
                None => match empty_policy {
                    EmptyPolicy::BlockUntilReady => {
                        let timeout_ms = block_timeout_ms.unwrap_or(DEFAULT_BLOCK_TIMEOUT_MS);
                        let notify = cache.notify_handle(upstream_output_pin_id).ok_or_else(|| {
                            EngineError::invalid_graph(format!(
                                "上游 `{upstream_node_id}` 引脚 `{upstream_output_pin_id}` 槽未预分配"
                            ))
                        })?;
                        match tokio::time::timeout(
                            std::time::Duration::from_millis(timeout_ms),
                            notify.notified(),
                        ).await {
                            Ok(()) => {
                                cache.read(upstream_output_pin_id, ttl_ms)
                                    .map(|c| c.value)
                                    .ok_or(EngineError::DataPinPullTimeout {
                                        upstream: upstream_node_id.to_owned(),
                                        pin: upstream_output_pin_id.to_owned(),
                                        timeout_ms,
                                    })
                            }
                            Err(_) => Err(EngineError::DataPinPullTimeout {
                                upstream: upstream_node_id.to_owned(),
                                pin: upstream_output_pin_id.to_owned(),
                                timeout_ms,
                            }),
                        }
                    }
                    EmptyPolicy::DefaultValue(v) => Ok(v.clone()),
                    EmptyPolicy::Skip => Ok(serde_json::Value::Null),
                },
            }
        }
```

- [ ] **Step 2: `pull_data_inputs` 循环：从 consumer.input_pins() 查每 pin 的 policy**

```rust
pub(crate) async fn pull_data_inputs(
    consumer_node: &dyn NodeTrait,  // ← 改：从 (id) 升级为 (&dyn NodeTrait) 以查 input_pins
    consumer_node_id: &str,
    exec_payload: Value,
    edges_by_consumer: &EdgesByConsumer,
    nodes_index: &HashMap<String, Arc<dyn NodeTrait>>,
    output_caches_index: &HashMap<String, Arc<OutputCache>>,
    trace_id: Uuid,
    pure_memo: &PureMemo,  // ← Task 4 加
) -> Result<Value, EngineError> {
    let entries = edges_by_consumer.for_consumer(consumer_node_id);
    if entries.is_empty() {
        return Ok(exec_payload);
    }

    let consumer_pins: HashMap<String, PinDefinition> = consumer_node
        .input_pins()
        .into_iter()
        .map(|p| (p.id.clone(), p))
        .collect();

    let mut data_values: Map<String, Value> = Map::new();
    for entry in entries {
        let consumer_pin = consumer_pins
            .get(&entry.consumer_input_pin_id)
            .ok_or_else(|| EngineError::invalid_graph(format!(
                "consumer pin `{}` 未在节点 input_pins 声明",
                entry.consumer_input_pin_id
            )))?;
        let upstream_value = pull_one(
            &entry.upstream_node_id,
            &entry.upstream_output_pin_id,
            nodes_index,
            output_caches_index,
            edges_by_consumer,
            trace_id,
            consumer_pin.empty_policy.clone(),
            consumer_pin.block_timeout_ms,
            consumer_pin.ttl_ms,
            pure_memo,
        ).await?;
        data_values.insert(entry.consumer_input_pin_id.clone(), upstream_value);
    }

    Ok(merge_payload(exec_payload, data_values))
}
```

`pull_one` 同步加 4 个参数（policy/timeout/ttl/memo）。

- [ ] **Step 3: `runner.rs` 调用点同步（Phase 3 写的处）**

```rust
let payload = match super::pull::pull_data_inputs(
    node.as_ref(),  // ← 新加
    &node_id,
    payload,
    &edges_by_consumer,
    &nodes_index,
    &output_caches_index,
    trace_id,
    &pure_memo,  // ← Task 4 加
).await { ... }
```

`pure_memo` 由 deploy.rs 创建并传入 run_node（per-deployment 单例，Arc 共享）。

- [ ] **Step 4: 集成测试 — `BlockUntilReady` 等到上游写**

`tests/pin_kind_phase4_block_wait.rs`（Step 5 task 单独建）

- [ ] **Step 5: 集成测试 — `DefaultValue` 立即返回**

`tests/pin_kind_phase4_default_value.rs`（Step 5 task 单独建）

> 这两个集成测试统一放进 Task 6 的 `tests/pin_kind_phase4_block_timeout.rs` + `tests/pin_kind_phase4_ttl_expiry.rs` 套件。本 Task 只确保编译 + 现有测试不退化。

- [ ] **Step 6: 跑现有测试**

```bash
cargo test --workspace
```

Expected: Phase 3 测试（`pin_kind_phase3.rs`）仍 PASS（默认 `BlockUntilReady` + 5s 超时，原测试上游肯定先写所以不会触发等待）。

- [ ] **Step 7: commit**

```bash
git add src/graph/pull.rs src/graph/runner.rs src/graph/deploy.rs
git commit -s -m "feat(graph): ADR-0014 Phase 4 pull_data_inputs 按 empty_policy 分支"
```

---

## Task 4: PURE 输入哈希记忆缓存（per-trace）

**Files:**
- Modify: `src/graph/pull.rs` — `PureMemo` 结构 + `pull_one` 命中检查
- Modify: `src/graph/deploy.rs` — 每次 deploy 创建 `PureMemo`

- [ ] **Step 1: 在 `pull.rs` 头部加 `PureMemo`**

```rust
use dashmap::DashMap;
use std::sync::Arc;

/// PURE pure-form 节点的"输入哈希 → 输出"记忆缓存（per-trace）。
///
/// 命中条件：上游节点 (a) `is_pure_form` 为真，且 (b) `capabilities()` 含
/// `NodeCapabilities::PURE`。后者是节点作者的"同输入同输出 + 无副作用"承诺；
/// 前者保证它走 pull 路径而非 spawn 路径。
///
/// 键：`(node_id, output_pin_id, trace_id, input_payload_hash)`
/// 值：`Value`（pure 节点对应输出引脚的值）
///
/// **per-trace 范围**：不同 trace 不共享。理由 (a) 上游 cache 槽可能在 trace 间
/// 更新，跨 trace 命中可能用陈旧值；(b) trace 结束时记忆自动随 ContextRef 释放。
/// 跨 trace 全局记忆需要更严格的 invalidation，归未来 ADR。
pub(crate) struct PureMemo {
    table: DashMap<MemoKey, Value>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct MemoKey {
    node_id: String,
    pin_id: String,
    trace_id: Uuid,
    input_hash: u64,
}

impl PureMemo {
    pub fn new() -> Self {
        Self { table: DashMap::new() }
    }

    fn key(node_id: &str, pin_id: &str, trace_id: Uuid, payload: &Value) -> MemoKey {
        use std::hash::{Hash, Hasher};
        // 对 payload 做稳定 hash —— 用 serde_json 序列化后的字节哈希
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        let serialized = serde_json::to_string(payload).unwrap_or_default();
        serialized.hash(&mut hasher);
        MemoKey {
            node_id: node_id.to_owned(),
            pin_id: pin_id.to_owned(),
            trace_id,
            input_hash: hasher.finish(),
        }
    }

    pub fn get(&self, node_id: &str, pin_id: &str, trace_id: Uuid, input: &Value) -> Option<Value> {
        let k = Self::key(node_id, pin_id, trace_id, input);
        self.table.get(&k).map(|v| v.clone())
    }

    pub fn put(&self, node_id: &str, pin_id: &str, trace_id: Uuid, input: &Value, output: Value) {
        let k = Self::key(node_id, pin_id, trace_id, input);
        self.table.insert(k, output);
    }
}
```

- [ ] **Step 2: 在 `pull_one` pure-form 分支加 memoization**

```rust
        if is_pure_form(upstream.as_ref()) {
            // 先组装上游 input payload
            let upstream_payload = pull_data_inputs(
                upstream.as_ref(),
                upstream_node_id,
                Value::Object(Map::new()),
                edges_by_consumer,
                nodes_index,
                output_caches_index,
                trace_id,
                pure_memo,
            ).await?;

            // PURE 命中检查
            let pure_capable = upstream.capabilities().contains(NodeCapabilities::PURE);
            if pure_capable {
                if let Some(memoized) = pure_memo.get(
                    upstream_node_id,
                    upstream_output_pin_id,
                    trace_id,
                    &upstream_payload,
                ) {
                    tracing::trace!(
                        node_id = upstream_node_id,
                        pin_id = upstream_output_pin_id,
                        "PURE memo hit"
                    );
                    return Ok(memoized);
                }
            }

            // 未命中 → 求值 + 写记忆
            let result = upstream.transform(trace_id, upstream_payload.clone()).await?;
            let value = extract_pin_value(&result, upstream_output_pin_id)
                .ok_or(EngineError::DataPinCacheEmpty {
                    upstream: upstream_node_id.to_owned(),
                    pin: upstream_output_pin_id.to_owned(),
                })?;

            if pure_capable {
                pure_memo.put(
                    upstream_node_id,
                    upstream_output_pin_id,
                    trace_id,
                    &upstream_payload,
                    value.clone(),
                );
            }
            Ok(value)
        }
```

把 Phase 3 的 "find matching output payload" 抽成 `extract_pin_value` 私有 helper。

- [ ] **Step 3: deploy.rs 创建 PureMemo 并传入**

```rust
    let pure_memo = std::sync::Arc::new(super::pull::PureMemo::new());
    // 在 spawn run_node 时传入
```

`run_node` 签名增 `pure_memo: Arc<PureMemo>` 参数。

- [ ] **Step 4: commit**

```bash
git add src/graph/pull.rs src/graph/deploy.rs src/graph/runner.rs
git commit -s -m "feat(graph): ADR-0014 Phase 4 PURE 节点输入哈希记忆（per-trace）"
```

---

## Task 5: 跨语言 fixture — `empty_policy` 4 case

**Files:**
- Create: `tests/fixtures/empty_policy_matrix.jsonc`
- Create: `crates/core/tests/empty_policy_contract.rs`

- [ ] **Step 1: fixture**

```jsonc
// ADR-0014 Phase 4：EmptyPolicy 序列化 + 反序列化合约。
//
// 4 case：每种 policy 各一例，外加 BlockUntilReady 默认 fallback 1 例。
[
  {
    "name": "BlockUntilReady",
    "json": {"kind": "block_until_ready"},
    "is_block": true,
    "is_default_value": false,
    "is_skip": false
  },
  {
    "name": "DefaultValue 标量",
    "json": {"kind": "default_value", "value": 42},
    "is_block": false,
    "is_default_value": true,
    "is_skip": false,
    "default_value": 42
  },
  {
    "name": "DefaultValue 对象",
    "json": {"kind": "default_value", "value": {"a": 1, "b": "x"}},
    "is_block": false,
    "is_default_value": true,
    "is_skip": false,
    "default_value": {"a": 1, "b": "x"}
  },
  {
    "name": "Skip",
    "json": {"kind": "skip"},
    "is_block": false,
    "is_default_value": false,
    "is_skip": true
  }
]
```

- [ ] **Step 2: Rust contract**

```rust
//! `empty_policy_matrix.jsonc` 跨语言契约（Rust 端）。

#![allow(clippy::unwrap_used)]

use nazh_core::EmptyPolicy;
use serde::Deserialize;
use serde_json::Value;

#[derive(Deserialize)]
struct Case {
    name: String,
    json: Value,
    is_block: bool,
    is_default_value: bool,
    is_skip: bool,
    #[serde(default)]
    default_value: Option<Value>,
}

#[test]
fn empty_policy_fixture_穷尽_4_case() {
    let raw = std::fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../tests/fixtures/empty_policy_matrix.jsonc"),
    ).unwrap();
    let stripped: String = raw
        .lines()
        .map(|l| if let Some(idx) = l.find("//") { &l[..idx] } else { l })
        .collect::<Vec<_>>()
        .join("\n");
    let cases: Vec<Case> = serde_json::from_str(&stripped).unwrap();
    assert_eq!(cases.len(), 4);

    for case in cases {
        let p: EmptyPolicy = serde_json::from_value(case.json.clone()).unwrap();
        assert_eq!(matches!(p, EmptyPolicy::BlockUntilReady), case.is_block, "case {}", case.name);
        let is_dv = matches!(p, EmptyPolicy::DefaultValue(_));
        assert_eq!(is_dv, case.is_default_value, "case {}", case.name);
        if let EmptyPolicy::DefaultValue(ref v) = p {
            assert_eq!(v, case.default_value.as_ref().unwrap(), "case {}", case.name);
        }
        assert_eq!(matches!(p, EmptyPolicy::Skip), case.is_skip, "case {}", case.name);
        // 往返
        let round = serde_json::to_value(&p).unwrap();
        assert_eq!(round, case.json, "case {} round-trip", case.name);
    }
}
```

- [ ] **Step 3: 跑 + commit**

```bash
cargo test --test empty_policy_contract
git add tests/fixtures/empty_policy_matrix.jsonc crates/core/tests/empty_policy_contract.rs
git commit -s -m "test(core): ADR-0014 Phase 4 EmptyPolicy 跨语言 fixture"
```

---

## Task 6: 集成测试 — `BlockUntilReady` 超时 / TTL 过期 / DefaultValue 立即

**Files:**
- Create: `tests/pin_kind_phase4_block_timeout.rs`
- Create: `tests/pin_kind_phase4_ttl_expiry.rs`
- Create: `tests/pin_kind_phase4_default_value.rs`

- [ ] **Step 1: `tests/pin_kind_phase4_block_timeout.rs`** — 上游永不 transform，下游 `BlockUntilReady + block_timeout_ms=200`，断言 `DataPinPullTimeout`

(完整代码参考 Phase 3 集成测试模板，新加测试节点：上游不写 cache、下游声明 Data 输入 + 200ms 超时；用 `EngineError::DataPinPullTimeout` matches)

- [ ] **Step 2: `tests/pin_kind_phase4_ttl_expiry.rs`** — 上游写 cache 后等 200ms，下游 ttl=100ms + DefaultValue("stale") 拉到默认值

- [ ] **Step 3: `tests/pin_kind_phase4_default_value.rs`** — 槽空 + DefaultValue(42) 立即返回 42

- [ ] **Step 4: 跑 + commit**

```bash
cargo test --test pin_kind_phase4_block_timeout --test pin_kind_phase4_ttl_expiry --test pin_kind_phase4_default_value
git add tests/pin_kind_phase4_*.rs
git commit -s -m "test(adr-0014): Phase 4 BlockUntilReady 超时 / TTL 过期 / DefaultValue 立即返回"
```

---

## Task 7: 集成测试 — 用例 4 旁路 fan-out（上游 transform 仅 1 次）

**Files:**
- Create: `tests/pin_kind_phase4_fanout.rs`

- [ ] **Step 1: 测试搭建**

```rust
//! ADR-0014 Phase 4 用例 4：modbusRead.latest 旁路 fan-out 给三个不同节奏消费者，
//! 断言上游 transform 仅被调用 1 次（fan-out 不重复 IO）。

#![allow(clippy::unwrap_used)]

// 三个消费者：
// - dashboard：100ms 节奏（独立 timer 触发，pull modbusRead.latest）
// - historySampler：5min（测试里改为 250ms）
// - alertGate：每次 source 触发都拉
//
// 单 source 触发 N 次，断言 source.transform_count == N（不被三个消费者放大）。
//
// 用 stub: source 节点自带原子计数器，每 transform 自增；测试结束断言 == N。
```

完整代码参考 Phase 3 + Phase 3b 集成测试模板。关键 stub：

```rust
struct CountingSource {
    id: String,
    counter: Arc<AtomicUsize>,
}

#[async_trait]
impl NodeTrait for CountingSource {
    // input_pins: default
    // output_pins: default Exec out + Data named "latest"
    async fn transform(&self, _: Uuid, _: Value) -> Result<NodeExecution, EngineError> {
        let n = self.counter.fetch_add(1, Ordering::SeqCst);
        Ok(NodeExecution::single(json!({ "tick": n })))
    }
}
```

trigger source N 次（如 5 次），等所有消费者完成，断言 `counter.load(SeqCst) == 5`（即使有 3 个 Data 拉取消费者）。

- [ ] **Step 2: 跑 + commit**

```bash
cargo test --test pin_kind_phase4_fanout
git add tests/pin_kind_phase4_fanout.rs
git commit -s -m "test(adr-0014): Phase 4 用例 4 旁路 fan-out（上游 transform 仅 1 次）"
```

---

## Task 8: 集成测试 — PURE memoization 命中

**Files:**
- Create: `tests/pin_kind_phase4_pure_memo.rs`

- [ ] **Step 1: 测试搭建**

```rust
//! ADR-0014 Phase 4：PURE pure-form 节点输入哈希记忆——同 trace 内多次 pull
//! 同 (节点, 输入) → transform 只调一次。

// stub: CountingC2f（c2f 行为 + 计数）
// 图：source(Exec out → sink.in)（推 source 一次）
//     source(Data value → c2f.value)
//     c2f(Data out → sink.temp1)（拉 c2f 第一次）
//     c2f(Data out → sink.temp2)（拉 c2f 第二次，应该命中 memo）
//
// 即同一 trace 内，sink 有两个 Data 输入都从 c2f 拉相同输出引脚。
// 断言：c2f.transform_count == 1（命中 memo）+ sink 收到 temp1 == temp2。
```

```rust
struct CountingC2f {
    id: String,
    counter: Arc<AtomicUsize>,
}

#[async_trait]
impl NodeTrait for CountingC2f {
    fn id(&self) -> &str { &self.id }
    fn kind(&self) -> &str { "countingC2f" }
    fn input_pins(&self) -> Vec<PinDefinition> {
        vec![PinDefinition {
            id: "value".to_owned(), label: "value".to_owned(),
            pin_type: PinType::Float, direction: PinDirection::Input,
            required: true, kind: PinKind::Data,
            empty_policy: EmptyPolicy::default(), block_timeout_ms: None, ttl_ms: None,
            description: None,
        }]
    }
    fn output_pins(&self) -> Vec<PinDefinition> {
        vec![PinDefinition::output_named_data("out", "out", PinType::Float, "f")]
    }
    fn capabilities(&self) -> NodeCapabilities { NodeCapabilities::PURE }
    async fn transform(&self, _: Uuid, p: Value) -> Result<NodeExecution, EngineError> {
        self.counter.fetch_add(1, Ordering::SeqCst);
        let c = p.get("value").and_then(Value::as_f64).unwrap_or(0.0);
        Ok(NodeExecution::single(json!({ "out": c * 9.0 / 5.0 + 32.0 })))
    }
}
```

- [ ] **Step 2: 跑 + commit**

```bash
cargo test --test pin_kind_phase4_pure_memo
git add tests/pin_kind_phase4_pure_memo.rs
git commit -s -m "test(adr-0014): Phase 4 PURE 节点 per-trace 记忆命中"
```

---

## Task 9: 前端 pin tooltip + EmptyPolicySelector 设置面板

**Files:**
- Modify: `web/src/components/flowgram/get-port-tooltip.ts`
- Modify: `web/src/components/flowgram/FlowgramNodeSettingsPanel.tsx`
- Modify: `web/src/lib/pin-compat.ts` — re-export EmptyPolicy

- [ ] **Step 1: pin tooltip 加 "空槽策略" + "TTL" 行**

```typescript
function formatEmptyPolicy(policy: EmptyPolicy | undefined): string | null {
  if (!policy || policy.kind === 'block_until_ready') return '阻塞等待上游';
  if (policy.kind === 'default_value') return `默认值：${JSON.stringify(policy.value)}`;
  if (policy.kind === 'skip') return '跳过（返回 null）';
  return null;
}

function formatTtl(ttlMs: number | undefined): string | null {
  if (!ttlMs) return null;
  return `TTL：${ttlMs} ms`;
}

// getPortTooltip 内部追加：
if (pin.kind === 'data' && pin.direction === 'input') {
  const policyLine = formatEmptyPolicy(pin.empty_policy);
  if (policyLine) lines.push(`空槽策略：${policyLine}`);
  const ttlLine = formatTtl(pin.ttl_ms);
  if (ttlLine) lines.push(ttlLine);
}
```

- [ ] **Step 2: settings panel `EmptyPolicySelector` 子组件（仅对 Data 输入引脚显示）**

```tsx
function EmptyPolicySelector({
  policy,
  blockTimeoutMs,
  onChange,
}: {
  policy: EmptyPolicy;
  blockTimeoutMs?: number;
  onChange: (policy: EmptyPolicy, blockTimeoutMs?: number) => void;
}) {
  const kind = policy.kind ?? 'block_until_ready';
  return (
    <div className="empty-policy-selector">
      <label>空槽策略</label>
      <select
        value={kind}
        onChange={(e) => {
          if (e.target.value === 'block_until_ready') onChange({ kind: 'block_until_ready' });
          if (e.target.value === 'default_value') onChange({ kind: 'default_value', value: null });
          if (e.target.value === 'skip') onChange({ kind: 'skip' });
        }}
      >
        <option value="block_until_ready">阻塞等待</option>
        <option value="default_value">默认值</option>
        <option value="skip">跳过</option>
      </select>
      {kind === 'block_until_ready' && (
        <input
          type="number"
          placeholder="超时 ms (默认 5000)"
          defaultValue={blockTimeoutMs}
          onBlur={(e) => onChange(policy, Number(e.target.value) || undefined)}
        />
      )}
      {kind === 'default_value' && (
        <input
          type="text"
          placeholder='默认值 JSON (如 42 / "x" / {"k":1})'
          defaultValue={
            'value' in policy ? JSON.stringify((policy as { value: unknown }).value) : ''
          }
          onBlur={(e) => {
            try { onChange({ kind: 'default_value', value: JSON.parse(e.target.value) }); }
            catch { onChange({ kind: 'default_value', value: e.target.value }); }
          }}
        />
      )}
    </div>
  );
}
```

- [ ] **Step 3: 接入设置面板（仅 Data 输入引脚）**

定位 settings panel 主组件渲染 input pin 列表的循环，每行 Data 输入引脚下方挂 `<EmptyPolicySelector>`。

- [ ] **Step 4: commit**

```bash
git add web/src/components/flowgram/get-port-tooltip.ts web/src/components/flowgram/FlowgramNodeSettingsPanel.tsx web/src/lib/pin-compat.ts
git commit -s -m "feat(web): ADR-0014 Phase 4 EmptyPolicySelector + pin tooltip 显示空槽策略 / TTL"
```

---

## Task 10: 文档更新 + spec 决策回写

**Files:**
- Modify: `docs/adr/0014-执行边与数据边分离.md` — 实施进度 + spec 第十一章决策 1/2 拍板回写
- Modify: `docs/superpowers/specs/2026-04-28-pin-kind-exec-data-design.md` — 第十一章勾掉决策 1/2
- Modify: `crates/core/AGENTS.md` — `OutputCache` + `EmptyPolicy` 小节
- Modify: `AGENTS.md` — ADR-0014 状态行 + 执行顺序

- [ ] **Step 1: 在 ADR 文档实施进度章节加 Phase 4**

```markdown
- ✅ **Phase 4（YYYY-MM-DD）**：缓存生命周期与策略落地——`PinDefinition.empty_policy`
  三态（`BlockUntilReady` 默认 / `DefaultValue(v)` / `Skip`）+ `block_timeout_ms`
  + `ttl_ms`。`OutputCache` 加 `Arc<Notify>` 让 `BlockUntilReady` 通过
  `notified().await + tokio::time::timeout` 实现。`EngineError::DataPinPullTimeout`
  覆盖等不到值的场景。`PureMemo`（per-trace）在 pull 路径上对 PURE pure-form
  上游做输入哈希记忆，同 trace 内多次拉同 (节点, pin, 输入) 命中跳过 transform。
  跨语言 fixture `tests/fixtures/empty_policy_matrix.jsonc` 4 case 穷尽。
  集成测试覆盖 `BlockUntilReady` 超时 / TTL 过期 / DefaultValue 立即 / 用例 4
  fan-out 上游单调用 / PURE memoization 命中 五个核心场景。前端 pin tooltip
  显示策略 + TTL，settings panel 加 `EmptyPolicySelector` 子组件。

  **Spec 第十一章决策回写**：
  - 决策 1：缓存空槽默认行为 → **`BlockUntilReady`**（理由：工业现场延迟一拍 > 用陈旧默认值）
  - 决策 2：是否引入 TTL → **引入 `ttl_ms: Option<u64>`，默认 None**（与 `BlockUntilReady` 协同表达"非陈旧"意图）
```

- [ ] **Step 2: spec 文档第十一章勾掉决策 1/2**

定位 `## 十一、待审定问题`，在决策 1/2 行尾加：

```markdown
- ~~**Phase 4 决策**：缓存空槽兜底策略的默认行为（block / skip / default_value）~~ → 已拍板 `BlockUntilReady`（Phase 4 plan）
- ~~**Phase 4 决策**：是否引入 TTL（"超过 N 秒未更新视为过期"）~~ → 已拍板引入 `ttl_ms: Option<u64>`（Phase 4 plan）
```

- [ ] **Step 3: `crates/core/AGENTS.md` 加 `OutputCache + EmptyPolicy` 小节**

documenting 三态语义 + `Notify` 唤醒模式 + 节点作者契约（`Skip` 时 Data pin 可见 `Value::Null`）。

- [ ] **Step 4: AGENTS.md 状态行**

```markdown
- ADR-0014（执行边与数据边分离 → 重命名为「引脚求值语义二分」）— **已实施 Phase 1+2+3+3b+4**（YYYY-MM-DD）。Phase 4：缓存生命周期（`empty_policy` / `ttl_ms` / `BlockUntilReady` 默认 + Notify 唤醒）+ PURE per-trace 记忆。Spec 第十一章决策 1/2 拍板回写。剩余 Phase 5（视觉打磨）。
```

ADR Execution Order #8 同步。

- [ ] **Step 5: 全量验证 + commit**

```bash
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all -- --check
npm --prefix web run test
git add docs/adr/0014-执行边与数据边分离.md docs/superpowers/specs/2026-04-28-pin-kind-exec-data-design.md crates/core/AGENTS.md AGENTS.md
git commit -s -m "docs(adr-0014): Phase 4 落地后状态同步 + spec 决策回写"
```

---

## Self-Review

### Spec coverage

- ✅ 空槽兜底引脚级声明（`block_until_ready` / `default_value` / `skip`）—— Task 1 + Task 3
- ✅ TTL 策略 —— Task 1 + Task 2
- ✅ ADR-0011 PURE 缓存与引脚二分协同（PURE + Data 输出 = 输入哈希） —— Task 4 + Task 8
- ✅ 用例 4（旁路 fan-out）真实可运行 —— Task 7

### Spec 第十一章决策拍板

- ✅ 决策 1（空槽默认行为 → BlockUntilReady）—— Task 1 + Task 10 回写
- ✅ 决策 2（TTL 引入 → ttl_ms: Option<u64>）—— Task 1 + Task 10 回写

### Placeholder scan

- 已检：所有代码块给实际实现；Task 6 三个测试文件的具体代码留给实施者按 Task 7 的搭建模板补全（已说明结构、stub 类型、断言点）
- 没有 "TODO / similar to" 等懒散语言

### Type consistency

- `EmptyPolicy { BlockUntilReady, DefaultValue(Value), Skip }` —— Task 1 / Task 3 / Task 5 / Task 9 一致
- `PinDefinition.empty_policy / block_timeout_ms / ttl_ms` —— Task 1 / Task 3 / Task 9 一致
- `EngineError::DataPinPullTimeout { upstream, pin, timeout_ms }` —— Task 2 / Task 3 / Task 6 一致
- `PureMemo` API：`new()` / `get()` / `put()` —— Task 4 内部一致；调用点 Task 4 + Task 8

### 已知风险

- **Task 2 read 签名变更（加 ttl_ms 参数）**：所有调用点（包括 Phase 1+2 测试）必须改。先编译看错误清单再批量改。
- **Task 4 PureMemo 用 hashmap default hasher**：抗碰撞性弱但用于内部 cache 命中可接受；安全敏感场景再换 `ahash` / `xxhash`。
- **Task 4 输入哈希基于 serde_json 序列化字符串**：Object 字段顺序可能不稳定（旧 serde_json 不保证 key 排序）。当前 `serde_json` 默认 `preserve_order` 关闭，所以 sort by key 自动稳定。如果未来开启 `preserve_order` 特性需要手动排序。
- **Task 7 用例 4 fan-out 测试时序敏感**：3 个消费者并发拉同一槽，要等全部完成后断言计数。需要 `tokio::join!` 或显式 channel sync 做 barrier。

---

## Implementation note

每条 task 单 commit，sign-off + 中文 commit msg。Phase 4 预期 10 commits。**前置**：Phase 3（pull 路径）必须已落地。Phase 4 不依赖 Phase 3b，可与 Phase 3b 任意顺序推进；但 Phase 3b 完整化用例 3，Phase 4 完整化用例 4，建议先 3b 再 4 让 spec 用例覆盖度逐步铺满。
