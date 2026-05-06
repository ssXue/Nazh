> **Status:** merged in f1f23a2

# ADR-0014 Phase 3 实施计划：UE5 风格 Pure 节点（pull 路径首次激活）

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 把 ADR-0014 的"求值语义二分"从 Phase 2（modbusRead 单节点 Data 输出 / 仅写缓存）推进到 Phase 3——Runner **首次激活 pull 路径**：当一个被 Exec 触发的节点声明了 Data 输入引脚时，在 transform 之前从上游 OutputCache 拉值；上游若是 pure-form（无 Exec 引脚）节点则递归求值。同时引入第一批纯计算节点 `c2f` / `minutesSince`，它们仅声明 Data 引脚，部署时不进 Tokio task spawn 列表，作为 UE5 Blueprint 风格"表达式树"的 MVP。

**Architecture:**
- **Ring 0**：`crates/core/src/node.rs` 新增 `is_pure_form(node: &dyn NodeTrait) -> bool` 自由函数（`input_pins` / `output_pins` 全无 Exec 引脚）。`crates/core/src/error.rs` 新增 `EngineError::DataPinUpstreamMissing` 和 `EngineError::DataPinCacheEmpty` 两类拉路径错误。
- **新 crate `crates/nodes-pure/`**：容纳"无副作用纯计算节点"。Phase 3 实现两个：`c2f`（摄氏 → 华氏，单 Float 输入 / 单 Float 输出 / 全 Data）、`minutesSince`（给定 RFC3339 时间戳，输出当前距其分钟数 / 单 String 输入 / 单 Integer 输出 / 全 Data）。`PurePlugin::register` 用 `NodeCapabilities::PURE` 标注（与 ADR-0011 PURE 正交：pure-form 是引脚形态，PURE capability 是缓存优化提示，二者今天都打上）。
- **`src/graph/`**：`deploy.rs` 阶段 0.5 多构造一份 `EdgesByConsumer` 反向索引（每个 consumer node id → 其所有 Data 入边的 (input_pin_id, upstream_node_id, upstream_pin_id) 三元组），spawn 阶段对 pure-form 节点跳过 `runtime.spawn(run_node(...))`。`runner.rs` 在 transform 之前调 `pull_data_inputs()` 收集 Data 输入值并合并进 payload；非 pure-form 上游读 `OutputCache`，pure-form 上游递归求值（`Box::pin` 处理 async recursion）。pull collector 抽到独立模块 `src/graph/pull.rs`。
- **前端**：`web/src/lib/pin-compat.ts` 新增 `isPureForm(input_pins, output_pins)`；`FlowgramCanvas.tsx` 把节点 DOM 加 `data-pure-form="true"` attribute；`web/src/styles/flowgram.css` 给 `[data-pure-form="true"]` 加绿色头 + 圆角胶囊形（参考 UE5 Blueprint Pure 节点视觉）。新增两个 NodeDefinition 文件 + 调色板分类"纯计算"。
- **跨语言契约**：fixture `tests/fixtures/pure_form_matrix.jsonc` 列举 4 类节点的 pure-form 判定（pure 全 Data / 混合 Exec+Data / 全 Exec / 仅 Data 输出但有 Exec 输入），Rust + Vitest 各自消费同一份 fixture。

**Tech Stack:**
- Rust：新 crate `crates/nodes-pure/`，扩展 `crates/core/src/node.rs` + `crates/core/src/error.rs`，重构 `src/graph/runner.rs`（拆 `pull.rs`）+ `src/graph/deploy.rs`，集成测试 `tests/pin_kind_phase3.rs`
- TypeScript / React 18 / FlowGram.AI（`web/src/lib/pin-compat.ts`、`web/src/components/FlowgramCanvas.tsx`、`web/src/styles/flowgram.css`、`web/src/components/flowgram/nodes/{c2f,minutes-since}.ts`、`web/src/components/flowgram/flowgram-node-library.ts`、`web/src/components/flowgram/nodes/catalog.ts`）
- Vitest（`web/src/lib/__tests__/pin-compat.test.ts` 扩展）/ Playwright（`web/e2e/pin-kind-pure-nodes.spec.ts`）
- ts-rs（自动覆盖：`PinKind` 已导出，本 Phase 不引入新 ts-rs 类型）

---

## File Structure

| 操作 | 路径 | 责任 |
|------|------|------|
| 修改 | `crates/core/src/node.rs` | 新增 `pub fn is_pure_form(node: &dyn NodeTrait) -> bool` 自由函数 + `#[cfg(test)]` 单测 |
| 修改 | `crates/core/src/lib.rs` | re-export `is_pure_form` |
| 修改 | `crates/core/src/error.rs` | 新增 `DataPinUpstreamMissing { consumer, pin }` + `DataPinCacheEmpty { upstream, pin }` 两类错误 |
| 创建 | `crates/nodes-pure/Cargo.toml` | 新 crate manifest（仅依赖 `nazh-core` + `async-trait` + `serde` + `serde_json` + `uuid` + `chrono`） |
| 创建 | `crates/nodes-pure/src/lib.rs` | `PurePlugin` + 节点 re-export |
| 创建 | `crates/nodes-pure/src/c2f.rs` | `C2fNode` 实现 + `#[cfg(test)]` 单测 |
| 创建 | `crates/nodes-pure/src/minutes_since.rs` | `MinutesSinceNode` 实现 + `#[cfg(test)]` 单测 |
| 创建 | `crates/nodes-pure/AGENTS.md` | crate 设计意图 / 节点目录 / 内部约定 |
| 修改 | `Cargo.toml` (workspace) | members 加 `crates/nodes-pure`，workspace deps 加 `nodes-pure = { path = "crates/nodes-pure" }` |
| 修改 | `Cargo.toml` (facade `nazh-engine`) | dependencies 加 `nodes-pure.workspace = true` |
| 修改 | `src/lib.rs` | `host.load(&PurePlugin)` 注册到 `standard_registry()`，re-export `c2f` / `minutesSince` 节点 |
| 修改 | `src/registry.rs` | 新增 `pure_plugin_注册全部纯计算节点()` 集成测试 |
| 创建 | `src/graph/pull.rs` | `EdgesByConsumer` 索引构造 + `pull_data_inputs` async 函数（Box::pin 递归求值 pure-form 上游） |
| 修改 | `src/graph/mod.rs` (即 `src/graph.rs`) | `pub(crate) mod pull;` |
| 修改 | `src/graph/deploy.rs` | 阶段 0.5 调 `pull::build_edges_by_consumer`；spawn 阶段对 pure-form 节点跳过 `runtime.spawn(run_node(...))` 并不创建对应 mpsc channel；`run_node` 调用增 `Arc<EdgesByConsumer>` + `Arc<HashMap<String, Arc<dyn NodeTrait>>>` + `Arc<HashMap<String, Arc<OutputCache>>>` 三个新参数 |
| 修改 | `src/graph/runner.rs` | transform 前调 `pull::pull_data_inputs(...)` 合并到 payload；fn 签名增 3 个参数 |
| 创建 | `tests/pin_kind_phase3.rs` | 集成测试：`c2f → minutesSince` 链 + Exec 触发 fmt-style 节点拉取整链 |
| 创建 | `tests/fixtures/pure_form_matrix.jsonc` | 4 配对穷尽 pure-form 判定 fixture |
| 创建 | `crates/core/tests/pure_form_contract.rs` | Rust 侧 fixture 消费 + 断言 |
| 创建 | `web/src/components/flowgram/nodes/c2f.ts` | NodeDefinition：单 Data Float 输入 / 单 Data Float 输出 |
| 创建 | `web/src/components/flowgram/nodes/minutes-since.ts` | NodeDefinition：单 Data String 输入 / 单 Data Integer 输出 |
| 修改 | `web/src/components/flowgram/flowgram-node-library.ts` | 两个 def import + `ALL_DEFS` 入栈 + 调色板分类"纯计算" |
| 修改 | `web/src/components/flowgram/nodes/catalog.ts` | 新增"纯计算"分类常量 |
| 修改 | `web/src/lib/pin-compat.ts` | 新增 `isPureForm(input_pins, output_pins): boolean` |
| 修改 | `web/src/lib/__tests__/pin-compat.test.ts` | 新增 fixture 共享测试块（消费 `pure_form_matrix.jsonc`） |
| 修改 | `web/src/components/FlowgramCanvas.tsx` | `FlowgramNodeCard` 渲染时根据 schema 加 `data-pure-form` attribute |
| 修改 | `web/src/styles/flowgram.css` | `[data-pure-form="true"]` 节点头部绿色 + 圆角胶囊形 |
| 创建 | `web/e2e/pin-kind-pure-nodes.spec.ts` | Playwright DOM 烟雾：拖入 c2f 后断言 `data-pure-form="true"` + 头部背景色 |
| 修改 | `docs/adr/0014-执行边与数据边分离.md` | "实施进度" 章节追加 Phase 3 段（含 commit 范围） |
| 修改 | `AGENTS.md` | "Phases 1-5 complete..." 段 + ADR Execution Order 表 ADR-0014 进度更新 |
| 修改 | `~/.claude/projects/-home-zhihongniu-Nazh/memory/MEMORY.md` + `project_system_architecture.md` + `project_architecture_review_2026_04.md` | Phase 3 落地状态同步 |

---

## Out of scope（明确不做的事，后续 plan 各自处理）

1. **`lookup` 节点**——配置驱动（携带 lookup table），结构与 c2f / minutesSince 不同质，单独 plan
2. **混合输入节点（Exec ▶in + Data ●xxx）的 payload 合并语义**——本 Phase 拉路径只在被 Exec 触发的下游节点上对 Data 输入做 collect；payload 合并约定（Exec payload spread 还是放 `"in"` 键下）以两个 pure node 不带 Exec 输入即可避开。Phase 3b 引入 `formatJson` 时再敲定
3. **PinKind ↔ 子图 4 处交叉点**（`subgraphInput/Output` 无 PinKind 配置 / `passthrough` 默认 Any/Exec / `flattenSubgraphs` 保留 PinKind 义务 / 容器节点 cache 槽语义）——Phase 3 的 pure 节点全部在主图存在即可被验证；子图穿透留给"ADR-0013 + ADR-0014 集成" 独立 plan
4. **Pure 节点的输入哈希缓存（ADR-0011 PURE 优化）**——本 Phase 每次 pull 都重新执行 pure 节点（同 trace 内多次拉取同一 pure 节点亦重算），缓存策略归 Phase 4
5. **缓存空槽兜底策略**（`default_value` / `block_until_ready` / `skip`）——本 Phase 空槽返回 `EngineError::DataPinCacheEmpty`，归 Phase 4
6. **节点头部色按 capability 自动着色（Trigger / Branching / 普通）**——本 Phase 仅对 pure-form 上绿头；Trigger 红头 / Branching 蓝头归 Phase 5
7. **AI 脚本生成 prompt 携带 PinKind/pure-form 信息**——归 Phase 5

---

## Task 1: Ring 0 `is_pure_form` helper + 错误类型

**Files:**
- Modify: `crates/core/src/node.rs` — 加 `pub fn is_pure_form` 与 `#[cfg(test)]` 单测
- Modify: `crates/core/src/error.rs` — 加 `DataPinUpstreamMissing` + `DataPinCacheEmpty`
- Modify: `crates/core/src/lib.rs` — re-export

- [ ] **Step 1: 在 `crates/core/src/node.rs` 末尾加 `is_pure_form` 自由函数（在 `#[cfg(test)]` mod 之前）**

```rust
/// 判定节点是否为 **pure-form**（UE5 Blueprint 风格的"表达式节点"）。
///
/// 定义：节点的 `input_pins` 与 `output_pins` 中**没有任何** [`PinKind::Exec`]
/// 引脚——意味着它**不参与触发链**：既不会被上游 Exec 边推、也不会向下游 Exec 推。
/// 此种节点在 [`deploy_workflow`](../graph/deploy.rs) 的 spawn 阶段被跳过 Tokio
/// task 创建，仅在被下游 Data 输入拉取时按需 `transform`（递归求值）。
///
/// **与 [`NodeCapabilities::PURE`] 的关系**：正交。
/// - `is_pure_form` 看引脚形态，由 `input_pins` / `output_pins` 自动推导
/// - `PURE` capability 是节点作者声明的"同输入同输出 + 无副作用"承诺，启用
///   未来 Phase 4 的输入哈希缓存。
///
/// 一个节点可以是 pure-form 而不打 PURE（少见，谨慎），也可以是 PURE 而非
/// pure-form（如 `if` / `switch`——参与触发链的纯函数）。`c2f` / `minutesSince`
/// 这种"理想 pure 计算节点"两者都满足。
pub fn is_pure_form(node: &dyn NodeTrait) -> bool {
    let no_exec_input = node
        .input_pins()
        .iter()
        .all(|p| p.kind != crate::PinKind::Exec);
    let no_exec_output = node
        .output_pins()
        .iter()
        .all(|p| p.kind != crate::PinKind::Exec);
    no_exec_input && no_exec_output
}
```

- [ ] **Step 2: 在 `crates/core/src/node.rs` 的 `#[cfg(test)]` 模块加 4 个单测**

```rust
#[cfg(test)]
mod is_pure_form_tests {
    use super::*;
    use crate::{PinDefinition, PinKind, PinType};
    use async_trait::async_trait;
    use serde_json::Value;
    use uuid::Uuid;

    struct StubNode {
        inputs: Vec<PinDefinition>,
        outputs: Vec<PinDefinition>,
    }

    #[async_trait]
    impl NodeTrait for StubNode {
        fn id(&self) -> &str { "stub" }
        fn kind(&self) -> &str { "stub" }
        fn input_pins(&self) -> Vec<PinDefinition> { self.inputs.clone() }
        fn output_pins(&self) -> Vec<PinDefinition> { self.outputs.clone() }
        async fn transform(&self, _: Uuid, payload: Value) -> Result<NodeExecution, EngineError> {
            Ok(NodeExecution::single(payload))
        }
    }

    fn data_pin(id: &str, dir: PinDirection) -> PinDefinition {
        PinDefinition {
            id: id.to_owned(), label: id.to_owned(),
            pin_type: PinType::Float, direction: dir,
            required: false, kind: PinKind::Data, description: None,
        }
    }
    fn exec_pin(id: &str, dir: PinDirection) -> PinDefinition {
        PinDefinition {
            id: id.to_owned(), label: id.to_owned(),
            pin_type: PinType::Any, direction: dir,
            required: matches!(dir, PinDirection::Input), kind: PinKind::Exec, description: None,
        }
    }

    #[test]
    fn 全_data_引脚是_pure_form() {
        let n = StubNode {
            inputs: vec![data_pin("in", PinDirection::Input)],
            outputs: vec![data_pin("out", PinDirection::Output)],
        };
        assert!(is_pure_form(&n));
    }

    #[test]
    fn 输入混_exec_不是_pure_form() {
        let n = StubNode {
            inputs: vec![exec_pin("in", PinDirection::Input)],
            outputs: vec![data_pin("out", PinDirection::Output)],
        };
        assert!(!is_pure_form(&n));
    }

    #[test]
    fn 输出混_exec_不是_pure_form() {
        let n = StubNode {
            inputs: vec![data_pin("in", PinDirection::Input)],
            outputs: vec![exec_pin("out", PinDirection::Output)],
        };
        assert!(!is_pure_form(&n));
    }

    #[test]
    fn 仅有输出且全_data_仍是_pure_form() {
        // 例如"设备表"节点——无输入、单 Data 输出
        let n = StubNode {
            inputs: vec![],
            outputs: vec![data_pin("out", PinDirection::Output)],
        };
        assert!(is_pure_form(&n));
    }
}
```

- [ ] **Step 3: 运行测试确认失败（is_pure_form 未定义）**

Run: `cargo test -p nazh-core node::is_pure_form_tests -- --nocapture`
Expected: 编译失败 `cannot find function 'is_pure_form'`（说明 Step 1 还没保存）。如果 Step 1 已保存则全 4 测试 PASS——此时跳到 Step 4。

- [ ] **Step 4: 在 `crates/core/src/error.rs` 的 `EngineError` 枚举里加两个新 variant**

定位 `pub enum EngineError {` 块（用 grep 找），在已有 `UnknownPin { ... }` variant 旁加：

```rust
    /// ADR-0014 Phase 3：被 Exec 触发的下游节点声明了 Data 输入引脚，但图中
    /// 找不到指向该 pin 的 Data 边。部署期 `pin_validator` 应已捕获 `required`
    /// 输入缺边——本错误用于运行时 Data 收集器对非 required Data 输入的兜底。
    #[error("节点 `{consumer}` 的 Data 输入引脚 `{pin}` 没有上游 Data 边")]
    DataPinUpstreamMissing { consumer: String, pin: String },

    /// ADR-0014 Phase 3：从上游节点的 Data 输出缓存槽读取时槽位为空——
    /// 上游节点尚未执行过 transform。Phase 3 直接拒绝；Phase 4 引入引脚级
    /// 兜底策略（`default_value` / `block_until_ready` / `skip`）后此错误
    /// 仅在 `block_until_ready` 超时时触发。
    #[error("上游节点 `{upstream}` 的 Data 输出引脚 `{pin}` 缓存为空（尚未执行）")]
    DataPinCacheEmpty { upstream: String, pin: String },
```

- [ ] **Step 5: 在 `crates/core/src/lib.rs` 的 re-export 块加 `is_pure_form`**

定位现有 `pub use node::{...}` 行，把 `is_pure_form` 加入 re-export 集合。

- [ ] **Step 6: 全测试通过 + commit**

```bash
cargo test -p nazh-core
git add crates/core/src/node.rs crates/core/src/error.rs crates/core/src/lib.rs
git commit -s -m "feat(core): ADR-0014 Phase 3 加 is_pure_form helper + Data pin 拉路径错误"
```

---

## Task 2: 新 crate `crates/nodes-pure/` 骨架 + workspace 注册

**Files:**
- Create: `crates/nodes-pure/Cargo.toml`
- Create: `crates/nodes-pure/src/lib.rs`
- Create: `crates/nodes-pure/AGENTS.md`
- Modify: 根 `Cargo.toml` workspace `members` + workspace deps + facade dependencies

- [ ] **Step 1: 创建 `crates/nodes-pure/Cargo.toml`**

```toml
[package]
name = "nodes-pure"
description = "Nazh 纯计算节点：c2f / minutesSince 等无副作用变换（Ring 1）"
version.workspace = true
edition.workspace = true
rust-version.workspace = true
license.workspace = true
authors.workspace = true
repository.workspace = true

[lints]
workspace = true

[dependencies]
nazh-core.workspace = true
async-trait.workspace = true
serde.workspace = true
serde_json.workspace = true
uuid.workspace = true
chrono.workspace = true

[dev-dependencies]
tokio = { workspace = true, features = ["macros", "rt"] }
```

- [ ] **Step 2: 创建 `crates/nodes-pure/src/lib.rs` 占位（PurePlugin 注册空集合）**

```rust
//! Nazh 纯计算节点（Ring 1）：c2f / minutesSince 等无副作用变换。
//!
//! ADR-0014 Phase 3 引入。所有节点的 input/output 引脚均为
//! [`PinKind::Data`](nazh_core::PinKind::Data)，即 [`is_pure_form`](nazh_core::is_pure_form)
//! 判定为 `true`——它们不参与触发链，仅在被下游 Data 输入拉取时即时求值。
//!
//! 同时打上 [`NodeCapabilities::PURE`](nazh_core::NodeCapabilities::PURE)
//! capability：与 ADR-0011 PURE 优化提示语义一致（同输入同输出 / 无副作用），
//! 为未来 Phase 4 输入哈希缓存奠定元数据基础。

use nazh_core::{NodeCapabilities, NodeRegistry, Plugin, PluginManifest};

pub struct PurePlugin;

impl Plugin for PurePlugin {
    fn manifest(&self) -> PluginManifest {
        PluginManifest {
            name: "nodes-pure",
            version: env!("CARGO_PKG_VERSION"),
        }
    }

    fn register(&self, _registry: &mut NodeRegistry) {
        // Phase 3 Task 3 / Task 4 接入 c2f / minutesSince
        let _ = NodeCapabilities::PURE;
    }
}
```

- [ ] **Step 3: 创建 `crates/nodes-pure/AGENTS.md`**

```markdown
# `crates/nodes-pure` — 纯计算节点（Ring 1）

## 这是什么

Nazh 的"无副作用纯函数"节点集合。所有节点声明仅 `PinKind::Data` 引脚，部署
时由 [`crate::is_pure_form`](nazh_core::is_pure_form) 判定为 pure-form，被下游
Data 输入拉取时即时求值（不进 Tokio task spawn 列表）。

## 当前节点目录（2026-04-28）

| 节点 kind | 输入 | 输出 | capability |
|-----------|------|------|------------|
| `c2f` | `value: Float` (Data) | `out: Float` (Data) | `PURE` |
| `minutesSince` | `since: String` (Data, RFC3339) | `out: Integer` (Data) | `PURE` |

## 内部约定

- 节点必须真·无副作用：不读时钟（`minutesSince` 是例外，明确依赖 `Utc::now()`，
  这是它的语义本身）、不发起 IO、不读 `WorkflowVariables`、不调 `AiService`
- 节点必须线程安全（`Send + Sync`）——递归 pull 求值在不同 task 上下文里
- 错误返回 `EngineError::Node { stage, source }`，`source` 携带具体 chain
  （类型不匹配 / 解析失败 / 数学溢出等）

## 修改本 crate 时

- 加新节点：在 `lib.rs` 的 `PurePlugin::register` 加 `register_with_capabilities(...)` +
  写 `mod xxx; pub use xxx::XxxNode;` + 同步更新本 AGENTS.md 节点目录表 +
  根 `src/registry.rs` 的 `pure_plugin_注册全部纯计算节点` 集成测试断言列表
- 节点必须有单元测试覆盖：（a）正常输入产出预期值（b）类型不匹配返回错误
  （c）边界条件（如 c2f 极大极小温度 / minutesSince 非法时间戳）

## 依赖约束

仅依赖 `nazh-core` + `async-trait` + `serde` + `serde_json` + `uuid` + `chrono`。
**不得**依赖 `connections` / `scripting` / `nodes-flow` / `nodes-io` / `ai`——
本 crate 是 Ring 1 中"零协议依赖"的最小子集，体现 pure 节点纯度。
```

- [ ] **Step 4: 修改根 `Cargo.toml` workspace block**

```toml
[workspace]
members = [".", "crates/core", "crates/pipeline", "crates/connections", "crates/scripting", "crates/nodes-flow", "crates/nodes-io", "crates/nodes-pure", "crates/ai", "crates/tauri-bindings", "src-tauri"]
```

在 `[workspace.dependencies]` 块加：

```toml
nodes-pure = { path = "crates/nodes-pure" }
```

- [ ] **Step 5: 修改 facade `nazh-engine` 的 `[dependencies]` 块**

定位现有 `nodes-flow.workspace = true`，紧邻加：

```toml
nodes-pure.workspace = true
```

- [ ] **Step 6: 验证 workspace 构建**

```bash
cargo build -p nodes-pure
cargo build --workspace
```

Expected: 两条命令均成功。

- [ ] **Step 7: commit**

```bash
git add crates/nodes-pure/ Cargo.toml
git commit -s -m "feat(nodes-pure): ADR-0014 Phase 3 新 crate 骨架 + workspace 注册"
```

---

## Task 3: 实现 `c2f` 节点 + 单元测试

**Files:**
- Create: `crates/nodes-pure/src/c2f.rs`
- Modify: `crates/nodes-pure/src/lib.rs` — `mod c2f; pub use c2f::C2fNode;` + register

- [ ] **Step 1: 创建 `crates/nodes-pure/src/c2f.rs`**

```rust
//! `c2f` 节点：摄氏转华氏。
//!
//! pure-form：单 Data Float 输入 (`value`)，单 Data Float 输出 (`out`)。
//! 公式：`out = value * 9.0 / 5.0 + 32.0`。

use async_trait::async_trait;
use nazh_core::{
    EngineError, NodeExecution, NodeTrait, PinDefinition, PinDirection, PinKind, PinType,
};
use serde_json::Value;
use uuid::Uuid;

pub struct C2fNode {
    id: String,
}

impl C2fNode {
    pub fn new(id: String) -> Self {
        Self { id }
    }

    fn data_input() -> PinDefinition {
        PinDefinition {
            id: "value".to_owned(),
            label: "摄氏度".to_owned(),
            pin_type: PinType::Float,
            direction: PinDirection::Input,
            required: true,
            kind: PinKind::Data,
            description: Some("待转换的摄氏温度（Float）".to_owned()),
        }
    }

    fn data_output() -> PinDefinition {
        PinDefinition {
            id: "out".to_owned(),
            label: "华氏度".to_owned(),
            pin_type: PinType::Float,
            direction: PinDirection::Output,
            required: false,
            kind: PinKind::Data,
            description: Some("转换后的华氏温度（Float）".to_owned()),
        }
    }
}

#[async_trait]
impl NodeTrait for C2fNode {
    fn id(&self) -> &str {
        &self.id
    }
    fn kind(&self) -> &str {
        "c2f"
    }
    fn input_pins(&self) -> Vec<PinDefinition> {
        vec![Self::data_input()]
    }
    fn output_pins(&self) -> Vec<PinDefinition> {
        vec![Self::data_output()]
    }

    async fn transform(
        &self,
        _trace_id: Uuid,
        payload: Value,
    ) -> Result<NodeExecution, EngineError> {
        // payload 由 Runner pull 收集器构造为 `{ "value": <Float> }`。
        let celsius = payload
            .get("value")
            .and_then(Value::as_f64)
            .ok_or_else(|| EngineError::node_error(
                self.id.clone(),
                "c2f 节点期望 payload.value 为 Float",
            ))?;
        let fahrenheit = celsius * 9.0 / 5.0 + 32.0;
        // 单 Data 输出节点，payload 用 `{ "out": ... }` 与 input 对偶
        Ok(NodeExecution::single(serde_json::json!({ "out": fahrenheit })))
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn 摄氏_0_转换为华氏_32() {
        let node = C2fNode::new("c2f_1".to_owned());
        let result = node
            .transform(Uuid::nil(), serde_json::json!({ "value": 0.0 }))
            .await
            .unwrap();
        let out = &result.outputs[0].payload;
        assert!((out.get("out").unwrap().as_f64().unwrap() - 32.0).abs() < 1e-9);
    }

    #[tokio::test]
    async fn 摄氏_100_转换为华氏_212() {
        let node = C2fNode::new("c2f_1".to_owned());
        let result = node
            .transform(Uuid::nil(), serde_json::json!({ "value": 100.0 }))
            .await
            .unwrap();
        let out = &result.outputs[0].payload;
        assert!((out.get("out").unwrap().as_f64().unwrap() - 212.0).abs() < 1e-9);
    }

    #[tokio::test]
    async fn payload_缺_value_键返回错误() {
        let node = C2fNode::new("c2f_1".to_owned());
        let err = node
            .transform(Uuid::nil(), serde_json::json!({}))
            .await
            .unwrap_err();
        assert!(matches!(err, EngineError::Node { .. }));
    }

    #[test]
    fn c2f_是_pure_form() {
        let node = C2fNode::new("c2f_1".to_owned());
        assert!(nazh_core::is_pure_form(&node));
    }
}
```

- [ ] **Step 2: 修改 `crates/nodes-pure/src/lib.rs` 把 c2f 接入**

替换 lib.rs 的 `register` 函数体：

```rust
    fn register(&self, registry: &mut NodeRegistry) {
        registry.register_with_capabilities("c2f", NodeCapabilities::PURE, |def, _res| {
            Ok(std::sync::Arc::new(C2fNode::new(def.id().to_owned())))
        });
    }
```

并在文件顶端 use 列表加：

```rust
mod c2f;
pub use c2f::C2fNode;
```

确保 `Plugin` trait import 已有 `NodeRegistry`（已 import）。

- [ ] **Step 3: 跑 c2f 单元测试**

```bash
cargo test -p nodes-pure
```

Expected: 4 个测试全 PASS（3 个 transform + 1 个 pure_form 断言）。

- [ ] **Step 4: 检查 `EngineError::node_error` 是否存在；若无则需查看 error.rs 用什么构造方式**

```bash
grep -n "fn node_error\|fn invalid_graph" crates/core/src/error.rs | head
```

如果只有 `invalid_graph` helper 没有 `node_error`，请改为：

```rust
EngineError::Node {
    stage: self.id.clone(),
    source: nazh_core::NodeError::message("c2f 节点期望 payload.value 为 Float"),
}
```

或匹配 EngineError 实际的 `Node` variant 结构（用 grep 确认）。**Step 1 的代码示例若与实际 EngineError API 不符，必须以实际为准修改。**

- [ ] **Step 5: commit**

```bash
git add crates/nodes-pure/src/c2f.rs crates/nodes-pure/src/lib.rs
git commit -s -m "feat(nodes-pure): 实现 c2f 节点（pure-form 单 Data Float in/out）"
```

---

## Task 4: 实现 `minutesSince` 节点 + 单元测试

**Files:**
- Create: `crates/nodes-pure/src/minutes_since.rs`
- Modify: `crates/nodes-pure/src/lib.rs` — `mod minutes_since; pub use ...;` + register

- [ ] **Step 1: 创建 `crates/nodes-pure/src/minutes_since.rs`**

```rust
//! `minutesSince` 节点：给定 RFC3339 时间戳字符串，返回当前距其分钟数。
//!
//! pure-form：单 Data String 输入 (`since`)，单 Data Integer 输出 (`out`)。
//! 时钟来源 [`chrono::Utc::now()`]——节点对系统时钟有显式依赖，但这是节点
//! 语义本身，并不构成"副作用"（无外部 IO、无 mutable state）。

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use nazh_core::{
    EngineError, NodeExecution, NodeTrait, PinDefinition, PinDirection, PinKind, PinType,
};
use serde_json::Value;
use uuid::Uuid;

pub struct MinutesSinceNode {
    id: String,
}

impl MinutesSinceNode {
    pub fn new(id: String) -> Self {
        Self { id }
    }

    fn data_input() -> PinDefinition {
        PinDefinition {
            id: "since".to_owned(),
            label: "起点时间".to_owned(),
            pin_type: PinType::String,
            direction: PinDirection::Input,
            required: true,
            kind: PinKind::Data,
            description: Some("RFC3339 格式时间戳（如 `2026-04-28T08:00:00Z`）".to_owned()),
        }
    }

    fn data_output() -> PinDefinition {
        PinDefinition {
            id: "out".to_owned(),
            label: "距今分钟数".to_owned(),
            pin_type: PinType::Integer,
            direction: PinDirection::Output,
            required: false,
            kind: PinKind::Data,
            description: Some("`Utc::now() - since` 的分钟数（向下取整）".to_owned()),
        }
    }
}

#[async_trait]
impl NodeTrait for MinutesSinceNode {
    fn id(&self) -> &str {
        &self.id
    }
    fn kind(&self) -> &str {
        "minutesSince"
    }
    fn input_pins(&self) -> Vec<PinDefinition> {
        vec![Self::data_input()]
    }
    fn output_pins(&self) -> Vec<PinDefinition> {
        vec![Self::data_output()]
    }

    async fn transform(
        &self,
        _trace_id: Uuid,
        payload: Value,
    ) -> Result<NodeExecution, EngineError> {
        let since_str = payload
            .get("since")
            .and_then(Value::as_str)
            .ok_or_else(|| EngineError::node_error(
                self.id.clone(),
                "minutesSince 节点期望 payload.since 为 RFC3339 字符串",
            ))?;
        let since: DateTime<Utc> = since_str.parse().map_err(|e| {
            EngineError::node_error(
                self.id.clone(),
                format!("minutesSince 解析时间戳失败：{e}"),
            )
        })?;
        let minutes = (Utc::now() - since).num_minutes();
        Ok(NodeExecution::single(
            serde_json::json!({ "out": minutes }),
        ))
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use chrono::Duration;

    #[tokio::test]
    async fn 起点为_5_分钟前返回_5_左右() {
        let node = MinutesSinceNode::new("ms_1".to_owned());
        let five_min_ago = (Utc::now() - Duration::minutes(5)).to_rfc3339();
        let result = node
            .transform(Uuid::nil(), serde_json::json!({ "since": five_min_ago }))
            .await
            .unwrap();
        let out = result.outputs[0]
            .payload
            .get("out")
            .unwrap()
            .as_i64()
            .unwrap();
        // 容忍 0~1 分钟漂移
        assert!((4..=5).contains(&out), "expected 4 or 5, got {out}");
    }

    #[tokio::test]
    async fn 非法_rfc3339_返回错误() {
        let node = MinutesSinceNode::new("ms_1".to_owned());
        let err = node
            .transform(Uuid::nil(), serde_json::json!({ "since": "not-a-date" }))
            .await
            .unwrap_err();
        assert!(matches!(err, EngineError::Node { .. }));
    }

    #[tokio::test]
    async fn payload_缺_since_键返回错误() {
        let node = MinutesSinceNode::new("ms_1".to_owned());
        let err = node
            .transform(Uuid::nil(), serde_json::json!({}))
            .await
            .unwrap_err();
        assert!(matches!(err, EngineError::Node { .. }));
    }

    #[test]
    fn minutes_since_是_pure_form() {
        let node = MinutesSinceNode::new("ms_1".to_owned());
        assert!(nazh_core::is_pure_form(&node));
    }
}
```

- [ ] **Step 2: 在 `crates/nodes-pure/src/lib.rs` 注册 `minutesSince`**

文件顶端 mod / pub use 块加：

```rust
mod minutes_since;
pub use minutes_since::MinutesSinceNode;
```

`register` 函数体追加（紧跟 c2f 之后）：

```rust
        registry.register_with_capabilities(
            "minutesSince",
            NodeCapabilities::PURE,
            |def, _res| {
                Ok(std::sync::Arc::new(MinutesSinceNode::new(def.id().to_owned())))
            },
        );
```

- [ ] **Step 3: 跑测试**

```bash
cargo test -p nodes-pure
```

Expected: 8 个测试全 PASS（c2f 4 + minutesSince 4）。

- [ ] **Step 4: commit**

```bash
git add crates/nodes-pure/src/minutes_since.rs crates/nodes-pure/src/lib.rs
git commit -s -m "feat(nodes-pure): 实现 minutesSince 节点（pure-form 单 Data 输入 / Integer 输出）"
```

---

## Task 5: facade 注册 PurePlugin + registry 契约测试

**Files:**
- Modify: `src/lib.rs` — `host.load(&PurePlugin)` + re-export
- Modify: `src/registry.rs` — 加 `pure_plugin_注册全部纯计算节点` 集成测试

- [ ] **Step 1: 在 `src/lib.rs` 顶端 use 块加 `PurePlugin` import**

定位现有 `pub use nodes_flow::{...}` 行下方，加：

```rust
pub use nodes_pure::{C2fNode, MinutesSinceNode, PurePlugin};
```

- [ ] **Step 2: 在 `standard_registry()` 函数体加注册**

定位 `host.load(&IoPlugin);`，紧邻其后加：

```rust
    host.load(&PurePlugin);
```

- [ ] **Step 3: 在 `src/registry.rs` 测试模块加新 case**

```rust
    #[test]
    fn pure_plugin_注册全部纯计算节点() {
        let registry = standard_registry();
        let types = registry.registered_types();

        for expected in ["c2f", "minutesSince"] {
            assert!(
                types.contains(&expected),
                "PurePlugin 缺少节点类型: {expected}"
            );
        }
    }

    #[test]
    fn pure_plugin_节点带_pure_capability() {
        let registry = standard_registry();
        for kind in ["c2f", "minutesSince"] {
            let caps = registry.capabilities_of(kind).expect("注册");
            assert!(
                caps.contains(NodeCapabilities::PURE),
                "节点 `{kind}` 应带 PURE capability"
            );
        }
    }
```

> **注**：第二个测试如果 `NodeRegistry::capabilities_of` 方法名不同（用 grep 在 plugin.rs 里找实际名字），请改为实际 API。

- [ ] **Step 4: 跑测试**

```bash
cargo test --workspace
```

Expected: 所有原有测试通过 + 新加的两个测试通过。

- [ ] **Step 5: commit**

```bash
git add src/lib.rs src/registry.rs
git commit -s -m "feat: facade standard_registry 注册 PurePlugin（c2f + minutesSince）"
```

---

## Task 6: deploy.rs 阶段 0.5 加 EdgesByConsumer 索引 + spawn 阶段跳过 pure-form

**Files:**
- Create: `src/graph/pull.rs` — `EdgesByConsumer` 类型 + `build_edges_by_consumer` 函数
- Modify: `src/graph.rs` (即 mod 文件) — `pub(crate) mod pull;`
- Modify: `src/graph/deploy.rs` — 调 `build_edges_by_consumer`、传给 run_node、spawn loop 跳过 pure-form
- Modify: `src/graph/runner.rs` — fn 签名增 3 个参数（先占位，实际 pull 调用 Task 7 写）

- [ ] **Step 1: 创建 `src/graph/pull.rs` 占位（Task 7 才写 pull 函数）**

```rust
//! ADR-0014 Phase 3：Data 输入引脚的运行时拉路径。
//!
//! 当一个被 Exec 边触发的下游节点在 [`NodeTrait::input_pins`] 中声明了
//! [`PinKind::Data`](nazh_core::PinKind::Data) 引脚，本模块负责在 Runner 调用
//! `transform` **之前**：
//! 1. 反查每个 Data 输入引脚对应的上游边（[`EdgesByConsumer`]）
//! 2. 上游若为 pure-form 节点 → 递归求值
//! 3. 上游若为 Exec 节点（如 `modbusRead.latest`）→ 读取其 [`OutputCache`]
//! 4. 把收集到的 Data 值合并进 `transform` payload

use std::collections::HashMap;

use super::types::WorkflowEdge;
use super::DEFAULT_OUTPUT_PIN_ID;

/// 反向索引：每个 consumer node id → 其所有 Data 入边的元组列表。
///
/// 元组结构：`(consumer_input_pin_id, upstream_node_id, upstream_output_pin_id)`。
/// `consumer_input_pin_id` 用 `target_port_id` 解析，缺省时为 `"in"`；
/// `upstream_output_pin_id` 用 `source_port_id` 解析，缺省时为 [`DEFAULT_OUTPUT_PIN_ID`]。
#[derive(Debug, Default, Clone)]
pub(crate) struct EdgesByConsumer {
    by_consumer: HashMap<String, Vec<DataInEdge>>,
}

#[derive(Debug, Clone)]
pub(crate) struct DataInEdge {
    pub consumer_input_pin_id: String,
    pub upstream_node_id: String,
    pub upstream_output_pin_id: String,
}

impl EdgesByConsumer {
    pub fn for_consumer(&self, consumer_node_id: &str) -> &[DataInEdge] {
        self.by_consumer
            .get(consumer_node_id)
            .map(Vec::as_slice)
            .unwrap_or(&[])
    }
}

/// 在 [`classify_edges`](super::topology::classify_edges) 已分出的 `data_edges`
/// 上构造反向索引。
pub(crate) fn build_edges_by_consumer<'a>(
    data_edges: &[&'a WorkflowEdge],
) -> EdgesByConsumer {
    let mut by_consumer: HashMap<String, Vec<DataInEdge>> = HashMap::new();
    for edge in data_edges {
        let entry = DataInEdge {
            consumer_input_pin_id: edge
                .target_port_id
                .clone()
                .unwrap_or_else(|| "in".to_owned()),
            upstream_node_id: edge.from.clone(),
            upstream_output_pin_id: edge
                .source_port_id
                .clone()
                .unwrap_or_else(|| DEFAULT_OUTPUT_PIN_ID.to_owned()),
        };
        by_consumer
            .entry(edge.to.clone())
            .or_default()
            .push(entry);
    }
    EdgesByConsumer { by_consumer }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use crate::graph::types::WorkflowEdge;

    fn data_edge(from: &str, sport: Option<&str>, to: &str, tport: Option<&str>) -> WorkflowEdge {
        WorkflowEdge {
            from: from.to_owned(),
            to: to.to_owned(),
            source_port_id: sport.map(ToOwned::to_owned),
            target_port_id: tport.map(ToOwned::to_owned),
        }
    }

    #[test]
    fn 单_data_边构造单 entry() {
        let e = data_edge("up", Some("latest"), "down", Some("temp"));
        let refs = vec![&e];
        let idx = build_edges_by_consumer(&refs);
        let entries = idx.for_consumer("down");
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].consumer_input_pin_id, "temp");
        assert_eq!(entries[0].upstream_node_id, "up");
        assert_eq!(entries[0].upstream_output_pin_id, "latest");
    }

    #[test]
    fn 多个_data_边按 consumer 分组() {
        let e1 = data_edge("up1", Some("o1"), "down", Some("a"));
        let e2 = data_edge("up2", Some("o2"), "down", Some("b"));
        let refs = vec![&e1, &e2];
        let idx = build_edges_by_consumer(&refs);
        assert_eq!(idx.for_consumer("down").len(), 2);
        assert!(idx.for_consumer("missing").is_empty());
    }

    #[test]
    fn 缺端口 id 默认到_in_和_out() {
        let e = data_edge("up", None, "down", None);
        let refs = vec![&e];
        let idx = build_edges_by_consumer(&refs);
        let entries = idx.for_consumer("down");
        assert_eq!(entries[0].consumer_input_pin_id, "in");
        assert_eq!(entries[0].upstream_output_pin_id, "out");
    }
}
```

- [ ] **Step 2: 在 `src/graph.rs` 加 `pub(crate) mod pull;`**

定位 `pub(crate) mod runner;`，紧邻加：

```rust
pub(crate) mod pull;
```

- [ ] **Step 3: 在 `src/graph/deploy.rs` 阶段 0.5 末尾构造 `EdgesByConsumer`**

定位 `detect_data_edge_cycle(&classified.data_edges)?;`，紧邻其后加：

```rust
    let edges_by_consumer = std::sync::Arc::new(super::pull::build_edges_by_consumer(
        &classified.data_edges,
    ));
```

并把 `nodes_by_id` 在 spawn 阶段之前转换为可共享的 Arc 索引以传给 run_node：

```rust
    // run_node 需要在拉路径中递归求值 pure-form 上游节点，需要持有所有节点的 Arc 句柄
    let nodes_index: std::sync::Arc<
        std::collections::HashMap<String, std::sync::Arc<dyn NodeTrait>>,
    > = std::sync::Arc::new(nodes_by_id.iter().map(|(k, v)| (k.clone(), Arc::clone(v))).collect());
```

> **注意**：原本 `nodes_by_id` 是用 `nodes_by_id.remove(node_id)` 在 spawn loop 里取出的。Phase 3 改为 **clone Arc 后保留索引** —— 不能 remove。把 spawn loop 里的 `nodes_by_id.remove(node_id)` 改为 `nodes_index.get(node_id).cloned()`。

- [ ] **Step 4: 在 spawn loop 跳过 pure-form 节点**

定位 spawn loop（`for node_id in &topology.deployment_order { ... runtime.spawn(run_node(...)); }`），在 spawn 之前加判定：

```rust
        let Some(node) = nodes_index.get(node_id).cloned() else {
            return Err(EngineError::invalid_graph(format!(
                "节点 `{node_id}` 在阶段 2 缺失"
            )));
        };

        // ADR-0014 Phase 3：pure-form 节点不参与触发链——不创建 input channel、
        // 不 spawn run_node task。它们仅在被下游 Data 输入拉取时即时求值。
        if nazh_core::is_pure_form(node.as_ref()) {
            // 释放预创建的 channel——pure 节点不会从 senders/receivers 收消息
            senders.remove(node_id);
            receivers.remove(node_id);
            continue;
        }
```

注意**保留** `nodes_index` 中的 Arc（pull collector 需要它递归调 transform）。`senders.remove` 释放掉的好处是：上游 Exec 边若误连 pure-form 节点，部署期 pin_validator 已经拒绝（pure-form 没有 Exec 输入），所以这里 channel 删除安全。

- [ ] **Step 5: run_node 函数签名加 3 个参数（先占位，Task 7 实写 pull）**

修改 `crates/core/...`（不对，run_node 在 `src/graph/runner.rs`），fn 签名加：

```rust
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
    // ADR-0014 Phase 3 拉路径所需 ↓
    edges_by_consumer: Arc<super::pull::EdgesByConsumer>,
    nodes_index: Arc<HashMap<String, Arc<dyn NodeTrait>>>,
    output_caches_index: Arc<HashMap<String, Arc<OutputCache>>>,
) {
    // ... 现有函数体不变（Task 7 才插入 pull 调用）
}
```

deploy.rs 的 `runtime.spawn(run_node(...))` 调用处增三个 Arc 参数。`output_caches_index` 由现有 `output_caches: HashMap<String, Arc<OutputCache>>` Arc 包装：

```rust
    let output_caches_index = std::sync::Arc::new(output_caches.clone());
```

> **注**：`output_caches` 当前已是 `HashMap<String, Arc<OutputCache>>`，Arc 包外层 HashMap 即可让 run_node 共享。

- [ ] **Step 6: 跑全测试 + spawn 跳过 verify**

```bash
cargo test --workspace
```

Expected: 现有测试全 PASS。pull.rs 自身的 3 个单测也 PASS。注意 `nodes_index.get` 替代 `nodes_by_id.remove` 改动可能引入 borrow / Arc clone 编译错误，按编译器提示修。

- [ ] **Step 7: commit**

```bash
git add src/graph/pull.rs src/graph.rs src/graph/deploy.rs src/graph/runner.rs
git commit -s -m "feat(graph): ADR-0014 Phase 3 EdgesByConsumer + spawn 跳过 pure-form"
```

---

## Task 7: Runner pre-transform pull 收集器（含 pure-form 递归求值）

**Files:**
- Modify: `src/graph/pull.rs` — 加 `pull_data_inputs` async 函数
- Modify: `src/graph/runner.rs` — transform 前调 `pull_data_inputs` 并合并到 payload

- [ ] **Step 1: 在 `src/graph/pull.rs` 加 `pull_data_inputs` 函数**

```rust
use std::sync::Arc;
use serde_json::{Map, Value};
use uuid::Uuid;
use nazh_core::{is_pure_form, EngineError, NodeTrait, OutputCache, PinKind};

/// 在被 Exec 触发的下游节点 transform 之前，收集其 Data 输入引脚的最新值，
/// 并把它们合并进 transform payload。
///
/// 合并规则（Phase 3 约定，混合输入节点见 Phase 3b 决策）：
/// - 若 `exec_payload` 为 `Object`，把每个 Data pin 的值以 `pin.id` 为键插入
/// - 否则（标量、数组）payload 重写为 `{"in": exec_payload, <pin_id>: value, ...}`
///
/// 上游若为 pure-form 节点 → 调 [`evaluate_pure_pull`] 递归求值。
/// 上游若为 Exec 节点 → 读其 [`OutputCache`] 槽。
pub(crate) async fn pull_data_inputs(
    consumer_node_id: &str,
    exec_payload: Value,
    edges_by_consumer: &EdgesByConsumer,
    nodes_index: &HashMap<String, Arc<dyn NodeTrait>>,
    output_caches_index: &HashMap<String, Arc<OutputCache>>,
    trace_id: Uuid,
) -> Result<Value, EngineError> {
    let entries = edges_by_consumer.for_consumer(consumer_node_id);
    if entries.is_empty() {
        return Ok(exec_payload);
    }

    // 收集所有上游 Data 值
    let mut data_values: Map<String, Value> = Map::new();
    for entry in entries {
        let upstream_value = pull_one(
            &entry.upstream_node_id,
            &entry.upstream_output_pin_id,
            nodes_index,
            output_caches_index,
            edges_by_consumer,
            trace_id,
        )
        .await?;
        data_values.insert(entry.consumer_input_pin_id.clone(), upstream_value);
    }

    // 合并到 exec payload
    Ok(merge_payload(exec_payload, data_values))
}

fn merge_payload(exec_payload: Value, data_values: Map<String, Value>) -> Value {
    match exec_payload {
        Value::Object(mut map) => {
            for (k, v) in data_values {
                map.insert(k, v);
            }
            Value::Object(map)
        }
        other => {
            let mut map = data_values;
            map.insert("in".to_owned(), other);
            Value::Object(map)
        }
    }
}

/// 从单个上游 (node_id, pin_id) 拉取一份 Data 值。
fn pull_one<'a>(
    upstream_node_id: &'a str,
    upstream_output_pin_id: &'a str,
    nodes_index: &'a HashMap<String, Arc<dyn NodeTrait>>,
    output_caches_index: &'a HashMap<String, Arc<OutputCache>>,
    edges_by_consumer: &'a EdgesByConsumer,
    trace_id: Uuid,
) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<Value, EngineError>> + Send + 'a>>
{
    Box::pin(async move {
        let upstream = nodes_index.get(upstream_node_id).ok_or_else(|| {
            EngineError::invalid_graph(format!(
                "拉路径上游节点 `{upstream_node_id}` 在 nodes_index 缺失"
            ))
        })?;

        if is_pure_form(upstream.as_ref()) {
            // 递归：先收集 pure 上游自己的 Data 输入，再调用其 transform
            let upstream_payload = pull_data_inputs(
                upstream_node_id,
                Value::Object(Map::new()),
                edges_by_consumer,
                nodes_index,
                output_caches_index,
                trace_id,
            )
            .await?;
            let result = upstream.transform(trace_id, upstream_payload).await?;
            // 找匹配 upstream_output_pin_id 的输出 payload
            // pure 节点 transform payload 约定为 `{ <pin_id>: value, ... }`
            for output in &result.outputs {
                if let Value::Object(map) = &output.payload {
                    if let Some(v) = map.get(upstream_output_pin_id) {
                        return Ok(v.clone());
                    }
                }
            }
            // 兜底：若 pure 节点只有单输出且 payload 不是 `{pin_id: value}` 形态
            // 整体返回（容忍无 wrap 的简单 pure 节点）
            result
                .outputs
                .first()
                .map(|o| o.payload.clone())
                .ok_or(EngineError::DataPinCacheEmpty {
                    upstream: upstream_node_id.to_owned(),
                    pin: upstream_output_pin_id.to_owned(),
                })
        } else {
            // 非 pure：读 OutputCache
            let cache = output_caches_index.get(upstream_node_id).ok_or_else(|| {
                EngineError::invalid_graph(format!(
                    "上游 Exec 节点 `{upstream_node_id}` 在 output_caches_index 缺失"
                ))
            })?;
            cache
                .read(upstream_output_pin_id)
                .map(|c| c.value)
                .ok_or(EngineError::DataPinCacheEmpty {
                    upstream: upstream_node_id.to_owned(),
                    pin: upstream_output_pin_id.to_owned(),
                })
        }
    })
}
```

把现有 `use std::collections::HashMap;` 等 import 补到文件顶端 use 块（与 Step 1 的 build 函数共享）。

- [ ] **Step 2: 在 `crates/core/src/error.rs` 检查 `DataPinUpstreamMissing` / `DataPinCacheEmpty` 是否在 Task 1 已加；如果未加补上**

```bash
grep -n "DataPinCacheEmpty\|DataPinUpstreamMissing" crates/core/src/error.rs
```

- [ ] **Step 3: 在 `src/graph/runner.rs` transform 之前调 `pull_data_inputs`**

定位 `let result = guarded_execute(... node.transform(trace_id, payload) ...)`，把 `payload` 替换为合并后的 payload：

```rust
        let payload = match payload_result {
            Ok(p) => p,
            Err(error) => {
                emit_failure(&event_tx, &node_id, trace_id, &error);
                continue;
            }
        };

        // ADR-0014 Phase 3：transform 之前先把所有 Data 输入引脚的最新值拉到
        // payload 里。pull collector 不动 Exec 路径——若节点没声明 Data 输入则
        // edges_by_consumer.for_consumer 返回空，merge_payload 直接返回原 payload。
        let payload = match super::pull::pull_data_inputs(
            &node_id,
            payload,
            &edges_by_consumer,
            &nodes_index,
            &output_caches_index,
            trace_id,
        )
        .await
        {
            Ok(merged) => merged,
            Err(error) => {
                emit_failure(&event_tx, &node_id, trace_id, &error);
                continue;
            }
        };
```

- [ ] **Step 4: 跑测试**

```bash
cargo test --workspace
```

Expected: 现有 14 类节点全 PASS（它们没 Data 输入，pull collector 短路）；pull.rs 自身的 3 单测仍 PASS。新增 pull 调用对生产路径零开销。

- [ ] **Step 5: commit**

```bash
git add src/graph/pull.rs src/graph/runner.rs
git commit -s -m "feat(graph): ADR-0014 Phase 3 Runner pre-transform pull 收集（含 pure-form 递归求值）"
```

---

## Task 8: 端到端集成测试（pure 链 + Exec 触发拉取）

**Files:**
- Create: `tests/pin_kind_phase3.rs` — 集成测试

- [ ] **Step 1: 创建 `tests/pin_kind_phase3.rs`**

```rust
//! ADR-0014 Phase 3：pure-form 节点 + Runner 拉路径端到端集成测试。

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::{collections::HashMap, sync::Arc, time::Duration};

use nazh_engine::{
    deploy_workflow, standard_registry, ConnectionManager, ExecutionEvent, WorkflowGraph,
};
use nazh_engine::{WorkflowEdge, WorkflowNodeDefinition};
use serde_json::json;
use tokio::time::timeout;

fn def(id: &str, kind: &str, config: serde_json::Value) -> WorkflowNodeDefinition {
    WorkflowNodeDefinition::new(id.to_owned(), kind.to_owned(), config)
}

fn edge(
    from: &str,
    sport: Option<&str>,
    to: &str,
    tport: Option<&str>,
) -> WorkflowEdge {
    WorkflowEdge {
        from: from.to_owned(),
        to: to.to_owned(),
        source_port_id: sport.map(ToOwned::to_owned),
        target_port_id: tport.map(ToOwned::to_owned),
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn pure_chain_被_native_触发拉取() {
    // DAG：
    //   timer(native) -[Exec out → in]-> sink(debugConsole)
    //   c2f(pure) -[Data out → temp_f]-> sink(debugConsole)
    //
    // sink 期望 transform 时收到 payload = { ...exec, temp_f: <华氏值> }
    //
    // c2f 的输入 value 由测试用 SubgraphInput-like 钩子注入——为简化，本测试
    // 用一个 modbusRead-like stub 节点（写 OutputCache.latest）模拟。
    // 这里我们用 `native` (单 transform)写入 Data cache 的能力暂不存在，所以
    // 用更轻的方法：让 c2f 自己有个常量上游——直接在 native 的 payload 里
    // 准备 value 字段，再走 c2f Data 输入。
    //
    // 为此我们改让 c2f 的 value 来自一个专门的"value 提供者"：用 `code` 节点
    // 把常量推给 modbusRead-like stub？太复杂。简化：
    //
    // 测试方案 1：用纯 c2f 单节点 + 直接 transform 调用（绕过 deploy）。
    // 测试方案 2：用 native -> debugConsole + 把 c2f 接为 debugConsole 的
    //            Data 输入，c2f 的 value 来自 native 的 `latest` Data 输出。
    //
    // 选 2 ——但 native 没 Data 输出。最干净的：用 modbusRead 但不实际连 PLC，
    // 而是测试只调 transform。
    //
    // **决策**：本测试用 native + 自定义"value 提供者"模式 — 用 modbusRead-like
    // mock。Phase 3 集成测试目标是验证 deploy + Runner pull 路径，不要求覆盖
    // 所有节点组合。
    //
    // 最小可行：用 nodes-flow `code` 节点跑一段 Rhai 脚本输出 { latest: 25.0 }
    // 给 modbusRead.latest? 不行 code 只有 Exec 输出。
    //
    // 最简方案：把 c2f 链接到一个真实有 Data 输出的节点——只有 modbusRead 满足。
    // 测试期我们 mock 一个 modbusRead 配置不实际打 PLC——但 modbusRead.transform
    // 会尝试连接...
    //
    // 折中方案 [APPROVED]：写一个 `#[cfg(test)]` 内置 stub 节点 plugin 注册到
    // standard_registry（仅本测试 file 用），既写 Data cache 又被 native 触发。
    // 见下方 stub_plugin 模块。

    // TODO: 此测试的最终形态见 Step 2 重写
    let _ = (def, edge); // silence unused warning until Step 2
}
```

- [ ] **Step 2: 重写 Step 1，用本地 stub_plugin 提供"Exec 触发 + Data 输出"上游节点**

把 Step 1 内容整体替换为：

```rust
//! ADR-0014 Phase 3：pure-form 节点 + Runner 拉路径端到端集成测试。

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::{collections::HashMap, sync::Arc, time::Duration};

use async_trait::async_trait;
use nazh_core::{
    EngineError, NodeCapabilities, NodeExecution, NodeRegistry, NodeTrait, PinDefinition,
    PinDirection, PinKind, PinType, Plugin, PluginManifest,
};
use nazh_engine::{
    standard_registry, ConnectionManager, ExecutionEvent, WorkflowEdge, WorkflowGraph,
    WorkflowNodeDefinition,
};
use serde_json::{json, Value};
use tokio::time::timeout;
use uuid::Uuid;

// ---- 本测试用的 stub 节点：Exec 触发，输出包含 `value` Float 写入 Data 缓存 ----

struct CelsiusSourceNode {
    id: String,
    constant_celsius: f64,
}

#[async_trait]
impl NodeTrait for CelsiusSourceNode {
    fn id(&self) -> &str {
        &self.id
    }
    fn kind(&self) -> &str {
        "celsiusSource"
    }
    fn input_pins(&self) -> Vec<PinDefinition> {
        vec![PinDefinition::default_input()]
    }
    fn output_pins(&self) -> Vec<PinDefinition> {
        vec![
            PinDefinition::default_output(),
            PinDefinition::output_named_data(
                "value",
                "value",
                PinType::Float,
                "测试用：写入 Data 缓存的常量摄氏温度",
            ),
        ]
    }
    async fn transform(
        &self,
        _trace_id: Uuid,
        _payload: Value,
    ) -> Result<NodeExecution, EngineError> {
        // 单 NodeOutput 走 Broadcast，Phase 1+2 写 cache 路径会顺手把 payload
        // 复制一份到 `value` Data 槽（dispatch=Broadcast 时所有 Data 输出都写）。
        Ok(NodeExecution::single(json!({ "value": self.constant_celsius })))
    }
}

// ---- 本测试用的 sink 节点：声明 Data 输入 `temp_f` 拉取 c2f 的输出 ----

struct AssertingSinkNode {
    id: String,
    captured: tokio::sync::mpsc::Sender<Value>,
}

#[async_trait]
impl NodeTrait for AssertingSinkNode {
    fn id(&self) -> &str {
        &self.id
    }
    fn kind(&self) -> &str {
        "assertingSink"
    }
    fn input_pins(&self) -> Vec<PinDefinition> {
        vec![
            PinDefinition::default_input(),
            PinDefinition {
                id: "temp_f".to_owned(),
                label: "temp_f".to_owned(),
                pin_type: PinType::Float,
                direction: PinDirection::Input,
                required: false,
                kind: PinKind::Data,
                description: None,
            },
        ]
    }
    fn output_pins(&self) -> Vec<PinDefinition> {
        vec![PinDefinition::default_output()]
    }
    async fn transform(
        &self,
        _trace_id: Uuid,
        payload: Value,
    ) -> Result<NodeExecution, EngineError> {
        // 把合并后的 payload 上报给测试主线程
        self.captured.send(payload.clone()).await.ok();
        Ok(NodeExecution::single(payload))
    }
}

struct StubPlugin;

impl Plugin for StubPlugin {
    fn manifest(&self) -> PluginManifest {
        PluginManifest {
            name: "stub-phase3",
            version: "0.0.0",
        }
    }
    fn register(&self, registry: &mut NodeRegistry) {
        // 真实接入需要 captured channel——本 plugin 注册闭包不能持有
        // 测试本地 channel；测试改为：构造图前直接调 standard_registry，
        // 然后 manually 用 registry.register_with_capabilities 注入两节点。
        let _ = registry;
    }
}

// ---- 测试主体 ----

#[tokio::test(flavor = "multi_thread")]
async fn pure_chain_被_celsius_source_触发拉取() {
    let (sink_tx, mut sink_rx) = tokio::sync::mpsc::channel::<Value>(4);

    let mut registry = standard_registry();
    // 注入 stub 节点（拿到 sink_tx 后才能构造 AssertingSinkNode）
    {
        let sink_tx = sink_tx.clone();
        registry.register_with_capabilities(
            "assertingSink",
            NodeCapabilities::empty(),
            move |def, _res| {
                Ok(Arc::new(AssertingSinkNode {
                    id: def.id().to_owned(),
                    captured: sink_tx.clone(),
                }))
            },
        );
    }
    registry.register_with_capabilities(
        "celsiusSource",
        NodeCapabilities::empty(),
        |def, _res| {
            let celsius = def
                .config()
                .get("celsius")
                .and_then(|v| v.as_f64())
                .unwrap_or(25.0);
            Ok(Arc::new(CelsiusSourceNode {
                id: def.id().to_owned(),
                constant_celsius: celsius,
            }))
        },
    );

    // 图：source(Exec out) → sink(in)
    //     source(Data value) → c2f(Data value)
    //     c2f(Data out) → sink(Data temp_f)
    let nodes = HashMap::from([
        (
            "source".to_owned(),
            WorkflowNodeDefinition::new(
                "source".to_owned(),
                "celsiusSource".to_owned(),
                json!({ "celsius": 25.0 }),
            ),
        ),
        (
            "c2f".to_owned(),
            WorkflowNodeDefinition::new("c2f".to_owned(), "c2f".to_owned(), json!({})),
        ),
        (
            "sink".to_owned(),
            WorkflowNodeDefinition::new(
                "sink".to_owned(),
                "assertingSink".to_owned(),
                json!({}),
            ),
        ),
    ]);
    let edges = vec![
        WorkflowEdge {
            from: "source".to_owned(),
            to: "sink".to_owned(),
            source_port_id: Some("out".to_owned()),
            target_port_id: Some("in".to_owned()),
        },
        WorkflowEdge {
            from: "source".to_owned(),
            to: "c2f".to_owned(),
            source_port_id: Some("value".to_owned()),
            target_port_id: Some("value".to_owned()),
        },
        WorkflowEdge {
            from: "c2f".to_owned(),
            to: "sink".to_owned(),
            source_port_id: Some("out".to_owned()),
            target_port_id: Some("temp_f".to_owned()),
        },
    ];
    let graph = WorkflowGraph {
        name: Some("pure_chain_test".to_owned()),
        nodes,
        edges,
        connections: vec![],
        variables: None,
    };

    let conn_manager = Arc::new(tokio::sync::RwLock::new(ConnectionManager::new()));
    let mut deployment = nazh_engine::deploy_workflow(graph, conn_manager, &registry)
        .await
        .expect("部署成功");

    // 触发 source（root 节点）
    let root_sender = deployment
        .ingress
        .root_senders
        .get("source")
        .expect("source 是根节点")
        .clone();
    let trace_id = Uuid::new_v4();
    let data_id = deployment
        .ingress
        .store
        .write(json!({}), 1)
        .expect("write");
    root_sender
        .send(nazh_core::ContextRef::new(trace_id, data_id, None))
        .await
        .expect("send");

    // 等 sink 收到 transform 的合并 payload
    let captured = timeout(Duration::from_secs(5), sink_rx.recv())
        .await
        .expect("超时未收到 sink 调用")
        .expect("sink_rx 被关闭");

    // 断言：合并 payload 既包含 Exec 推过来的 source 输出，又包含 c2f 拉算出的 temp_f
    let temp_f = captured
        .get("temp_f")
        .and_then(|v| v.as_f64())
        .expect("temp_f 应在 payload 中");
    assert!((temp_f - 77.0).abs() < 1e-9, "25°C → 77°F, got {temp_f}");

    // 关停 deployment 让 RAII guards drop
    deployment.shutdown().await;
}
```

> **重要假设**：本测试假定 `WorkflowGraph` / `WorkflowNodeDefinition::new` / `ConnectionManager::new` / `WorkflowDeployment::shutdown` 等 API 形态与现状一致。若 grep 后发现 API 名字略有出入，按实际改。

- [ ] **Step 3: 跑集成测试**

```bash
cargo test --test pin_kind_phase3 -- --nocapture
```

Expected: PASS。如果失败：
- 编译错误 → 按编译器提示对齐 API
- 运行 deadlock → 检查 sink_rx 是否被持有（drop 掉发送端） / Phase 1 写 cache 路径是否触发（Broadcast 应触发）
- temp_f 值错 → 检查 c2f 算法 / pull merge 逻辑

- [ ] **Step 4: commit**

```bash
git add tests/pin_kind_phase3.rs
git commit -s -m "test(adr-0014): Phase 3 端到端集成测试（pure chain + Runner pull）"
```

---

## Task 9: 跨语言 fixture + Rust 契约测试

**Files:**
- Create: `tests/fixtures/pure_form_matrix.jsonc`
- Create: `crates/core/tests/pure_form_contract.rs`

- [ ] **Step 1: 创建 fixture `tests/fixtures/pure_form_matrix.jsonc`**

```jsonc
// ADR-0014 Phase 3：pure-form 判定真值表，Rust + Vitest 共享。
//
// 4 配对穷尽：
//   - allDataAllSides    → pure-form ✓ (典型 c2f / minutesSince / lookup)
//   - mixedExecInput     → ✗ (有 Exec input，参与触发链)
//   - mixedExecOutput    → ✗ (有 Exec output，参与触发链)
//   - emptyInputAllData  → pure-form ✓ (无输入只有 Data 输出，如设备表常量)
//
// 任一方与本 fixture 漂移 → CI 红。
[
  {
    "name": "allDataAllSides",
    "input_pins": [{"kind": "data"}],
    "output_pins": [{"kind": "data"}],
    "expected_pure_form": true
  },
  {
    "name": "mixedExecInput",
    "input_pins": [{"kind": "exec"}, {"kind": "data"}],
    "output_pins": [{"kind": "data"}],
    "expected_pure_form": false
  },
  {
    "name": "mixedExecOutput",
    "input_pins": [{"kind": "data"}],
    "output_pins": [{"kind": "exec"}, {"kind": "data"}],
    "expected_pure_form": false
  },
  {
    "name": "emptyInputAllData",
    "input_pins": [],
    "output_pins": [{"kind": "data"}],
    "expected_pure_form": true
  }
]
```

- [ ] **Step 2: 创建 `crates/core/tests/pure_form_contract.rs`**

```rust
//! ADR-0014 Phase 3：pure_form 跨语言契约测试。
//!
//! 共享 fixture：`/tests/fixtures/pure_form_matrix.jsonc`（仓库根，与 Vitest 同源）。

#![allow(clippy::unwrap_used)]

use async_trait::async_trait;
use nazh_core::{
    is_pure_form, EngineError, NodeExecution, NodeTrait, PinDefinition, PinDirection, PinKind,
    PinType,
};
use serde::Deserialize;
use serde_json::Value;
use uuid::Uuid;

#[derive(Debug, Deserialize)]
struct PinSpec {
    kind: String,
}

#[derive(Debug, Deserialize)]
struct Case {
    name: String,
    input_pins: Vec<PinSpec>,
    output_pins: Vec<PinSpec>,
    expected_pure_form: bool,
}

struct Stub {
    id: String,
    inputs: Vec<PinDefinition>,
    outputs: Vec<PinDefinition>,
}

#[async_trait]
impl NodeTrait for Stub {
    fn id(&self) -> &str {
        &self.id
    }
    fn kind(&self) -> &str {
        "stub"
    }
    fn input_pins(&self) -> Vec<PinDefinition> {
        self.inputs.clone()
    }
    fn output_pins(&self) -> Vec<PinDefinition> {
        self.outputs.clone()
    }
    async fn transform(&self, _: Uuid, payload: Value) -> Result<NodeExecution, EngineError> {
        Ok(NodeExecution::single(payload))
    }
}

fn pin(kind_str: &str, dir: PinDirection) -> PinDefinition {
    let kind = match kind_str {
        "exec" => PinKind::Exec,
        "data" => PinKind::Data,
        other => panic!("未知 pin kind: {other}"),
    };
    PinDefinition {
        id: format!("p_{kind_str}"),
        label: format!("p_{kind_str}"),
        pin_type: PinType::Any,
        direction: dir,
        required: matches!(dir, PinDirection::Input) && matches!(kind, PinKind::Exec),
        kind,
        description: None,
    }
}

#[test]
fn fixture_穷尽_4_配对() {
    let raw = std::fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../tests/fixtures/pure_form_matrix.jsonc"),
    )
    .expect("读取 fixture");
    // 简单去注释（jsonc → json）：仅去除单行 `//` 注释
    let stripped: String = raw
        .lines()
        .map(|l| {
            if let Some(idx) = l.find("//") {
                &l[..idx]
            } else {
                l
            }
        })
        .collect::<Vec<_>>()
        .join("\n");
    let cases: Vec<Case> = serde_json::from_str(&stripped).expect("解析 fixture");
    assert_eq!(cases.len(), 4, "fixture 必须穷尽 4 配对");

    for case in cases {
        let stub = Stub {
            id: case.name.clone(),
            inputs: case
                .input_pins
                .iter()
                .map(|p| pin(&p.kind, PinDirection::Input))
                .collect(),
            outputs: case
                .output_pins
                .iter()
                .map(|p| pin(&p.kind, PinDirection::Output))
                .collect(),
        };
        assert_eq!(
            is_pure_form(&stub),
            case.expected_pure_form,
            "case `{}` 判定与 fixture 不符",
            case.name
        );
    }
}
```

- [ ] **Step 3: 跑测试**

```bash
cargo test -p nazh-core --test pure_form_contract
```

Expected: PASS（4 case 全对齐）。

- [ ] **Step 4: commit**

```bash
git add tests/fixtures/pure_form_matrix.jsonc crates/core/tests/pure_form_contract.rs
git commit -s -m "test(adr-0014): Phase 3 pure-form 跨语言契约 fixture + Rust 端"
```

---

## Task 10: 前端 `isPureForm` helper + Vitest fixture 共享

**Files:**
- Modify: `web/src/lib/pin-compat.ts` — 加 `isPureForm`
- Modify: `web/src/lib/__tests__/pin-compat.test.ts` — 共享 fixture 测试块

- [ ] **Step 1: 在 `web/src/lib/pin-compat.ts` 末尾加 `isPureForm`**

```typescript
import type { PinDefinition } from '../generated/PinDefinition';

/**
 * ADR-0014 Phase 3：判定节点是否为 pure-form（无 Exec 引脚）。
 *
 * 与 Rust `nazh_core::is_pure_form` 同语义——任一端有 `kind: 'exec'` 引脚即非
 * pure-form。空输入 / 空输出 + 全 Data 仍算 pure-form（典型如"设备表"常量节点）。
 *
 * 跨语言契约 fixture：`tests/fixtures/pure_form_matrix.jsonc`（仓库根）。
 */
export function isPureForm(
  inputPins: ReadonlyArray<Pick<PinDefinition, 'kind'>>,
  outputPins: ReadonlyArray<Pick<PinDefinition, 'kind'>>,
): boolean {
  const noExecIn = inputPins.every((p) => (p.kind ?? 'exec') !== 'exec');
  const noExecOut = outputPins.every((p) => (p.kind ?? 'exec') !== 'exec');
  return noExecIn && noExecOut;
}
```

- [ ] **Step 2: 在 `web/src/lib/__tests__/pin-compat.test.ts` 加共享 fixture 测试**

```typescript
import { describe, expect, it } from 'vitest';
import { readFileSync } from 'node:fs';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { isPureForm } from '../pin-compat';

const __dirname = dirname(fileURLToPath(import.meta.url));

describe('isPureForm — fixture parity with Rust `nazh_core::is_pure_form`', () => {
  const raw = readFileSync(
    resolve(__dirname, '../../../../tests/fixtures/pure_form_matrix.jsonc'),
    'utf8',
  );
  // 去掉 `//` 行注释，模仿 jsonc 解析
  const stripped = raw
    .split('\n')
    .map((line) => {
      const idx = line.indexOf('//');
      return idx >= 0 ? line.slice(0, idx) : line;
    })
    .join('\n');
  const cases = JSON.parse(stripped) as Array<{
    name: string;
    input_pins: Array<{ kind: 'exec' | 'data' }>;
    output_pins: Array<{ kind: 'exec' | 'data' }>;
    expected_pure_form: boolean;
  }>;

  it.each(cases)('$name → $expected_pure_form', (c) => {
    expect(isPureForm(c.input_pins, c.output_pins)).toBe(c.expected_pure_form);
  });
});
```

- [ ] **Step 3: 跑 Vitest**

```bash
npm --prefix web run test -- pin-compat
```

Expected: 4 case 全 PASS。

- [ ] **Step 4: commit**

```bash
git add web/src/lib/pin-compat.ts web/src/lib/__tests__/pin-compat.test.ts
git commit -s -m "feat(web): isPureForm helper + 跨语言 fixture 共享（ADR-0014 Phase 3）"
```

---

## Task 11: 前端节点定义 c2f + minutesSince

**Files:**
- Create: `web/src/components/flowgram/nodes/c2f.ts`
- Create: `web/src/components/flowgram/nodes/minutes-since.ts`
- Modify: `web/src/components/flowgram/nodes/catalog.ts` — 新增"纯计算"分类
- Modify: `web/src/components/flowgram/flowgram-node-library.ts` — import + ALL_DEFS

- [ ] **Step 1: 创建 `web/src/components/flowgram/nodes/c2f.ts`**

参考已有节点定义（用 `cat web/src/components/flowgram/nodes/native.ts` 看样板）。结构大致：

```typescript
import type { NodeDefinition } from './shared';
import { defineNode } from './shared';

export const c2fDef: NodeDefinition = defineNode({
  kind: 'c2f',
  label: '摄氏 → 华氏',
  category: 'pure-compute',
  glyph: 'thermometer',
  description: '把摄氏温度转换为华氏。pure-form：仅 Data 引脚。',
  inputPins: [
    {
      id: 'value',
      label: '摄氏度',
      pinType: { kind: 'float' },
      kind: 'data',
      required: true,
    },
  ],
  outputPins: [
    {
      id: 'out',
      label: '华氏度',
      pinType: { kind: 'float' },
      kind: 'data',
      required: false,
    },
  ],
  defaultConfig: {},
  // pure-form 节点不参与触发链——SettingsPanel 中可隐藏 Exec/buffer/timeout 等
});
```

> **注**：`defineNode` 与 `NodeDefinition` 的实际 API 形态请按 `web/src/components/flowgram/nodes/shared.ts` 中的现有签名校准。如果没有 `defineNode` helper，参考 `native.ts` / `loop.ts` 的对象字面量构造方式。`category: 'pure-compute'` 是 Step 3 加的常量。

- [ ] **Step 2: 创建 `web/src/components/flowgram/nodes/minutes-since.ts`**

```typescript
import type { NodeDefinition } from './shared';
import { defineNode } from './shared';

export const minutesSinceDef: NodeDefinition = defineNode({
  kind: 'minutesSince',
  label: '距今分钟数',
  category: 'pure-compute',
  glyph: 'clock',
  description:
    '给定 RFC3339 时间戳字符串（如 `2026-04-28T08:00:00Z`），返回当前距其分钟数。pure-form：仅 Data 引脚。',
  inputPins: [
    {
      id: 'since',
      label: '起点时间',
      pinType: { kind: 'string' },
      kind: 'data',
      required: true,
    },
  ],
  outputPins: [
    {
      id: 'out',
      label: '分钟数',
      pinType: { kind: 'integer' },
      kind: 'data',
      required: false,
    },
  ],
  defaultConfig: {},
});
```

- [ ] **Step 3: 在 `web/src/components/flowgram/nodes/catalog.ts` 加新分类常量**

定位现有分类（如 `'流程控制'` / `'子图封装'`），同模式加：

```typescript
export const NODE_CATEGORY_PURE_COMPUTE = '纯计算' as const;
```

并在 `NODE_CATEGORIES` 数组（如有）追加。

- [ ] **Step 4: 在 `web/src/components/flowgram/flowgram-node-library.ts` import + ALL_DEFS 加两个 def**

```typescript
import { c2fDef } from './nodes/c2f';
import { minutesSinceDef } from './nodes/minutes-since';

// ALL_DEFS 数组里追加
const ALL_DEFS: NodeDefinition[] = [
  // ... 现有 ...
  c2fDef,
  minutesSinceDef,
];
```

调色板分组逻辑（`buildPaletteJson` 等）若按 `category` 自动分组则无需额外改。如果是手写映射，把 `'pure-compute' → '纯计算'` 加上。

- [ ] **Step 5: ts-rs 检查（不一定需要，但顺手跑）**

```bash
cargo test -p tauri-bindings --features ts-export export_bindings
```

Expected: 无新生成的 ts 文件 diff（本 Phase 没新加 `#[ts(export)]` 类型）。

- [ ] **Step 6: 前端 type-check**

```bash
npm --prefix web run build
```

或用 `tsc --noEmit`。Expected: 无 type 错误。

- [ ] **Step 7: commit**

```bash
git add web/src/components/flowgram/
git commit -s -m "feat(web): c2f + minutesSince NodeDefinition + 纯计算分类（ADR-0014 Phase 3）"
```

---

## Task 12: 前端 pure-form 视觉（绿色头 + 圆角）

**Files:**
- Modify: `web/src/components/FlowgramCanvas.tsx` — `FlowgramNodeCard` 渲染时根据 schema 加 `data-pure-form="true"`
- Modify: `web/src/styles/flowgram.css` — `[data-pure-form="true"]` 样式

- [ ] **Step 1: 在 `FlowgramNodeCard` 组件取得 input/output pins schema 后调 `isPureForm`**

定位 `FlowgramNodeCard`（grep `FlowgramNodeCard` in `FlowgramCanvas.tsx`）。该组件已有逻辑读取 schema 显示端口；在 root `<div>` 上加：

```tsx
import { isPureForm } from '../lib/pin-compat';

// ... 在 FlowgramNodeCard 内部 ...
const inputPins = nodeSchema?.input_pins ?? [];
const outputPins = nodeSchema?.output_pins ?? [];
const pureForm = isPureForm(inputPins, outputPins);

return (
  <div
    className="flowgram-node-card"
    data-node-kind={kind}
    data-pure-form={pureForm ? 'true' : undefined}
    // ... 其他 attribute ...
  >
    {/* ... */}
  </div>
);
```

> **注**：已有 `data-port-pin-kind` attribute 的设置模式可参照（在端口 DOM 上）。这里在节点 root DOM 上。

- [ ] **Step 2: 在 `web/src/styles/flowgram.css` 末尾加样式**

```css
/* ADR-0014 Phase 3：pure-form 节点视觉差异化（UE5 Blueprint Pure Node 风格）。
 * 头部绿色 + 圆角胶囊形——一眼区分"表达式节点"与"触发链节点"。
 * 后续 Phase 5 还会按 capability 给 Trigger / Branching 着色，本 Phase 只覆盖 Pure。 */
.flowgram-node-card[data-pure-form='true'] {
  border-radius: 18px; /* 比普通节点的 8px 明显偏圆 */
  border-color: var(--pin-float, #2fb75f);
}

.flowgram-node-card[data-pure-form='true'] .flowgram-node-header {
  background: linear-gradient(180deg, #2fb75f 0%, #258a47 100%);
  color: #ffffff;
}

.flowgram-node-card[data-pure-form='true'] .flowgram-node-header-icon {
  /* 让 glyph 在绿底上更可读 */
  filter: brightness(0) invert(1);
}
```

> **注**：`.flowgram-node-header` / `.flowgram-node-header-icon` 类名需对照实际 DOM 结构（用浏览器 devtools 或 grep `className=` in `FlowgramCanvas.tsx`）。如类名不同，按实际改。

- [ ] **Step 3: 启动 dev server 手动验证**

```bash
cd src-tauri && ../web/node_modules/.bin/tauri dev --no-watch
```

在调色板拖入"摄氏 → 华氏"节点 — 应看到绿色头部 + 圆角胶囊形。再拖入一个 `native` 节点对比 — 应看到普通灰头 + 方形角。

- [ ] **Step 4: commit**

```bash
git add web/src/components/FlowgramCanvas.tsx web/src/styles/flowgram.css
git commit -s -m "feat(web): pure-form 节点绿色头 + 圆角胶囊视觉（ADR-0014 Phase 3）"
```

---

## Task 13: E2E DOM 烟雾测试

**Files:**
- Create: `web/e2e/pin-kind-pure-nodes.spec.ts`

- [ ] **Step 1: 创建 `web/e2e/pin-kind-pure-nodes.spec.ts`**

参考 `web/e2e/pin-kind-modbus.spec.ts`（Phase 2 的烟雾测试模板）。结构：

```typescript
import { expect, test } from '@playwright/test';

test.describe('ADR-0014 Phase 3 — pure-form 节点视觉烟雾', () => {
  test('拖入 c2f 后节点 DOM 携带 data-pure-form=true', async ({ page }) => {
    await page.goto('/');
    // 等画布就绪
    await expect(page.locator('.flowgram-canvas')).toBeVisible();

    // 在调色板搜索 c2f / 摄氏
    const paletteSearch = page.locator('[data-testid="palette-search"]');
    if (await paletteSearch.isVisible()) {
      await paletteSearch.fill('摄氏');
    }
    // 拖入第一个匹配项（具体定位器按实际调色板 DOM 结构调整）
    const paletteItem = page.locator('[data-node-kind="c2f"]').first();
    await paletteItem.dragTo(page.locator('.flowgram-canvas'));

    // 断言节点 DOM 携带 data-pure-form
    const node = page.locator('.flowgram-node-card[data-node-kind="c2f"]');
    await expect(node).toBeVisible();
    await expect(node).toHaveAttribute('data-pure-form', 'true');
  });
});
```

> **注**：选择器 `[data-testid="palette-search"]` / `[data-node-kind="c2f"]` 需按现有 E2E 测试中实际使用的 selector 模式对齐。如果 Phase 2 的 `pin-kind-modbus.spec.ts` 用了不同 selector pattern（如 `text=`），按 Phase 2 模式照抄。

- [ ] **Step 2: 跑 E2E**

```bash
cd src-tauri && cargo build --release  # 必要时
npm --prefix web run test:e2e -- pin-kind-pure-nodes
```

Expected: PASS。

- [ ] **Step 3: commit**

```bash
git add web/e2e/pin-kind-pure-nodes.spec.ts
git commit -s -m "test(e2e): ADR-0014 Phase 3 c2f pure-form DOM 烟雾"
```

---

## Task 14: 文档更新（ADR / AGENTS / memory）

**Files:**
- Modify: `docs/adr/0014-执行边与数据边分离.md` — 实施进度追加 Phase 3 段
- Modify: `AGENTS.md` — Phase 3 状态更新 + ADR Execution Order 表
- Modify: `~/.claude/projects/-home-zhihongniu-Nazh/memory/MEMORY.md` + `project_system_architecture.md` + `project_architecture_review_2026_04.md`

- [ ] **Step 1: 在 `docs/adr/0014-执行边与数据边分离.md` "实施进度" 章节追加 Phase 3 条目**

定位 `- 🟡 **Phase 3-5（规划中）**`，把 Phase 3 单独列：

```markdown
- ✅ **Phase 3（2026-04-28）**：UE5 风格 Pure 节点首发——`c2f` / `minutesSince`
  仅声明 Data 引脚（pure-form 由 `nazh_core::is_pure_form` 推导），部署期跳过
  Tokio task spawn；Runner 在 transform 之前调 `src/graph/pull.rs::pull_data_inputs`
  收集 Data 输入：上游若 pure-form 则递归求值（`Box::pin` 处理 async recursion），
  上游若 Exec 节点则读 OutputCache 槽。新 crate `crates/nodes-pure/` 容纳零协议
  依赖的纯计算节点。前端 `isPureForm` helper + 节点 DOM `data-pure-form="true"`
  attribute + CSS 绿头/圆角胶囊视觉差异化（参考 UE5 Blueprint Pure Node）。
  跨语言 fixture `tests/fixtures/pure_form_matrix.jsonc` 4 配对穷尽 Rust + Vitest
  共享。集成测试 `tests/pin_kind_phase3.rs` 用本地 stub 节点验证 source → c2f →
  sink 拉链：sink 收到的 payload 同时含 source Exec push 与 c2f Data pull。
  详见 `docs/plans/2026-04-28-adr-0014-phase-3-pure-nodes.md`。
- 🟡 **Phase 4-5（规划中）**：缓存空槽兜底策略 / TTL / 视觉打磨（节点头部色按
  capability 自动着色）/ AI prompt 携带 pure-form 信息。
```

- [ ] **Step 2: 修改 `AGENTS.md` 顶部 ADR-0014 段**

定位 `- ADR-0014（执行边与数据边分离 → 重命名为「引脚求值语义二分」）— **已实施 Phase 1 + Phase 2**`，更新为：

```markdown
- ADR-0014（执行边与数据边分离 → 重命名为「引脚求值语义二分」）— **已实施 Phase 1 + Phase 2 + Phase 3**（2026-04-28）。Phase 3：UE5 风格 Pure 节点首发——`c2f` + `minutesSince` 在新 crate `crates/nodes-pure/` 落地，`is_pure_form` 自动推导，部署期跳过 Tokio task spawn，Runner 首次激活 pull 路径（`src/graph/pull.rs` 含递归求值 pure-form 上游 + 读 OutputCache 非 pure-form 上游），前端 `isPureForm` + 绿头圆角视觉。跨语言 fixture `pure_form_matrix.jsonc` 4 配对穷尽。集成测试覆盖 source→c2f→sink 三段拉链。Phase 3 plan `docs/plans/2026-04-28-adr-0014-phase-3-pure-nodes.md`
```

ADR Execution Order 表里把 ADR-0014 行更新：

```markdown
> 8. ✅ **ADR-0014** Pin 求值语义二分 — **Phase 1 + Phase 2 + Phase 3 已实施**（2026-04-28）；Phase 4-5 各自独立 plan
```

`Project Status` "Phases 1-5 complete..." 段下方"Current batch of ADRs" 同步更新。

- [ ] **Step 3: 修改 memory 文件**

更新 `MEMORY.md` 第一行 System Architecture 摘要，把 ADR-0014(P1+P2) 改为 ADR-0014(P1+P2+P3)：

```bash
sed -i 's/ADR-0014(P1+P2)/ADR-0014(P1+P2+P3)/g' /home/zhihongniu/.claude/projects/-home-zhihongniu-Nazh/memory/MEMORY.md
```

更新 `project_system_architecture.md` 与 `project_architecture_review_2026_04.md` 的相应段（手动用 Edit 工具，按"ADR-0014 Phase 3 已实施"措辞补充）。

- [ ] **Step 4: 全量验证**

```bash
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all -- --check
npm --prefix web run test
```

Expected: 全 PASS。任何 clippy warning 都视为 error 并修复（不要 `#[allow]` 一刀切）。

- [ ] **Step 5: commit**

```bash
git add docs/adr/0014-执行边与数据边分离.md AGENTS.md
git commit -s -m "docs(adr-0014): Phase 3 落地后 ADR / AGENTS / memory 更新"
```

memory 文件不进 git（在 `~/.claude/...`，不是项目内）。

---

## Self-Review

### Spec coverage

- ✅ 引入第一批纯计算节点（`c2f` / `minutesSince`）—— Task 3 + 4。`lookup` 显式 out-of-scope（理由：配置驱动 + 携带 lookup table，结构不同质，独立 plan）
- ✅ 部署期识别"无 Exec 引脚节点" = pure 节点形态 —— Task 1 `is_pure_form` + Task 6 spawn 跳过
- ✅ Runner 路径：pure 节点不在 Tokio task spawn 列表 —— Task 6
- ✅ Runner 路径：被下游 transform 时点拉求值 —— Task 7 `pull_data_inputs` 含递归
- ✅ 前端节点头部色：pure 节点绿色头 + 圆角形 —— Task 12
- ✅ 用例 3（表达式树）真实可运行 —— Task 8 集成测试 + Task 12+13 视觉/E2E

### Placeholder scan

- 已检：所有 "TBD / TODO / 待实现 / similar to" 的 task 描述都写出实际代码或具体引用现有文件路径
- Task 8 Step 1 的 sketch-then-rewrite 模式（Step 2 替换 Step 1）是有意保留的设计探索 trace——实施时直接照 Step 2 落地，不会留下半成品

### Type consistency

- `is_pure_form(node: &dyn NodeTrait) -> bool` —— Task 1 / Task 6 / Task 7 / 测试一致
- `EngineError::DataPinUpstreamMissing { consumer, pin }` / `DataPinCacheEmpty { upstream, pin }` —— Task 1 + Task 7 一致
- `EdgesByConsumer` / `DataInEdge { consumer_input_pin_id, upstream_node_id, upstream_output_pin_id }` —— Task 6 + Task 7 一致
- `pull_data_inputs(consumer_node_id, exec_payload, edges_by_consumer, nodes_index, output_caches_index, trace_id)` —— Task 7 + Runner 调用点一致
- `isPureForm(inputPins, outputPins)` (TS) —— Task 10 + Task 12 一致

### 已知风险

- **Task 6 nodes_by_id 改 nodes_index Arc 索引** 是 deploy.rs 局部重构，可能在 spawn 阶段因 borrow / move 触发编译错误——按编译器提示对齐 Arc::clone 用法即可，不会动整体逻辑
- **Task 7 async recursion** Box::pin 写法需小心 lifetime——若编译失败可考虑改用 `async-recursion` crate（workspace 加 dep）
- **Task 8 集成测试的 stub 节点假设**——CelsiusSourceNode 用 `output_named_data` 写 Data 输出 `value` 槽。Phase 1+2 的 cache 写路径在 `Broadcast` dispatch 下会自动写所有 Data 输出。验证条件就在这里。
- **Task 11+12 前端 API 形态假设**（`defineNode` / `NodeDefinition` / `.flowgram-node-header`）需要在实施时按实际代码 grep 校准——这些是脚手架级路径，几乎不会改语义

---

## Implementation note

每条 task 完成后**不要批量提交**——单 task 单 commit，commit 信息中文，遵循根 `AGENTS.md` 的 sign-off 与 hook 规则（无 `--no-verify` / 无 `--amend`）。Phase 3 预期 14 commits。

如遇任一 task 卡顿（编译错误 / 测试失败超过 15min 没有进展），暂停并写"卡点 memo"到对话中，让用户介入。**不要绕过任何 invariant**（unwrap / unsafe / 跳测试 / `--no-verify`）。
