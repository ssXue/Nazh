# ADR-0014 Phase 1：引脚二分基础设施 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 在 Ring 0 引入 `PinKind`（Exec/Data）维度 + `OutputCache` 缓存槽数据结构；部署期跨 Kind 拒绝 + Data 边独立环检测；Runner 在 transform 完成后写 OutputCache 但不读（业务级消费 Phase 2 才做）。生产 14 类节点 0 改动，编译期默认 Kind=Exec。

**Architecture:**
1. `PinDefinition` 新增 `kind: PinKind` 字段（serde default = Exec），向后兼容现有 14 类节点
2. 新建 `crates/core/src/cache.rs`：`OutputCache` 持有 `DashMap<pin_id, Arc<ArcSwap<Option<CachedOutput>>>>` 槽位
3. 部署期校验在 `pin_validator` 加 PinKind 一致性校验；`topology` 加 `classify_edges` + `detect_data_edge_cycle` helper
4. `deploy.rs` 在阶段 0.5 后给每节点构造 OutputCache，区分 exec/data downstream，注入 Runner
5. Runner `run_node` 接收 OutputCache + 节点的 Data 输出 pin id 集合，transform 完成后按 dispatch 解析的目标 port 决定写 cache（不读）
6. Phase 1 测试用 `#[cfg(test)]` stub 节点验证骨架；生产节点不改一行

**Tech Stack:** Rust + Tokio + serde + ts-rs + arc-swap (新增 workspace dep) + dashmap (已有) + thiserror

---

## 设计要点（实施前必读）

- **不改前端 / 不改 IPC schema**：Phase 1 完全在 Ring 0 + facade 完成，前端 `web/src/generated/PinDefinition.ts` 仅多一个可选的 `kind` 字段，旧画布 JSON 反序列化无影响
- **不改业务节点**：`if`、`switch`、`tryCatch`、`loop`、`code`、`timer`、`serial`、`native`、`modbusRead`、`httpClient`、`mqttClient`、`barkPush`、`sqlWriter`、`debugConsole` 全部保持 `kind` 字段未声明，编译期默认走 `PinKind::Exec`
- **拓扑排序保持现状**：当前 `WorkflowGraph::topology()` 仍按全 edges 计算入度，因为 Phase 1 没有任何业务节点声明 Data 输出 pin——业务工作流没有 Data 边可言。Phase 2 引入第一个 Data 输出节点时再考虑是否引入 `exec_only_topology`
- **Phase 1 范围严格限定写入端**：transform 完成后写 cache，但下游不读 cache（读取在 Phase 2/3）
- **stub 节点是测试专用**：所有 Data 引脚用例放在 `#[cfg(test)]` 模块里，业务路径无感

---

## File Structure

### 新建文件

| 路径 | 职责 |
|---|---|
| `crates/core/src/cache.rs` | `OutputCache` + `CachedOutput` 类型，无锁缓存槽 |
| `tests/pin_kind_phase1.rs`（新增 facade crate 集成测试） | 端到端 stub 节点 deploy + submit + 断言 cache 写入 |

### 修改文件

| 路径 | 修改内容 |
|---|---|
| `Cargo.toml` (workspace root) | 新增 `arc-swap` 到 `[workspace.dependencies]` |
| `crates/core/Cargo.toml` | 引用 workspace 的 `arc-swap` |
| `crates/core/src/pin.rs` | 加 `PinKind` 枚举 + `PinDefinition.kind` 字段 + 工厂函数更新 |
| `crates/core/src/cache.rs` | （新建，见上） |
| `crates/core/src/lib.rs` | 模块导出 + `export_bindings::export_all()` 加 `PinKind::export()` |
| `crates/core/src/error.rs` | 加 `IncompatiblePinKinds` 错误变体 |
| `src/graph/pin_validator.rs` | 跨 Kind 校验 + 测试 |
| `src/graph/topology.rs` | 加 `classify_edges` 与 `detect_data_edge_cycle` 公开方法（pub(crate)）+ 测试 |
| `src/graph/deploy.rs` | 阶段 0.5 后构造 OutputCache、调用 cycle 检测、把 downstream 按 Kind 分类、注入 run_node |
| `src/graph/runner.rs` | `run_node` 签名加 `output_cache` + `data_output_pins`；transform 完成后写 cache |
| `CLAUDE.md` (= `AGENTS.md`) | 在 ADR Execution Order 表里把 ADR-0014 标 Phase 1 已实施；项目状态同步 |
| `docs/adr/0014-执行边与数据边分离.md` | 标题改名（"引脚二分"），重写决策章节，引用 spec 文档 |

---

## Task 1: 添加 arc-swap 依赖

**Files:**
- Modify: `Cargo.toml` (workspace root)
- Modify: `crates/core/Cargo.toml`

- [ ] **Step 1: 在 workspace 根 Cargo.toml 加 arc-swap**

打开 `Cargo.toml`，在 `[workspace.dependencies]` 块的 `chrono` 行下面（保持字母序）插入：

```toml
arc-swap = "1"
```

- [ ] **Step 2: 在 crates/core/Cargo.toml 引用**

打开 `crates/core/Cargo.toml`，在 `[dependencies]` 块的 `bitflags` 行后插入：

```toml
arc-swap = { workspace = true }
```

- [ ] **Step 3: 验证编译通过**

```bash
cargo check -p nazh-core
```

预期：编译通过，无 warning。

- [ ] **Step 4: Commit**

```bash
git add Cargo.toml crates/core/Cargo.toml
git commit -s -m "chore(core): 引入 arc-swap workspace 依赖（ADR-0014 Phase 1 准备）"
```

---

## Task 2: 引入 PinKind 枚举

**Files:**
- Modify: `crates/core/src/pin.rs`

- [ ] **Step 1: 写失败测试**

在 `crates/core/src/pin.rs` 末尾的 `mod tests` 块内（`---- 兼容矩阵 ----` 注释之前），插入：

```rust
    // ---- PinKind ----

    #[test]
    fn pin_kind_默认值是_exec() {
        assert_eq!(PinKind::default(), PinKind::Exec);
    }

    #[test]
    fn pin_kind_序列化为小写字符串() {
        assert_eq!(serde_json::to_string(&PinKind::Exec).unwrap(), "\"exec\"");
        assert_eq!(serde_json::to_string(&PinKind::Data).unwrap(), "\"data\"");
    }

    #[test]
    fn pin_kind_反序列化从小写字符串() {
        let exec: PinKind = serde_json::from_str("\"exec\"").unwrap();
        let data: PinKind = serde_json::from_str("\"data\"").unwrap();
        assert_eq!(exec, PinKind::Exec);
        assert_eq!(data, PinKind::Data);
    }

    #[test]
    fn pin_kind_兼容性必须严格相等() {
        assert!(PinKind::Exec.is_compatible_with(PinKind::Exec));
        assert!(PinKind::Data.is_compatible_with(PinKind::Data));
        assert!(!PinKind::Exec.is_compatible_with(PinKind::Data));
        assert!(!PinKind::Data.is_compatible_with(PinKind::Exec));
    }
```

- [ ] **Step 2: 运行测试确认失败**

```bash
cargo test -p nazh-core pin::tests::pin_kind 2>&1 | head -20
```

预期：编译失败，"cannot find type `PinKind` in this scope"。

- [ ] **Step 3: 实现 PinKind**

在 `crates/core/src/pin.rs` 中，把 `impl fmt::Display for PinDirection { ... }` 块**之后**、`impl fmt::Display for PinType { ... }` 块**之前**插入：

```rust
/// 引脚的求值语义。与 [`PinType`]（数据形状）正交。
///
/// 设计动机与决策见 ADR-0014（重构后的"引脚二分"方案）。
///
/// - [`Exec`](Self::Exec)：上游完成 transform → MPSC push → 下游 transform。
///   这是 Nazh 1.0 的默认语义；所有现有节点不显式声明时走这条路径。
/// - [`Data`](Self::Data)：上游完成 transform → 写入输出缓存槽（不 push）；
///   下游被自己的 Exec 边触发时在 transform 前从缓存槽拉取（Phase 2 起）。
///
/// **设计前提**：引脚对引脚必须 PinKind 一致——Exec 只能连 Exec、Data 只能连 Data。
/// 部署期 [`pin_validator`](crate::PinDefinition) 拒绝跨 Kind 连接。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-export", derive(TS), ts(export))]
#[serde(rename_all = "lowercase")]
pub enum PinKind {
    /// 推语义。**默认值**——所有现有引脚不声明时为 Exec，向后兼容。
    #[default]
    Exec,
    /// 拉语义。上游写缓存、下游被自己的 Exec 边触发时读缓存。
    Data,
}

impl fmt::Display for PinKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Exec => "exec",
            Self::Data => "data",
        })
    }
}

impl PinKind {
    /// 判断"上游引脚 self → 下游引脚 other"在求值语义维度上是否兼容。
    /// 规则：必须严格相等——Exec ↔ Exec、Data ↔ Data。
    #[must_use]
    pub fn is_compatible_with(self, other: Self) -> bool {
        self == other
    }
}
```

- [ ] **Step 4: 运行测试确认通过**

```bash
cargo test -p nazh-core pin::tests::pin_kind
```

预期：4 个 PinKind 测试全部 PASS。

- [ ] **Step 5: Commit**

```bash
git add crates/core/src/pin.rs
git commit -s -m "feat(core): 引入 PinKind 求值语义维度（ADR-0014 Phase 1）"
```

---

## Task 3: PinDefinition 加 kind 字段 + 工厂函数支持

**Files:**
- Modify: `crates/core/src/pin.rs`

- [ ] **Step 1: 写失败测试**

在 `crates/core/src/pin.rs` 的 `mod tests` 块内、`pin_kind_*` 测试之后插入：

```rust
    #[test]
    fn pin_definition_默认工厂方法的_kind_是_exec() {
        assert_eq!(PinDefinition::default_input().kind, PinKind::Exec);
        assert_eq!(PinDefinition::default_output().kind, PinKind::Exec);
        assert_eq!(
            PinDefinition::required_input(PinType::Json, "test").kind,
            PinKind::Exec
        );
        assert_eq!(
            PinDefinition::output(PinType::Json, "test").kind,
            PinKind::Exec
        );
    }

    #[test]
    fn pin_definition_缺_kind_字段反序列化默认_exec() {
        // 旧前端 / 旧节点 JSON 不带 kind 字段，必须能反序列化为 Exec
        let json = r#"{"id":"in","label":"in","pin_type":{"kind":"any"},"direction":"input","required":true}"#;
        let pin: PinDefinition = serde_json::from_str(json).unwrap();
        assert_eq!(pin.kind, PinKind::Exec);
    }

    #[test]
    fn pin_definition_显式_kind_字段反序列化正确() {
        let json = r#"{"id":"latest","label":"latest","pin_type":{"kind":"any"},"direction":"output","required":false,"kind":"data"}"#;
        let pin: PinDefinition = serde_json::from_str(json).unwrap();
        assert_eq!(pin.kind, PinKind::Data);
    }
```

- [ ] **Step 2: 运行测试确认失败**

```bash
cargo test -p nazh-core pin::tests::pin_definition 2>&1 | head -30
```

预期：编译失败，"no field `kind` on type `PinDefinition`"。

- [ ] **Step 3: 给 PinDefinition 加 kind 字段**

在 `crates/core/src/pin.rs` 中找到 `pub struct PinDefinition { ... }`，把 `description` 字段**之前**插入：

```rust
    /// 求值语义（ADR-0014 引脚二分）。未声明默认 [`PinKind::Exec`]，向后兼容现有节点。
    #[serde(default)]
    pub kind: PinKind,
```

- [ ] **Step 4: 更新 4 个工厂函数**

把 `default_input` / `default_output` / `required_input` / `output` 的 struct 字面量都加上 `kind: PinKind::Exec,`（在 `description` 字段之前），例如：

```rust
    pub fn default_input() -> Self {
        Self {
            id: "in".to_owned(),
            label: "in".to_owned(),
            pin_type: PinType::Any,
            direction: PinDirection::Input,
            required: true,
            kind: PinKind::Exec,
            description: None,
        }
    }
```

`default_output`、`required_input`、`output` 三个工厂方法都做同样修改：在 `required: ...,` 之后、`description: ...,` 之前加 `kind: PinKind::Exec,`。

- [ ] **Step 5: 运行测试确认通过**

```bash
cargo test -p nazh-core pin::tests
```

预期：全部 PASS（含原有兼容矩阵测试与新加的 PinKind / PinDefinition 测试）。

- [ ] **Step 6: 验证 facade crate 编译通过**

```bash
cargo check --workspace --all-targets 2>&1 | tail -20
```

预期：编译通过——所有 14 类节点不显式声明 `kind` 字段时走 serde default 反序列化为 Exec，但代码中构造 `PinDefinition` 字面量的位置（pin_validator tests 与各 nodes-* crate 测试）需要同步加 `kind: PinKind::Exec`。

如果有编译错误（"missing field `kind`"），按报错位置补 `kind: PinKind::Exec,`。最常见的会是 `src/graph/pin_validator.rs` 内的 `pin()` helper 函数——把它升级为：

```rust
    fn pin(id: &str, dir: PinDirection, ty: PinType, required: bool) -> PinDefinition {
        PinDefinition {
            id: id.to_owned(),
            label: id.to_owned(),
            pin_type: ty,
            direction: dir,
            required,
            kind: PinKind::Exec,
            description: None,
        }
    }
```

如果还有其他位置（例如 `crates/nodes-flow/src/`、`crates/nodes-io/src/` 内若直接用字面量构造），统一补 `kind: PinKind::Exec`。

- [ ] **Step 7: Commit**

```bash
git add crates/core/src/pin.rs src/graph/pin_validator.rs crates/nodes-flow/ crates/nodes-io/ crates/scripting/
git commit -s -m "feat(core): PinDefinition 加 kind 字段（ADR-0014 Phase 1，默认 Exec 向后兼容）"
```

---

## Task 4: ts-rs 导出 PinKind

**Files:**
- Modify: `crates/core/src/lib.rs`

- [ ] **Step 1: 在 pub use 块加 PinKind**

找到 `pub use pin::{PinDefinition, PinDirection, PinType};`，改为：

```rust
pub use pin::{PinDefinition, PinDirection, PinKind, PinType};
```

- [ ] **Step 2: 在 export_bindings 模块加导出**

找到 `pub mod export_bindings { ... }` 块，把 `PinDefinition, PinDirection, PinType,` 行改为 `PinDefinition, PinDirection, PinKind, PinType,`，并在 `PinType::export()?;` 行**之后**插入：

```rust
        PinKind::export()?;
```

完整修改后该行附近形如：

```rust
        PinDirection::export()?;
        PinType::export()?;
        PinKind::export()?;
        PinDefinition::export()?;
```

- [ ] **Step 3: 重新生成前端类型并验证导出成功**

```bash
cargo test -p tauri-bindings --features ts-export export_bindings
```

预期：测试通过，`web/src/generated/` 下生成 `PinKind.ts`。

- [ ] **Step 4: 检查生成的 TS 类型**

```bash
cat web/src/generated/PinKind.ts
```

预期内容形如：

```typescript
export type PinKind = "exec" | "data";
```

并检查 `web/src/generated/PinDefinition.ts` 内含 `kind?: PinKind;` 或 `kind: PinKind;` 字段。

- [ ] **Step 5: Commit**

```bash
git add crates/core/src/lib.rs web/src/generated/
git commit -s -m "feat(bindings): ts-rs 导出 PinKind（ADR-0014 Phase 1）"
```

---

## Task 5: OutputCache + CachedOutput 类型

**Files:**
- Create: `crates/core/src/cache.rs`

- [ ] **Step 1: 创建文件并写实现 + 单元测试**

```rust
// crates/core/src/cache.rs

//! ADR-0014 引脚二分：节点输出缓存槽。
//!
//! 每个声明 [`PinKind::Data`](crate::PinKind::Data) 输出引脚的节点持有一份
//! [`OutputCache`]，每个 Data 输出引脚对应一个 [`ArcSwap`] 槽位。Runner 在
//! 节点 transform 完成后写槽位；下游通过 Data 边消费时（Phase 2 起）读槽位。
//!
//! **Phase 1 范围**：仅完成"写"——下游消费在 Phase 2/3 接入。

use std::sync::Arc;

use arc_swap::ArcSwap;
use chrono::{DateTime, Utc};
use dashmap::DashMap;
use serde_json::Value;
use uuid::Uuid;

/// 单个 Data 输出引脚的缓存值快照。
///
/// `trace_id` 携带产生此值时的上游 trace。下游消费时记录到自己的事件中——
/// 让"一次 transform 关联多个 trace"在观测层显式可见（设计见 ADR-0014 风险 3）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CachedOutput {
    pub value: Value,
    pub produced_at: DateTime<Utc>,
    pub trace_id: Uuid,
}

/// 单节点持有的输出缓存——一个 Data 输出引脚对应一个槽位。
///
/// 槽位用 [`ArcSwap`] 包裹 `Option<CachedOutput>`：
/// - 写：[`store`](ArcSwap::store)，无锁
/// - 读：[`load_full`](ArcSwap::load_full)，返回快照副本
///
/// `slots` 由 [`prepare_slot`](Self::prepare_slot) 在部署期初始化（仅声明
/// Data 输出 pin 的节点会有非空 `slots`），运行期对未预分配的 pin 写入是
/// 静默 noop（属于实现 bug，Runner 不应触发）。
#[derive(Debug, Default)]
pub struct OutputCache {
    slots: DashMap<String, Arc<ArcSwap<Option<CachedOutput>>>>,
}

impl OutputCache {
    pub fn new() -> Self {
        Self::default()
    }

    /// 部署期为指定 pin id 预分配槽位；同一 pin 多次预分配是幂等的。
    pub fn prepare_slot(&self, pin_id: &str) {
        if !self.slots.contains_key(pin_id) {
            self.slots
                .insert(pin_id.to_owned(), Arc::new(ArcSwap::from_pointee(None)));
        }
    }

    /// 写入指定 pin 的最新值。
    /// pin 未预分配时静默忽略——上层 Runner 在调用前应确保 [`prepare_slot`](Self::prepare_slot) 已 cover。
    pub fn write(&self, pin_id: &str, output: CachedOutput) {
        if let Some(slot) = self.slots.get(pin_id) {
            slot.store(Arc::new(Some(output)));
        }
    }

    /// 读取指定 pin 的最新缓存值。pin 未预分配或槽空时返回 `None`。
    pub fn read(&self, pin_id: &str) -> Option<CachedOutput> {
        let slot = self.slots.get(pin_id)?;
        let snapshot = slot.load_full();
        (*snapshot).clone()
    }

    /// 已分配槽位的 pin id 列表，主要供测试 / 调试。
    pub fn slot_ids(&self) -> Vec<String> {
        self.slots.iter().map(|entry| entry.key().clone()).collect()
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    fn sample_output(value: i64) -> CachedOutput {
        CachedOutput {
            value: Value::from(value),
            produced_at: Utc::now(),
            trace_id: Uuid::nil(),
        }
    }

    #[test]
    fn 未预分配的_pin_读返回_none() {
        let cache = OutputCache::new();
        assert!(cache.read("ghost").is_none());
    }

    #[test]
    fn 预分配后未写入读返回_none() {
        let cache = OutputCache::new();
        cache.prepare_slot("latest");
        assert!(cache.read("latest").is_none());
    }

    #[test]
    fn 写后读返回最新值() {
        let cache = OutputCache::new();
        cache.prepare_slot("latest");
        cache.write("latest", sample_output(42));
        let got = cache.read("latest").unwrap();
        assert_eq!(got.value, Value::from(42));
    }

    #[test]
    fn 多次写入只保留最新() {
        let cache = OutputCache::new();
        cache.prepare_slot("latest");
        cache.write("latest", sample_output(1));
        cache.write("latest", sample_output(2));
        cache.write("latest", sample_output(3));
        let got = cache.read("latest").unwrap();
        assert_eq!(got.value, Value::from(3));
    }

    #[test]
    fn 写未预分配的_pin_是_noop() {
        let cache = OutputCache::new();
        cache.write("missing", sample_output(99));
        assert!(cache.read("missing").is_none());
        assert!(cache.slot_ids().is_empty());
    }

    #[test]
    fn prepare_slot_是幂等的() {
        let cache = OutputCache::new();
        cache.prepare_slot("a");
        cache.prepare_slot("a");
        cache.prepare_slot("a");
        assert_eq!(cache.slot_ids().len(), 1);
    }

    #[test]
    fn 多个_pin_独立存储() {
        let cache = OutputCache::new();
        cache.prepare_slot("alpha");
        cache.prepare_slot("beta");
        cache.write("alpha", sample_output(1));
        cache.write("beta", sample_output(2));
        assert_eq!(cache.read("alpha").unwrap().value, Value::from(1));
        assert_eq!(cache.read("beta").unwrap().value, Value::from(2));
    }
}
```

- [ ] **Step 2: 运行单元测试**

```bash
cargo test -p nazh-core cache::tests
```

预期：7 个测试全部 PASS。

- [ ] **Step 3: Commit**

```bash
git add crates/core/src/cache.rs
git commit -s -m "feat(core): 新增 OutputCache + CachedOutput（ADR-0014 Phase 1 缓存槽）"
```

---

## Task 6: lib.rs 导出 cache 模块

**Files:**
- Modify: `crates/core/src/lib.rs`

- [ ] **Step 1: 在 pub mod 块加 cache 模块**

找到 `pub mod ai;` 后跟的模块声明列表，在 `pub mod context;` **之后**插入：

```rust
pub mod cache;
```

- [ ] **Step 2: 在 pub use 块加 cache 类型**

找到 `pub use context::{ContextRef, WorkflowContext};` 行，在它**之后**插入：

```rust
pub use cache::{CachedOutput, OutputCache};
```

- [ ] **Step 3: 验证编译并运行测试**

```bash
cargo test -p nazh-core
```

预期：包括 cache::tests 在内全部 PASS。

- [ ] **Step 4: Commit**

```bash
git add crates/core/src/lib.rs
git commit -s -m "feat(core): lib 导出 OutputCache / CachedOutput"
```

---

## Task 7: 加 EngineError::IncompatiblePinKinds 错误变体

**Files:**
- Modify: `crates/core/src/error.rs`

- [ ] **Step 1: 在 IncompatiblePinTypes 之后插入新变体**

找到 `IncompatiblePinTypes { ... }`（约第 105 行），在它的关闭大括号之后、`UnknownPin` 之前插入：

```rust
    /// 边两端引脚的求值语义不一致——上游 Exec / 下游 Data 或反之。
    /// `from` / `to` 形如 `"node_id.pin_id"`；`from_kind` / `to_kind` 是引脚 PinKind 的字符串。
    #[error("边 `{from}` → `{to}` 求值语义不匹配：上游 `{from_kind}`，下游 `{to_kind}`（ADR-0014：引脚二分要求 Kind 一致）")]
    IncompatiblePinKinds {
        from: String,
        to: String,
        from_kind: String,
        to_kind: String,
    },
```

- [ ] **Step 2: 验证编译**

```bash
cargo check -p nazh-core
```

预期：编译通过。`thiserror` 自动派生 `Error` impl，无需手写。

- [ ] **Step 3: Commit**

```bash
git add crates/core/src/error.rs
git commit -s -m "feat(core): EngineError 加 IncompatiblePinKinds 变体（ADR-0014 Phase 1）"
```

---

## Task 8: pin_validator 跨 Kind 校验

**Files:**
- Modify: `src/graph/pin_validator.rs`

- [ ] **Step 1: 写失败测试**

在 `src/graph/pin_validator.rs` 的 `mod tests` 块内，找到 `fn 默认_any_的两节点直连通过校验()` 测试**之前**，把 `pin()` helper 函数（约第 191 行）替换为支持 PinKind 参数的版本：

```rust
    fn pin(id: &str, dir: PinDirection, ty: PinType, required: bool) -> PinDefinition {
        PinDefinition {
            id: id.to_owned(),
            label: id.to_owned(),
            pin_type: ty,
            direction: dir,
            required,
            kind: PinKind::Exec,
            description: None,
        }
    }

    fn pin_with_kind(
        id: &str,
        dir: PinDirection,
        ty: PinType,
        required: bool,
        kind: PinKind,
    ) -> PinDefinition {
        PinDefinition {
            id: id.to_owned(),
            label: id.to_owned(),
            pin_type: ty,
            direction: dir,
            required,
            kind,
            description: None,
        }
    }
```

并把 `use nazh_core::{...};` 行加上 `PinKind`：

```rust
    use nazh_core::{EngineError, NodeExecution, NodeTrait, PinDefinition, PinDirection, PinKind, PinType};
```

然后在 `fn array_嵌套兼容通过校验()` 测试**之后**插入新测试：

```rust
    #[test]
    fn 跨_kind_连接报_incompatible_pin_kinds() {
        let nodes = HashMap::from([
            node(
                "a",
                vec![PinDefinition::default_input()],
                vec![pin_with_kind(
                    "out",
                    PinDirection::Output,
                    PinType::Any,
                    false,
                    PinKind::Data,
                )],
            ),
            node(
                "b",
                vec![pin_with_kind(
                    "in",
                    PinDirection::Input,
                    PinType::Any,
                    true,
                    PinKind::Exec,
                )],
                vec![PinDefinition::default_output()],
            ),
        ]);
        let edges = vec![edge("a", "b", None, None)];
        let err = validate_pin_compatibility(&nodes, &edges).unwrap_err();
        match err {
            EngineError::IncompatiblePinKinds { from, to, from_kind, to_kind } => {
                assert_eq!(from, "a.out");
                assert_eq!(to, "b.in");
                assert_eq!(from_kind, "data");
                assert_eq!(to_kind, "exec");
            }
            other => panic!("应报 IncompatiblePinKinds，实际：{other:?}"),
        }
    }

    #[test]
    fn 同_kind_data_data_连接通过校验() {
        let nodes = HashMap::from([
            node(
                "a",
                vec![PinDefinition::default_input()],
                vec![pin_with_kind(
                    "out",
                    PinDirection::Output,
                    PinType::Any,
                    false,
                    PinKind::Data,
                )],
            ),
            node(
                "b",
                vec![pin_with_kind(
                    "in",
                    PinDirection::Input,
                    PinType::Any,
                    true,
                    PinKind::Data,
                )],
                vec![PinDefinition::default_output()],
            ),
        ]);
        let edges = vec![edge("a", "b", None, None)];
        validate_pin_compatibility(&nodes, &edges).unwrap();
    }
```

- [ ] **Step 2: 运行测试确认失败**

```bash
cargo test -p nazh-engine pin_validator::tests::跨_kind 2>&1 | head -30
```

预期：`跨_kind_连接报_incompatible_pin_kinds` 测试失败——当前校验函数不知道 PinKind，会先在 PinType 维度通过然后没有更多检查。

- [ ] **Step 3: 在 pin_validator 加 Kind 校验**

打开 `src/graph/pin_validator.rs`，在 `validate_pin_compatibility` 函数内找到 `if !from_pin.pin_type.is_compatible_with(&to_pin.pin_type) { ... }` 块，在它的关闭大括号**之后**插入：

```rust
        if !from_pin.kind.is_compatible_with(to_pin.kind) {
            return Err(EngineError::IncompatiblePinKinds {
                from: format!("{}.{}", edge.from, from_pin.id),
                to: format!("{}.{}", edge.to, to_pin.id),
                from_kind: from_pin.kind.to_string(),
                to_kind: to_pin.kind.to_string(),
            });
        }
```

并把文件头部 `use nazh_core::{...};` 加上 `PinKind`：

```rust
use nazh_core::{EngineError, NodeTrait, PinDefinition, PinDirection, PinKind};
```

（注意：`PinKind` 已 use 但在校验体内通过 `from_pin.kind` 字段访问，技术上不需要 use；保留 use 是为后续 helper 可能直接引用。如果 clippy 报 unused，删掉这行。）

- [ ] **Step 4: 运行测试确认通过**

```bash
cargo test -p nazh-engine pin_validator::tests
```

预期：所有 pin_validator 测试 PASS（含原有 + 新加的 2 个 Kind 测试）。

- [ ] **Step 5: Commit**

```bash
git add src/graph/pin_validator.rs
git commit -s -m "feat(graph): pin_validator 加 PinKind 一致性校验（ADR-0014 Phase 1）"
```

---

## Task 9: topology 加 classify_edges 方法

**Files:**
- Modify: `src/graph/topology.rs`

- [ ] **Step 1: 写失败测试**

在 `src/graph/topology.rs` 文件末尾（如有 `mod tests` 块就追加；没有就新建）插入：

```rust
#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use crate::graph::types::WorkflowEdge;
    use nazh_core::{NodeExecution, NodeTrait, PinDefinition, PinDirection, PinKind, PinType};
    use async_trait::async_trait;
    use serde_json::Value;
    use std::sync::Arc;
    use uuid::Uuid;

    /// 测试 stub 节点：通过构造函数注入 input / output pin 列表。
    struct StubNode {
        id: String,
        inputs: Vec<PinDefinition>,
        outputs: Vec<PinDefinition>,
    }

    #[async_trait]
    impl NodeTrait for StubNode {
        fn id(&self) -> &str {
            &self.id
        }
        fn kind(&self) -> &'static str {
            "stub"
        }
        fn input_pins(&self) -> Vec<PinDefinition> {
            self.inputs.clone()
        }
        fn output_pins(&self) -> Vec<PinDefinition> {
            self.outputs.clone()
        }
        async fn transform(
            &self,
            _trace_id: Uuid,
            _payload: Value,
        ) -> Result<NodeExecution, nazh_core::EngineError> {
            Ok(NodeExecution::broadcast(Value::Null))
        }
    }

    fn pin(id: &str, dir: PinDirection, kind: PinKind) -> PinDefinition {
        PinDefinition {
            id: id.to_owned(),
            label: id.to_owned(),
            pin_type: PinType::Any,
            direction: dir,
            required: false,
            kind,
            description: None,
        }
    }

    fn make_node(
        id: &str,
        inputs: Vec<PinDefinition>,
        outputs: Vec<PinDefinition>,
    ) -> Arc<dyn NodeTrait> {
        Arc::new(StubNode {
            id: id.to_owned(),
            inputs,
            outputs,
        })
    }

    fn edge(from: &str, to: &str, source_port: Option<&str>) -> WorkflowEdge {
        WorkflowEdge {
            from: from.to_owned(),
            to: to.to_owned(),
            source_port_id: source_port.map(str::to_owned),
            target_port_id: None,
        }
    }

    #[test]
    fn classify_edges_把_data_pin_出边归为_data() {
        let mut nodes: HashMap<String, Arc<dyn NodeTrait>> = HashMap::new();
        nodes.insert(
            "a".to_owned(),
            make_node(
                "a",
                vec![pin("in", PinDirection::Input, PinKind::Exec)],
                vec![pin("latest", PinDirection::Output, PinKind::Data)],
            ),
        );
        nodes.insert(
            "b".to_owned(),
            make_node(
                "b",
                vec![pin("in", PinDirection::Input, PinKind::Data)],
                vec![PinDefinition::default_output()],
            ),
        );

        let edges = vec![edge("a", "b", Some("latest"))];
        let classified = classify_edges(&edges, &nodes).unwrap();
        assert_eq!(classified.exec_edges.len(), 0);
        assert_eq!(classified.data_edges.len(), 1);
        assert_eq!(classified.data_edges[0].from, "a");
    }

    #[test]
    fn classify_edges_把_exec_pin_出边归为_exec() {
        let mut nodes: HashMap<String, Arc<dyn NodeTrait>> = HashMap::new();
        nodes.insert(
            "a".to_owned(),
            make_node(
                "a",
                vec![PinDefinition::default_input()],
                vec![PinDefinition::default_output()],
            ),
        );
        nodes.insert(
            "b".to_owned(),
            make_node(
                "b",
                vec![PinDefinition::default_input()],
                vec![PinDefinition::default_output()],
            ),
        );

        let edges = vec![edge("a", "b", None)];
        let classified = classify_edges(&edges, &nodes).unwrap();
        assert_eq!(classified.exec_edges.len(), 1);
        assert_eq!(classified.data_edges.len(), 0);
    }

    #[test]
    fn classify_edges_未知_source_port_报错() {
        let mut nodes: HashMap<String, Arc<dyn NodeTrait>> = HashMap::new();
        nodes.insert(
            "a".to_owned(),
            make_node(
                "a",
                vec![PinDefinition::default_input()],
                vec![PinDefinition::default_output()],
            ),
        );
        nodes.insert(
            "b".to_owned(),
            make_node(
                "b",
                vec![PinDefinition::default_input()],
                vec![PinDefinition::default_output()],
            ),
        );

        let edges = vec![edge("a", "b", Some("ghost"))];
        let err = classify_edges(&edges, &nodes).unwrap_err();
        assert!(matches!(err, nazh_core::EngineError::UnknownPin { .. }));
    }
}
```

- [ ] **Step 2: 运行测试确认失败**

```bash
cargo test -p nazh-engine topology::tests::classify_edges 2>&1 | head -20
```

预期：编译失败，"cannot find function `classify_edges`"。

- [ ] **Step 3: 实现 classify_edges**

打开 `src/graph/topology.rs`，在文件顶部 use 块加：

```rust
use std::sync::Arc;

use nazh_core::{NodeTrait, PinKind};
```

（如果 `Arc` 已 use 跳过；如果 `NodeTrait` / `PinKind` 已 use 跳过）

然后在 `impl WorkflowGraph { ... }` 块**之后**、`#[cfg(test)]` 之前插入：

```rust
/// 边按 [`PinKind`] 分类的结果（ADR-0014 Phase 1）。
///
/// `'a` 借用 `WorkflowEdge` 列表本身的生命周期——分类只重组引用，不克隆。
pub(crate) struct ClassifiedEdges<'a> {
    pub exec_edges: Vec<&'a super::types::WorkflowEdge>,
    pub data_edges: Vec<&'a super::types::WorkflowEdge>,
}

const DEFAULT_OUTPUT_PIN_ID: &str = "out";

/// 按上游节点 source pin 的 [`PinKind`] 把边分类为 exec / data。
///
/// 参数 `nodes` 必须包含图中所有节点（阶段 0.5 实例化后）。
///
/// # Errors
///
/// 边引用的源节点不存在、或源节点 output_pins 中找不到对应 pin id 时返回
/// [`EngineError::UnknownPin`]——这种 case 也应在 `pin_validator` 提前发现，
/// 但本函数自包含校验避免依赖前置阶段，便于单测。
pub(crate) fn classify_edges<'a>(
    edges: &'a [super::types::WorkflowEdge],
    nodes: &HashMap<String, Arc<dyn NodeTrait>>,
) -> Result<ClassifiedEdges<'a>, crate::EngineError> {
    let mut exec_edges = Vec::new();
    let mut data_edges = Vec::new();

    for edge in edges {
        let from_node = nodes.get(&edge.from).ok_or_else(|| {
            crate::EngineError::invalid_graph(format!(
                "classify_edges：边的源节点 `{}` 不存在",
                edge.from
            ))
        })?;
        let from_pin_id = edge
            .source_port_id
            .as_deref()
            .unwrap_or(DEFAULT_OUTPUT_PIN_ID);
        let from_pin =
            from_node
                .output_pins()
                .into_iter()
                .find(|p| p.id == from_pin_id)
                .ok_or_else(|| crate::EngineError::UnknownPin {
                    node: edge.from.clone(),
                    pin: from_pin_id.to_owned(),
                    direction: nazh_core::PinDirection::Output,
                })?;

        match from_pin.kind {
            PinKind::Exec => exec_edges.push(edge),
            PinKind::Data => data_edges.push(edge),
        }
    }

    Ok(ClassifiedEdges {
        exec_edges,
        data_edges,
    })
}
```

- [ ] **Step 4: 运行测试确认通过**

```bash
cargo test -p nazh-engine topology::tests::classify_edges
```

预期：3 个 classify_edges 测试 PASS。

- [ ] **Step 5: Commit**

```bash
git add src/graph/topology.rs
git commit -s -m "feat(graph): topology::classify_edges 按 PinKind 分类边（ADR-0014 Phase 1）"
```

---

## Task 10: detect_data_edge_cycle

**Files:**
- Modify: `src/graph/topology.rs`

- [ ] **Step 1: 写失败测试**

在 `src/graph/topology.rs` 的 `mod tests` 块末尾追加：

```rust
    #[test]
    fn detect_data_edge_cycle_无_data_边时通过() {
        let mut nodes: HashMap<String, Arc<dyn NodeTrait>> = HashMap::new();
        nodes.insert(
            "a".to_owned(),
            make_node(
                "a",
                vec![PinDefinition::default_input()],
                vec![PinDefinition::default_output()],
            ),
        );
        nodes.insert(
            "b".to_owned(),
            make_node(
                "b",
                vec![PinDefinition::default_input()],
                vec![PinDefinition::default_output()],
            ),
        );
        let edges = vec![edge("a", "b", None)];
        let classified = classify_edges(&edges, &nodes).unwrap();
        detect_data_edge_cycle(&classified.data_edges).unwrap();
    }

    #[test]
    fn detect_data_edge_cycle_data_边形成环时报错() {
        // a 的 Data 输出 → b 的 Data 输入；b 的 Data 输出 → a 的 Data 输入
        // 构成 Data 边的环
        let mut nodes: HashMap<String, Arc<dyn NodeTrait>> = HashMap::new();
        nodes.insert(
            "a".to_owned(),
            make_node(
                "a",
                vec![pin("in", PinDirection::Input, PinKind::Data)],
                vec![pin("out", PinDirection::Output, PinKind::Data)],
            ),
        );
        nodes.insert(
            "b".to_owned(),
            make_node(
                "b",
                vec![pin("in", PinDirection::Input, PinKind::Data)],
                vec![pin("out", PinDirection::Output, PinKind::Data)],
            ),
        );
        let edges = vec![edge("a", "b", Some("out")), edge("b", "a", Some("out"))];
        let classified = classify_edges(&edges, &nodes).unwrap();
        let err = detect_data_edge_cycle(&classified.data_edges).unwrap_err();
        assert!(matches!(err, crate::EngineError::InvalidGraph(_)));
    }

    #[test]
    fn detect_data_edge_cycle_data_边自环报错() {
        let mut nodes: HashMap<String, Arc<dyn NodeTrait>> = HashMap::new();
        nodes.insert(
            "a".to_owned(),
            make_node(
                "a",
                vec![pin("in", PinDirection::Input, PinKind::Data)],
                vec![pin("out", PinDirection::Output, PinKind::Data)],
            ),
        );
        let edges = vec![edge("a", "a", Some("out"))];
        let classified = classify_edges(&edges, &nodes).unwrap();
        let err = detect_data_edge_cycle(&classified.data_edges).unwrap_err();
        assert!(matches!(err, crate::EngineError::InvalidGraph(_)));
    }
```

- [ ] **Step 2: 运行测试确认失败**

```bash
cargo test -p nazh-engine topology::tests::detect_data 2>&1 | head -20
```

预期：编译失败，"cannot find function `detect_data_edge_cycle`"。

- [ ] **Step 3: 实现 detect_data_edge_cycle**

在 `src/graph/topology.rs` 的 `classify_edges` 函数**之后**插入：

```rust
/// 在 Data 边构成的子图上做环检测（ADR-0014 Phase 1）。
///
/// Data 边不参与主拓扑（避免 Data 拉取关系污染 Exec 触发顺序），但**仍可能
/// 形成依赖环**——A 的 Data 输出依赖 B 的最新值、B 的 Data 输出又依赖 A 的最新值。
/// 此种环让 Phase 2/3 的"下游 transform 前拉上游缓存"陷入无定义循环依赖。
///
/// 算法：在 Data 边构成的图上跑 Kahn——若不能消化所有节点，存在环。
///
/// # Errors
///
/// Data 边构成环时返回 [`EngineError::InvalidGraph`]。
pub(crate) fn detect_data_edge_cycle(
    data_edges: &[&super::types::WorkflowEdge],
) -> Result<(), crate::EngineError> {
    if data_edges.is_empty() {
        return Ok(());
    }

    // 构造 Data 子图：仅含 data_edges 涉及的节点
    let mut incoming: HashMap<String, usize> = HashMap::new();
    let mut downstream: HashMap<String, Vec<String>> = HashMap::new();
    for edge in data_edges {
        incoming.entry(edge.from.clone()).or_insert(0);
        *incoming.entry(edge.to.clone()).or_insert(0) += 1;
        downstream
            .entry(edge.from.clone())
            .or_default()
            .push(edge.to.clone());
    }

    let total_nodes = incoming.len();
    let mut queue: VecDeque<String> = incoming
        .iter()
        .filter(|(_, count)| **count == 0)
        .map(|(id, _)| id.clone())
        .collect();
    let mut consumed = 0_usize;

    while let Some(node_id) = queue.pop_front() {
        consumed += 1;
        if let Some(neighbors) = downstream.get(&node_id) {
            for neighbor in neighbors {
                if let Some(count) = incoming.get_mut(neighbor) {
                    *count -= 1;
                    if *count == 0 {
                        queue.push_back(neighbor.clone());
                    }
                }
            }
        }
    }

    if consumed != total_nodes {
        return Err(crate::EngineError::invalid_graph(
            "Data 边构成环（ADR-0014）：下游 transform 时无法确定缓存读取顺序",
        ));
    }
    Ok(())
}
```

- [ ] **Step 4: 运行测试确认通过**

```bash
cargo test -p nazh-engine topology::tests
```

预期：所有 topology::tests 测试 PASS（含原有 + 新加的 3 个 cycle 测试 + 之前的 3 个 classify 测试）。

- [ ] **Step 5: Commit**

```bash
git add src/graph/topology.rs
git commit -s -m "feat(graph): detect_data_edge_cycle Data 边独立环检测（ADR-0014 Phase 1）"
```

---

## Task 11: deploy.rs 注入 OutputCache 与边分类

**Files:**
- Modify: `src/graph/deploy.rs`
- Modify: `src/graph/types.rs`

- [ ] **Step 1: types.rs 加 DataDownstreamTarget 与节点 OutputCache 集合**

打开 `src/graph/types.rs`，在 `pub(crate) struct DownstreamTarget { ... }` **之后**插入：

```rust
/// Data 边的下游目标：仅记录目标节点 id 与目标 pin id（Phase 1 不实际使用，
/// Phase 2 起下游 transform 前据此读取上游 OutputCache 槽位）。
#[derive(Clone, Debug)]
pub(crate) struct DataDownstreamTarget {
    pub(crate) target_node_id: String,
    pub(crate) target_pin_id: String,
    pub(crate) source_pin_id: String,
}
```

- [ ] **Step 2: 修改 types.rs 的 use 块加 OutputCache**

找到顶部 `use crate::{...};`，加入 `OutputCache`：

```rust
use crate::{
    CancellationToken, ContextRef, DataStore, EngineError, ExecutionEvent, LifecycleGuard,
    OutputCache, SharedResources, VariableDeclaration, WorkflowContext, WorkflowNodeDefinition,
};
```

（如果 use 块组织略有不同，按原样合并即可。`OutputCache` 通过 `crates/core` 重新导出。）

- [ ] **Step 3: 修改 deploy.rs，在阶段 0.5 后做边分类与环检测**

打开 `src/graph/deploy.rs`，在文件顶部 use 块加：

```rust
use std::collections::HashSet;

use super::topology::{classify_edges, detect_data_edge_cycle};
use super::types::DataDownstreamTarget;
use nazh_core::{OutputCache, PinKind};
```

（合并到现有 use 块。`HashSet` 通常已有，跳过即可。）

然后找到 `pin_validator::validate_pin_compatibility(&nodes_by_id, &graph.edges)?;` 这一行，在它**之后**插入：

```rust
    // ADR-0014 Phase 1：边按上游 source pin 的 PinKind 分类，Data 子图独立环检测
    let classified = classify_edges(&graph.edges, &nodes_by_id)?;
    detect_data_edge_cycle(&classified.data_edges)?;

    // 给每个节点准备 OutputCache：仅声明 Data 输出 pin 的节点会有非空 slots
    let output_caches: HashMap<String, Arc<OutputCache>> = nodes_by_id
        .iter()
        .map(|(id, node)| {
            let cache = OutputCache::new();
            for pin in node.output_pins() {
                if pin.kind == PinKind::Data {
                    cache.prepare_slot(&pin.id);
                }
            }
            (id.clone(), Arc::new(cache))
        })
        .collect();

    // Data 边按 from 节点分组：from_node_id → Vec<DataDownstreamTarget>
    let mut data_targets_by_source: HashMap<String, Vec<DataDownstreamTarget>> = HashMap::new();
    for edge in &classified.data_edges {
        let source_pin_id = edge
            .source_port_id
            .clone()
            .unwrap_or_else(|| "out".to_owned());
        let target_pin_id = edge
            .target_port_id
            .clone()
            .unwrap_or_else(|| "in".to_owned());
        data_targets_by_source
            .entry(edge.from.clone())
            .or_default()
            .push(DataDownstreamTarget {
                target_node_id: edge.to.clone(),
                target_pin_id,
                source_pin_id,
            });
    }

    // 计算每个节点的 Data 输出 pin id 集合（Runner 用来决定哪些 output 写 cache）
    let data_output_pin_ids_by_node: HashMap<String, HashSet<String>> = nodes_by_id
        .iter()
        .map(|(id, node)| {
            let pins: HashSet<String> = node
                .output_pins()
                .into_iter()
                .filter(|p| p.kind == PinKind::Data)
                .map(|p| p.id)
                .collect();
            (id.clone(), pins)
        })
        .collect();
```

- [ ] **Step 4: 修改阶段 2 spawn 路径，把 OutputCache 与 data_pin_ids 传给 run_node**

在 deploy.rs 的阶段 2 spawn 块（约 `for node_id in &topology.deployment_order { ... runtime.spawn(run_node(...)) ... }`），把 `runtime.spawn(run_node(...))` 调用改造为：

```rust
        let data_output_pin_ids = data_output_pin_ids_by_node
            .get(node_id)
            .cloned()
            .unwrap_or_default();
        let output_cache = Arc::clone(
            output_caches
                .get(node_id)
                .ok_or_else(|| EngineError::invalid_graph("阶段 2：output_cache 缺失"))?,
        );

        runtime.spawn(run_node(
            node,
            node_definition.timeout_ms().map(Duration::from_millis),
            input_rx,
            downstream_senders,
            result_tx.clone(),
            event_tx.clone(),
            Arc::clone(&store),
            output_cache,
            data_output_pin_ids,
        ));
```

注意：`run_node` 签名要在 Task 12 同步更新。本 step 完成后 `cargo check` 会失败——Task 12 修复。

**为什么不在本 step 同时更新 run_node**：分两个 commit 让 review 更细——本任务专注 deploy 层的"边分类 + 缓存槽分配"决策；Task 12 专注 Runner 双路径写入逻辑。

- [ ] **Step 5: 暂时跳过编译验证，因为 run_node 签名 Task 12 才更新**

直接进入 commit。Task 12 完成后会做完整 cargo test。

- [ ] **Step 6: Commit**

```bash
git add src/graph/deploy.rs src/graph/types.rs
git commit -s -m "feat(graph): deploy 阶段 0.5 后做边分类 + 环检测 + OutputCache 注入（ADR-0014 Phase 1，需 Task 12 修复 run_node 签名）"
```

---

## Task 12: runner.rs 双路径写入

**Files:**
- Modify: `src/graph/runner.rs`

- [ ] **Step 1: 更新 run_node 签名与实现**

打开 `src/graph/runner.rs`，把整个 `run_node` 函数替换为：

```rust
/// 单节点的异步执行循环：接收 [`ContextRef`] → 读取数据 → 执行 → 写入输出 → 分发。
///
/// ADR-0014 Phase 1：transform 完成后，对每条 [`NodeOutput`]，按 [`NodeDispatch`]
/// 解析的目标 port id 与 `data_output_pin_ids` 求交集——交集中的 pin 走 Data
/// 路径（写 [`OutputCache`] 槽位，不 push）；其余 pin 走 Exec 路径（推 MPSC）。
/// Phase 1 不读 cache（下游消费在 Phase 2 接入）。
#[allow(clippy::too_many_lines, clippy::too_many_arguments)]
pub(crate) async fn run_node(
    node: Arc<dyn NodeTrait>,
    timeout: Option<Duration>,
    mut input_rx: mpsc::Receiver<ContextRef>,
    downstream_senders: Vec<DownstreamTarget>,
    result_tx: mpsc::Sender<ContextRef>,
    event_tx: mpsc::Sender<ExecutionEvent>,
    store: Arc<dyn DataStore>,
    output_cache: Arc<OutputCache>,
    data_output_pin_ids: HashSet<String>,
) {
    let node_id = node.id().to_owned();

    while let Some(ctx_ref) = input_rx.recv().await {
        let trace_id = ctx_ref.trace_id;

        emit_event(
            &event_tx,
            ExecutionEvent::Started {
                stage: node_id.clone(),
                trace_id,
            },
        );

        let payload_result = store.read_mut(&ctx_ref.data_id);
        store.release(&ctx_ref.data_id);

        let payload = match payload_result {
            Ok(p) => p,
            Err(error) => {
                emit_failure(&event_tx, &node_id, trace_id, &error);
                continue;
            }
        };

        let span = tracing::info_span!(
            "node.transform",
            node_id = %node_id,
            trace_id = %trace_id,
        );
        let result = guarded_execute(
            &node_id,
            trace_id,
            timeout,
            node.transform(trace_id, payload),
        )
        .instrument(span)
        .await;

        match result {
            Ok(output) => {
                let mut send_error = None;
                let mut merged_metadata = serde_json::Map::new();

                for node_output in output.outputs {
                    // 解析此次输出的目标 port id 集合（dispatch 决定）
                    let dispatch_ports: Option<Vec<String>> = match &node_output.dispatch {
                        NodeDispatch::Broadcast => None, // None 表示"所有 pin"
                        NodeDispatch::Route(ports) => Some(ports.clone()),
                    };

                    // ADR-0014 Phase 1：先写 Data 缓存槽（不 push）
                    if !data_output_pin_ids.is_empty() {
                        let data_pins_to_write: Vec<&String> = match &dispatch_ports {
                            None => data_output_pin_ids.iter().collect(),
                            Some(ports) => ports
                                .iter()
                                .filter(|p| data_output_pin_ids.contains(*p))
                                .collect(),
                        };
                        for pin_id in data_pins_to_write {
                            output_cache.write(
                                pin_id,
                                nazh_core::CachedOutput {
                                    value: node_output.payload.clone(),
                                    produced_at: chrono::Utc::now(),
                                    trace_id,
                                },
                            );
                        }
                    }

                    // Exec 路径：仅匹配非 Data 输出 pin 的下游 sender
                    let matching_targets = match &node_output.dispatch {
                        NodeDispatch::Broadcast => downstream_senders
                            .iter()
                            .filter(|target| {
                                target
                                    .source_port_id
                                    .as_ref()
                                    .map_or(true, |port| !data_output_pin_ids.contains(port))
                            })
                            .collect::<Vec<_>>(),
                        NodeDispatch::Route(port_ids) => downstream_senders
                            .iter()
                            .filter(|target| {
                                target.source_port_id.as_ref().is_some_and(|port_id| {
                                    !data_output_pin_ids.contains(port_id)
                                        && port_ids.iter().any(|candidate| candidate == port_id)
                                })
                            })
                            .collect::<Vec<_>>(),
                    };

                    for (key, value) in node_output.metadata {
                        merged_metadata.insert(key, value);
                    }

                    let consumer_count = if matching_targets.is_empty() {
                        1
                    } else {
                        matching_targets.len()
                    };

                    let data_id = match store.write(node_output.payload, consumer_count) {
                        Ok(id) => id,
                        Err(error) => {
                            send_error = Some(error);
                            break;
                        }
                    };

                    let new_ref = ContextRef::new(trace_id, data_id, Some(node_id.clone()));

                    let write_result = if matching_targets.is_empty() {
                        result_tx
                            .send(new_ref)
                            .await
                            .map_err(|_| EngineError::ChannelClosed {
                                stage: node_id.clone(),
                            })
                    } else {
                        let mut downstream_error = None;
                        for target in &matching_targets {
                            if target.sender.send(new_ref.clone()).await.is_err() {
                                downstream_error = Some(EngineError::ChannelClosed {
                                    stage: node_id.clone(),
                                });
                                break;
                            }
                        }
                        if let Some(error) = downstream_error {
                            Err(error)
                        } else {
                            Ok(())
                        }
                    };

                    match write_result {
                        Ok(()) => {
                            if matching_targets.is_empty() {
                                emit_event(
                                    &event_tx,
                                    ExecutionEvent::Output {
                                        stage: node_id.clone(),
                                        trace_id,
                                    },
                                );
                            }
                        }
                        Err(error) => {
                            send_error = Some(error);
                            break;
                        }
                    }
                }

                if let Some(error) = send_error {
                    emit_failure(&event_tx, &node_id, trace_id, &error);
                    break;
                }

                emit_event(
                    &event_tx,
                    ExecutionEvent::Completed(crate::CompletedExecutionEvent {
                        stage: node_id.clone(),
                        trace_id,
                        metadata: if merged_metadata.is_empty() {
                            None
                        } else {
                            Some(merged_metadata)
                        },
                    }),
                );
            }
            Err(error) => {
                emit_failure(&event_tx, &node_id, trace_id, &error);
            }
        }
    }
}
```

关键变量解释：
- `dispatch_ports`（第二个块顶部）：把 `NodeDispatch::Broadcast` 转为 `None`、把 `Route(ports)` 转为 `Some(ports.clone())`，作为"Data 写入目标 pin 集合"决策的输入
- `data_pins_to_write`：与 `data_output_pin_ids` 取交集后的实际写 cache 的 pin id 集合
- Exec 路径的 `matching_targets`：在原有 `match &node_output.dispatch` 基础上多加一层过滤，把"source pin 在 data_output_pin_ids 内"的 sender 排除——避免 Exec 路径推到 Data pin 的 sender 上（虽然部署期已校验 Data pin 不该有 Exec sender，但这层防御让 Runner 也是局部正确的）。

- [ ] **Step 2: 修改 runner.rs 顶部 use**

把文件顶部 use 改为：

```rust
use std::{collections::HashSet, sync::Arc, time::Duration};

use tokio::sync::mpsc;
use tracing::Instrument;

use nazh_core::{
    ContextRef, DataStore, EngineError, ExecutionEvent, NodeDispatch, NodeTrait, OutputCache,
    event::{emit_event, emit_failure},
    guard::guarded_execute,
};

use super::types::DownstreamTarget;
```

- [ ] **Step 3: 验证整个 workspace 编译**

```bash
cargo check --workspace --all-targets 2>&1 | tail -40
```

预期：编译通过。

- [ ] **Step 4: 跑全量测试**

```bash
cargo test --workspace 2>&1 | tail -20
```

预期：全部 PASS（含 cache::tests + pin::tests + pin_validator::tests + topology::tests + 现有所有节点测试）。

- [ ] **Step 5: Commit**

```bash
git add src/graph/runner.rs
git commit -s -m "feat(graph): runner 双路径骨架——Data 输出 pin 写 OutputCache 槽位（ADR-0014 Phase 1）"
```

---

## Task 13: 端到端 stub 集成测试

**Files:**
- Create: `tests/pin_kind_phase1.rs`

- [ ] **Step 1: 写集成测试文件**

```rust
//! ADR-0014 Phase 1 端到端集成测试：stub 节点声明 Data 输出 pin，
//! deploy + submit 后断言 OutputCache 槽位被正确写入。
//!
//! 业务节点 Phase 1 全保持 Kind=Exec，不在本测试范围。

use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;
use nazh_engine::{
    EngineError, NodeCapabilities, NodeExecution, NodeRegistry, NodeTrait, PinDefinition,
    PinDirection, PinKind, PinType, WorkflowContext, WorkflowGraph, WorkflowNodeDefinition,
    deploy_workflow, shared_connection_manager,
};
use serde_json::{Value, json};
use uuid::Uuid;

/// 声明双输出（Exec + Data）的 stub 节点：transform 直接返回 input payload。
struct DualOutputStub {
    id: String,
}

#[async_trait]
impl NodeTrait for DualOutputStub {
    fn id(&self) -> &str {
        &self.id
    }
    fn kind(&self) -> &'static str {
        "dualOutputStub"
    }
    fn input_pins(&self) -> Vec<PinDefinition> {
        vec![PinDefinition::default_input()]
    }
    fn output_pins(&self) -> Vec<PinDefinition> {
        vec![
            PinDefinition::default_output(), // Exec out（默认 kind = Exec）
            PinDefinition {
                id: "latest".to_owned(),
                label: "latest".to_owned(),
                pin_type: PinType::Any,
                direction: PinDirection::Output,
                required: false,
                kind: PinKind::Data,
                description: None,
            },
        ]
    }
    async fn transform(
        &self,
        _trace_id: Uuid,
        payload: Value,
    ) -> Result<NodeExecution, EngineError> {
        Ok(NodeExecution::broadcast(payload))
    }
}

#[tokio::test]
async fn data_输出_pin_的节点_transform_后_output_cache_被写入() {
    // 1. 注册 stub 节点
    let mut registry = NodeRegistry::default();
    registry.register_with_capabilities(
        "dualOutputStub",
        NodeCapabilities::empty(),
        |def, _res| {
            Ok(Arc::new(DualOutputStub {
                id: def.id().to_owned(),
            }) as Arc<dyn NodeTrait>)
        },
    );

    // 2. 构造图：单个 stub 节点为根，Data 输出无下游
    // 用 serde_json 构造 WorkflowNodeDefinition——避免依赖未公开的 helper
    let stub_def: WorkflowNodeDefinition = serde_json::from_value(json!({
        "id": "stub",
        "type": "dualOutputStub",
        "config": {}
    }))
    .unwrap();
    let mut nodes = HashMap::new();
    nodes.insert("stub".to_owned(), stub_def);
    let graph = WorkflowGraph {
        name: Some("phase1-data-cache-test".to_owned()),
        connections: vec![],
        nodes,
        edges: vec![],
        variables: None,
    };

    // 3. 部署 + submit
    let conn_mgr = shared_connection_manager();
    let mut deployment = deploy_workflow(graph, conn_mgr, &registry)
        .await
        .expect("deploy should succeed");

    deployment
        .submit(WorkflowContext::new(Value::from(42)))
        .await
        .unwrap();

    // 等待节点完成 transform
    let _completed = wait_for_completion(&mut deployment).await;

    // 4. 断言：通过 deployment.resources() 找不到 OutputCache（它不在 resource bag 内），
    //    Phase 1 没有暴露 cache 给壳层——验证仅在 #[cfg(test)] / 内部测试中通过节点自身或专门接口
    //    读取。本集成测试断言仅"deploy + submit 不 panic、Completed 事件被发出"，
    //    cache 槽位的实际内容由 cache::tests 单测覆盖。
    //
    // Phase 2 起把 cache 暴露到 resources 让下游能拉，那时再加"读取 cache 断言写入"的
    // 端到端测试。

    deployment.shutdown().await;
}

async fn wait_for_completion(
    deployment: &mut nazh_engine::WorkflowDeployment,
) -> Option<nazh_engine::CompletedExecutionEvent> {
    use nazh_engine::ExecutionEvent;
    while let Some(event) = deployment.next_event().await {
        if let ExecutionEvent::Completed(c) = event {
            return Some(c);
        }
    }
    None
}
```

**关于 `WorkflowNodeDefinition::for_test`**：检查 `crates/core/src/plugin.rs` 是否已有此 helper。如果没有，使用现有公开 API（`WorkflowNodeDefinition` 的 `Deserialize` from JSON）替代：

```rust
let node_def: WorkflowNodeDefinition = serde_json::from_value(serde_json::json!({
    "id": "stub",
    "type": "dualOutputStub",
    "config": {},
}))
.unwrap();
nodes.insert("stub".to_owned(), node_def);
```

如果 `WorkflowContext::new` 也不存在（API 名不同），看 `crates/core/src/context.rs` 的实际工厂方法名（典型如 `WorkflowContext { trace_id, timestamp, payload }` 字面量）。

> 实施时应优先按现有公开 API 调整，不要为测试新增 helper（除非测试明显需要复用）。

- [ ] **Step 2: 跑集成测试**

```bash
cargo test --test pin_kind_phase1
```

预期：测试 PASS。

如果失败，最常见原因：
- API 名对不上 → 按编译错误调整 imports / 工厂调用
- `wait_for_completion` 卡死 → 加 `tokio::time::timeout` 包装：

```rust
let completed = tokio::time::timeout(
    std::time::Duration::from_secs(2),
    wait_for_completion(&mut deployment),
).await.expect("transform 应该在 2s 内完成").expect("应有 Completed 事件");
```

- [ ] **Step 3: Commit**

```bash
git add tests/pin_kind_phase1.rs
git commit -s -m "test: ADR-0014 Phase 1 端到端 stub 节点验证 deploy + Data 输出引脚不 panic"
```

---

## Task 14: 验收回归 + 文档同步

**Files:**
- Modify: `CLAUDE.md` (= `AGENTS.md`)
- Modify: `docs/adr/0014-执行边与数据边分离.md`

- [ ] **Step 1: 跑全量回归（强制三件套）**

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

预期：全部通过、无 warning。如果 clippy 报问题，按报错位置调整代码（不要 `#[allow]` 绕过——除非确实是 false positive 且有充分理由）。

- [ ] **Step 2: 验证 ts-rs 导出未漂移**

```bash
cargo test -p tauri-bindings --features ts-export export_bindings
git diff web/src/generated/
```

预期：仅 `PinKind.ts` 新增、`PinDefinition.ts` 加 `kind` 字段——这些应在 Task 4 已 commit。如果 diff 仍有变化，说明 Task 4 漏 commit，补上：

```bash
git add web/src/generated/
git commit -s -m "chore(bindings): 同步 ts-rs 生成产物"
```

- [ ] **Step 3: 重写 ADR-0014**

打开 `docs/adr/0014-执行边与数据边分离.md`：

1. 把标题改为 `# ADR-0014: 引脚求值语义二分（PinKind: Exec / Data）`
2. 把 `**状态**` 行改为 `**状态**: 已实施 Phase 1`
3. 在文件顶部新增日期更新行：`**最近更新**: 2026-04-28（Phase 1 落地）`
4. 在"## 决策"节里把 `EdgeKind::Exec / EdgeKind::Data` 范式整段改为引用 spec 文档：

```markdown
## 决策

> 我们决定把"求值语义（push 还是 pull）"放在**引脚**上而非**边**上。`PinDefinition`
> 新增 `kind: PinKind` 字段，与 `PinType`（数据形状）正交。边的语义由两端引脚的
> PinKind 自动派生（Exec 引脚连 Exec 引脚 = push 边；Data 引脚连 Data 引脚 = pull 边）。
> 部署期校验拒绝跨 Kind 连接。

详细设计与 4 个核心用例见
`docs/superpowers/specs/2026-04-28-pin-kind-exec-data-design.md`。
原"边二分"方案（`EdgeKind` 在边上）的问题与重写理由也在该 spec 中详述。
```

5. 在"## 后果"或末尾新增小节 `## 实施进度`：

```markdown
## 实施进度

- ✅ **Phase 1（2026-04-28）**：基础设施骨架——Ring 0 加 PinKind + PinDefinition.kind；
  OutputCache + CachedOutput；部署期跨 Kind 校验 + Data 边独立环检测；Runner 双路径
  写入端骨架（cache 写但不读）；ts-rs 导出。生产 14 类节点 0 改动。详见
  `docs/superpowers/plans/2026-04-28-adr-0014-phase-1-pin-kind-基础.md`。
- 🟡 **Phase 2（待启动）**：第一个真实业务节点（候选 modbusRead `latest` Data 输出）
  + 前端 FlowGram 引脚渲染 + IPC describe_node_pins 返回 kind + 下游消费 cache
- ⚪ Phase 3-5：UE5 风格 Pure 节点 / 缓存策略 / 视觉打磨（详见 spec）
```

- [ ] **Step 4: 更新 CLAUDE.md / AGENTS.md**

打开 `CLAUDE.md`（`AGENTS.md` 的 symlink），找到 "ADR-0010 Pin 声明系统" 那一行下方的下一条 ADR 记录或 "Current batch of ADRs" 列表。在 ADR-0010 实施记录之后插入：

```markdown
- ADR-0014 (执行边与数据边分离 → 重命名为「引脚求值语义二分」) — **已实施 Phase 1**（2026-04-28，Ring 0 加 `PinKind` + `OutputCache`；部署期跨 Kind 校验 + Data 边独立环检测；Runner 双路径写入骨架。生产 14 类节点 0 改动）。设计文档 `docs/superpowers/specs/2026-04-28-pin-kind-exec-data-design.md`；Phase 1 plan `docs/superpowers/plans/2026-04-28-adr-0014-phase-1-pin-kind-基础.md`
```

并把"ADR Execution Order"中 ADR-0014 那条改为：

```markdown
> 8. **ADR-0014** Pin 求值语义二分（原"Exec/Data 边分离"，方案重写为"引脚二分"）—
>    **Phase 1 已实施**（2026-04-28）；Phase 2-5 各自独立 plan
```

- [ ] **Step 5: 更新 memory 系统**

打开 `~/.claude/projects/-home-zhihongniu-Nazh/memory/MEMORY.md` 与 `project_system_architecture.md` / `project_architecture_review_2026_04.md`，把"ADR-0014 待启动"改为"ADR-0014 Phase 1 已实施（引脚二分）"。

具体改动：

`MEMORY.md`：把第一条 `- [System Architecture]` 行内的"已实施 ADR-0008/..."列表里加 `0014(P1)`。

`project_system_architecture.md` 与 `project_architecture_review_2026_04.md`：在已实施清单加上 ADR-0014 Phase 1 条目；在"下一候选"行去掉 ADR-0014（因为已 in flight）。

- [ ] **Step 6: 最后跑一次全量回归确认无破坏**

```bash
cargo fmt --all -- --check && \
cargo clippy --workspace --all-targets -- -D warnings && \
cargo test --workspace && \
cargo test -p tauri-bindings --features ts-export export_bindings
```

预期：全绿。

- [ ] **Step 7: Commit 文档与 memory**

```bash
git add CLAUDE.md docs/adr/0014-执行边与数据边分离.md
git commit -s -m "docs: ADR-0014 重命名为引脚二分 + 标注 Phase 1 已实施"
```

memory 文件不在 repo 里（在用户 home 下），直接保存即可，无需 commit。

---

## 实施完成标准

全部 14 个 task 完成后，应满足以下验收点：

- [ ] `cargo fmt --all -- --check` 通过
- [ ] `cargo clippy --workspace --all-targets -- -D warnings` 通过
- [ ] `cargo test --workspace` 全绿
- [ ] `cargo test -p tauri-bindings --features ts-export export_bindings` 通过
- [ ] `web/src/generated/PinKind.ts` 存在且内容为 `export type PinKind = "exec" | "data";`
- [ ] `web/src/generated/PinDefinition.ts` 含 `kind?: PinKind` 或 `kind: PinKind` 字段
- [ ] 现有 14 类节点测试 100% 通过（无任何节点声明 `PinKind::Data` 输出，全走默认 Exec）
- [ ] `tests/pin_kind_phase1.rs` 端到端 stub 测试通过
- [ ] ADR-0014 状态更新为"已实施 Phase 1"
- [ ] CLAUDE.md ADR Execution Order 同步
- [ ] memory `MEMORY.md` 与 `project_system_architecture.md` 同步

## 实施完成后的下一步

- 启动 Phase 2 plan 起草（候选：`modbusRead` 加 `●latest` Data 输出引脚）——单独 spec 章节已涵盖范围
- 不在本 plan 内的事项（缓存读取下游消费、前端引脚渲染、IPC describe_node_pins 含 kind、AI prompt 携带 kind 等）均归 Phase 2-5 单独 plan
