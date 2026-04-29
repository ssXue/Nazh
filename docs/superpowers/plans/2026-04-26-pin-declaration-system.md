> **Status:** merged in 2b383ae

# ADR-0010 Pin 声明系统实施计划（Phase 1）

> **Status:** ✅ Phase 1 已合入 main（2026-04-26）
>
> 提交链：
> - `f72138f` Task 0：Ring 0 Pin 类型骨架
> - `1a9f44c` Task 1：NodeTrait input_pins/output_pins 默认实现
> - `281ec7b` Tasks 2-5：部署期校验器 + 4 个分支节点迁移（原子提交，单独发任一会让 4 个 E2E 红灯）
> - `4d2bf22` Task 6：ts-rs 导出
> - **Task 8（本提交）**：ADR-0010 → 已实施、ADR README 索引、根 AGENTS.md ADR Execution Order + Project Status、crates/core/AGENTS.md + crates/nodes-flow/AGENTS.md 同步
>
> 后续 Phase 2 / Phase 3（前端端口可视化、协议节点 pin 收紧）另立 plan，本 plan 仅交付 Phase 1。

**Goal:** 在 `NodeTrait` 上引入 `input_pins()` / `output_pins()` 实例方法 + `PinDefinition` / `PinType` / `PinDirection` 三类 Ring 0 类型，部署期对每条 `WorkflowEdge` 做类型兼容校验，把"哪些端口存在 / 端口是什么类型"从边上的字符串约定升级为节点的一等声明契约。Phase 1 默认 `Any`，14 个现存节点零改动；先在 `if` / `switch` / `loop` / `tryCatch` 四个分支节点上落地具体 pin。

**Architecture:**
- **Ring 0 (`crates/core`)** 新增 `pin.rs` 模块：`PinDefinition` / `PinType` / `PinDirection` 三类型 + `is_compatible_with` 兼容矩阵。`NodeTrait` 加 `input_pins(&self) -> Vec<PinDefinition>` / `output_pins(&self) -> Vec<PinDefinition>`，默认实现返回单 `Any` 引脚保兼容。
- **Runner (`src/graph/deploy.rs`)** 在阶段 1（`on_deploy`）之前**新增阶段 0.5**：实例化节点后立刻迭代 `WorkflowEdge`，按 pin id 解析两端 pin schema，调用 `is_compatible_with`，不通过即返回 `EngineError::IncompatiblePinTypes` 整图回滚。
- **IPC (`crates/tauri-bindings`)** 不在 `NodeTypeEntry` 上加 pin（type-level pin 留给 phase 2）。Phase 1 仅打通 `PinType` / `PinDefinition` 的 ts-rs 导出，让未来 phase 2 直接复用类型。
- **Phase 1 节点迁移范围**：`if` / `switch` / `loop` / `tryCatch`。这四个节点已在 runtime 用 `NodeDispatch::Route(["body"])` 等具名端口；Phase 1 把它们的 pin 声明从隐式提升为显式，校验器才有东西可查。

**Tech Stack:** Rust 2024、`serde` / `ts-rs`（`ts-export` feature 门控）、`thiserror`（新错误变体）、`async-trait`（trait 兼容）。**不引入** `schemars` / JSON Schema crate —— 详见决策点 1。

## 关键决策点（执行前请确认）

1. **`PinType::Json` 不带 schema payload**。ADR 草稿写的是 `Json(Option<JsonSchema>)`；Phase 1 落地为 `Json`（无 payload）。原因：JSON Schema crate（`schemars` ~1MB / `jsonschema` 拉 reqwest）会把 Ring 0 footprint 推上一个台阶，且 phase 1 的兼容矩阵用不上结构校验。结构校验留待未来独立 ADR + 单独 plan。
2. **`Custom(&'static str)` 改为 `Custom(String)`**。原 ADR 字面给的是 `&'static str`，但需要 serde round-trip + ts-rs 导出 + 配置文件读入，`&'static str` 走不通；改为 `String` 语义不变。
3. **`output_pins(&self)` 是实例方法不是 `'static` 表**。`switch` 节点的分支 key 来自 config，必须读 `&self.branches` 才能给出准确 pin 列表。trait 默认实现仍提供"单 `Any`"兜底，不影响普通节点。
4. **Phase 1 不动前端端口渲染**。FlowGram 画布的多端口可视化是 phase 2 的范围。Phase 1 只把 pin schema 经 IPC 透出（一个新命令），前端 type-only 拿到不消费——避免单 PR 跨三层失控。
5. **校验执行点放在 `deploy.rs` 阶段 0.5**（节点实例化后、`on_deploy` 前），不放 `topology.rs`。原因：`topology()` 只持有 `WorkflowGraph`（无 registry / 无实例），动它意味着把 registry 引用注入纯静态拓扑分析模块，破坏 `topology.rs` 的职责边界。

---

## Task 0：Ring 0 引入 Pin 类型骨架

**Files:**
- Create: `crates/core/src/pin.rs`
- Modify: `crates/core/src/lib.rs`（`pub use pin::*`）
- Modify: `crates/core/src/error.rs`（新增 `IncompatiblePinTypes` / `UnknownPin` / `DuplicatePinId` 三个错误变体）
- Modify: `crates/core/AGENTS.md`（同步 pin 契约说明）

- [x] **Step 1：定义 `PinDirection` / `PinType` / `PinDefinition`**

```rust
// crates/core/src/pin.rs
use serde::{Deserialize, Serialize};
#[cfg(feature = "ts-export")]
use ts_rs::TS;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-export", derive(TS), ts(export))]
pub enum PinDirection { Input, Output }

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-export", derive(TS), ts(export))]
pub enum PinType {
    Any,
    Bool,
    Integer,
    Float,
    String,
    Json,                 // Phase 1：无 schema payload，决策点 1
    Binary,
    Array(Box<PinType>),
    Custom(String),       // 决策点 2：String 而非 &'static str
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-export", derive(TS), ts(export))]
pub struct PinDefinition {
    pub id: String,
    pub label: String,
    pub pin_type: PinType,
    pub direction: PinDirection,
    pub required: bool,
    #[serde(default)]
    #[cfg_attr(feature = "ts-export", ts(optional))]
    pub description: Option<String>,
}

impl PinDefinition {
    pub fn default_input() -> Self { /* id: "in", label: "in", Any, Input, required: true */ }
    pub fn default_output() -> Self { /* id: "out", label: "out", Any, Output, required: false */ }
}
```

- [x] **Step 2：实现 `PinType::is_compatible_with` 兼容矩阵**

按 ADR-0010 部署期校验规则：

| from \ to | Any | T | Array(Any) | Array(T) | Json | Custom(s) |
|-----------|-----|---|------------|----------|------|-----------|
| Any | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ |
| T | ✓ | ✓ if T==U | ✗ | ✗ | ✗ | ✗ |
| Array(Any) | ✓ | ✗ | ✓ | ✓ | ✗ | ✗ |
| Array(T) | ✓ | ✗ | ✓ | ✓ if T==U | ✗ | ✗ |
| Json | ✓ | ✗ | ✗ | ✗ | ✓ | ✗ |
| Custom(s) | ✓ | ✗ | ✗ | ✗ | ✗ | ✓ if s==t |

返回 `bool`，方向是"上游 from 输出能不能流到下游 to 输入"。

- [x] **Step 3：单元测试覆盖兼容矩阵**

至少覆盖：
- `Any → 所有类型` 都通过
- `所有类型 → Any` 都通过
- `String → Integer` 拒绝
- `Array(Any) → Array(Integer)` 通过（上游承诺）
- `Array(Integer) → Array(String)` 拒绝
- `Custom("modbus-register") → Custom("modbus-register")` 通过
- `Custom("a") → Custom("b")` 拒绝

- [x] **Step 4：在 `EngineError` 加三个变体**

```rust
#[error("边 `{from_node}.{from_pin}` → `{to_node}.{to_pin}` 类型不兼容：上游 {from_type:?}，下游期望 {to_type:?}")]
IncompatiblePinTypes {
    from_node: String, from_pin: String,
    to_node: String, to_pin: String,
    from_type: String, to_type: String,  // Debug 字符串化，避免在 Error 上加 PinType 依赖循环
},

#[error("节点 `{node}` 不存在引脚 `{pin}`（方向 {direction}）")]
UnknownPin { node: String, pin: String, direction: String },

#[error("节点 `{node}` 声明了重复的 {direction} 引脚 `{pin}`")]
DuplicatePinId { node: String, pin: String, direction: String },
```

`from_type` / `to_type` 用 `format!("{:?}", pin_type)` 写入字符串，避免 `EngineError`（位于 Ring 0 errors）反向依赖 `PinType`（即将位于 Ring 0 pin）—— 同一 crate 内不会真有循环，但保持 errors 模块零内部耦合是该 crate 的既定风格。

- [x] **Step 5：`crates/core/src/lib.rs` 加 `pub use pin::{PinDefinition, PinDirection, PinType};`**

- [x] **Step 6：`crates/core/AGENTS.md` 加"Pin 声明系统"小节**

简述：
- pin 是**实例级声明**，与 `NodeCapabilities` 类型级标签互补
- `Any` 默认 + 渐进式收紧的迁移哲学
- 新加 `Custom("xxx")` 类型时的 review checklist
- Phase 1 不含 JSON Schema —— 触发条件后开 ADR

---

## Task 1：`NodeTrait` 加 `input_pins` / `output_pins` 默认实现

**Files:**
- Modify: `crates/core/src/node.rs`（trait 加两个默认方法）

- [x] **Step 1：trait 加方法**

```rust
#[async_trait]
pub trait NodeTrait: Send + Sync {
    fn id(&self) -> &str;
    fn kind(&self) -> &'static str;

    /// 输入引脚声明。默认单 `Any` 输入；多输入或具名输入需 override。
    fn input_pins(&self) -> Vec<PinDefinition> {
        vec![PinDefinition::default_input()]
    }

    /// 输出引脚声明。默认单 `Any` 输出；分支节点 / 多端口节点需 override。
    fn output_pins(&self) -> Vec<PinDefinition> {
        vec![PinDefinition::default_output()]
    }

    async fn transform(&self, trace_id: Uuid, payload: Value) -> Result<NodeExecution, EngineError>;
    async fn on_deploy(&self, _ctx: NodeLifecycleContext) -> Result<LifecycleGuard, EngineError> {
        Ok(LifecycleGuard::noop())
    }
}
```

- [x] **Step 2：trait 默认实现的 doc comment 写清"承诺含义"**

参照 `NodeCapabilities` 的"契约 / 反例 / 消费者"三段式，给 `input_pins` / `output_pins` 写明：
- **契约**：返回的 `id` 在该节点上稳定（部署后不可改）；`required: true` 的输入引脚必须有入边
- **消费者**：`deploy.rs` 校验器、未来 IPC `describe_node_pins` 命令、phase 2 前端画布
- **反例**：把 `mqttClient` 的 publish/subscribe 模式 pin 列表写成两个不同的，而是 `output_pins(&self)` 内部按 `self.config.mode` 返回不同列表（实例方法本来就允许）

- [x] **Step 3：`crates/core/src/plugin.rs::tests::StubNode` 不需要改** —— 它会拿默认 Any 实现。其他测试同理。

- [x] **Step 4：`cargo test -p nazh-core` 全绿**

---

## Task 2：部署期校验器（`src/graph/deploy.rs` 阶段 0.5）

**Files:**
- Modify: `src/graph/deploy.rs`
- Create: `src/graph/pin_validator.rs`（新模块，validation 逻辑独立）
- Modify: `src/graph/mod.rs`（`pub(crate) mod pin_validator;`，如有 mod.rs；否则在 `lib.rs` 的 `mod graph` 内）

- [x] **Step 1：实现 `validate_pin_compatibility` 函数**

```rust
// src/graph/pin_validator.rs
pub(crate) fn validate_pin_compatibility(
    nodes: &HashMap<String, Arc<dyn NodeTrait>>,
    edges: &[WorkflowEdge],
) -> Result<(), EngineError> {
    // 1. 对每个节点，构造 pin id → PinDefinition 的索引
    //    检测 DuplicatePinId（同方向同 id 出现两次）
    // 2. 对每条边：
    //    - 若 source_port_id 是 Some，查对应 output pin；否则取 default_output（"out"）
    //    - 若 target_port_id 是 Some，查对应 input pin；否则取 default_input（"in"）
    //    - 若引用了不存在的 pin id → UnknownPin
    //    - 调 from.pin_type.is_compatible_with(&to.pin_type) → IncompatiblePinTypes
    // 3. 检测 required 输入：每个节点的 required input pin 必须有至少一条入边指向它
    Ok(())
}
```

- [x] **Step 2：`deploy_workflow_with_ai` 在阶段 1 前调用 `validate_pin_compatibility`**

```rust
// src/graph/deploy.rs，阶段 1 之前：
// ---- 阶段 0.5：Pin 类型校验 ----
//
// 阶段 0（topology）已校验图是 DAG；本阶段在阶段 1（on_deploy）之前
// 校验每条边两端 pin 类型兼容，确保任何副作用前就拒绝错配。
let mut node_instances: HashMap<String, Arc<dyn NodeTrait>> = HashMap::new();
for node_id in &topology.deployment_order {
    let definition = graph.nodes.get(node_id).ok_or_else(...)?;
    let node = registry.create(definition, shared_resources.clone())?;
    node_instances.insert(node_id.clone(), node);
}
pin_validator::validate_pin_compatibility(&node_instances, &graph.edges)?;

// ---- 阶段 1：on_deploy ----
// 节点已在阶段 0.5 实例化，下方循环改为消费 node_instances
```

注意：阶段 0.5 把 `registry.create()` 提前到了拓扑序遍历之外。阶段 1 的循环改为 `node_instances.remove(node_id)` 取已实例化节点，避免重复 create。这同时让节点的 lifecycle 资源（如 MQTT 连接）的借用时机仍是 `on_deploy` 内（pin 校验只读 trait 方法，不做 IO）。

- [x] **Step 3：阶段 0.5 失败时无 LifecycleGuard 需回滚**（pin 校验在 on_deploy 之前，没有副作用）。仅返回 `Err(...)`，`node_instances` 自然 drop。

- [x] **Step 4：单元测试 `src/graph/pin_validator.rs::tests`**

至少：
- 默认 Any → Any 的图通过（全部现存节点的回归）
- 引用不存在的 pin id 报 `UnknownPin`
- 类型不兼容报 `IncompatiblePinTypes`
- 节点声明重复 pin id 报 `DuplicatePinId`
- 缺少 required 输入的节点报错（用 mock node）

- [x] **Step 5：`tests/workflow.rs` 新增"pin 校验拒绝错配 DAG"集成测试**

构造一个最小 DAG：上游 `String` 输出节点 → 下游 `Integer` 输入节点 → `deploy_workflow` 应返回 `Err(IncompatiblePinTypes)`。

---

## Task 3：迁移 `if` / `tryCatch` —— 静态命名输出

**Files:**
- Modify: `crates/nodes-flow/src/if_node.rs`
- Modify: `crates/nodes-flow/src/try_catch.rs`
- Modify: `crates/nodes-flow/AGENTS.md`（pin 声明表格）

- [x] **Step 1：`IfNode::output_pins` 返回 `["true", "false"]` 两个 `Any` 输出**

```rust
fn output_pins(&self) -> Vec<PinDefinition> {
    vec![
        PinDefinition { id: "true".into(),  label: "真".into(),  pin_type: PinType::Any,
                        direction: PinDirection::Output, required: false,
                        description: Some("条件为真时路由到此".into()) },
        PinDefinition { id: "false".into(), label: "假".into(), pin_type: PinType::Any,
                        direction: PinDirection::Output, required: false,
                        description: Some("条件为假时路由到此".into()) },
    ]
}
```

- [x] **Step 2：`TryCatchNode::output_pins` 返回 `["try", "catch"]`**

注意 `TryCatchNode::transform` 内部 `Route(["try"])` / `Route(["catch"])` 已与 pin id 一致，无需改动 transform 主体。

- [x] **Step 3：单元测试在 `crates/nodes-flow/src/{if_node,try_catch}.rs::tests`**

测 `output_pins()` 返回的 id 集合等于 `{"true","false"}` / `{"try","catch"}`，类型均为 `Any`。

- [x] **Step 4：`crates/nodes-flow/AGENTS.md` 增 pin 表格**

```
| 节点 | 输入 pin | 输出 pin |
|------|----------|----------|
| code | in: Any | out: Any |
| if   | in: Any | true: Any / false: Any |
| switch | in: Any | <动态：分支 key + default> |
| loop | in: Any | body: Any / done: Any |
| tryCatch | in: Any | try: Any / catch: Any |
```

---

## Task 4：迁移 `loop` —— 静态命名输出

**Files:**
- Modify: `crates/nodes-flow/src/loop_node.rs`

- [x] **Step 1：`LoopNode::output_pins` 返回 `["body", "done"]`**

`description` 区分 "迭代单项 payload" / "迭代结束信号"。

- [x] **Step 2：单测验证 pin id 集合 = `{"body","done"}`**

---

## Task 5：迁移 `switch` —— 实例级动态输出

**Files:**
- Modify: `crates/nodes-flow/src/switch_node.rs`

- [x] **Step 1：`SwitchNode` 持有 `branches` 配置**

当前 `SwitchNode` 只持有 `default_branch: String`。在构造时把 `config.branches: Vec<SwitchBranchConfig>` 也存进 self（或存 id 列表，节省一份 clone）。

- [x] **Step 2：`output_pins(&self)` 实例方法读 `self.branches` + `default_branch`**

```rust
fn output_pins(&self) -> Vec<PinDefinition> {
    let mut pins: Vec<PinDefinition> = self.branches.iter().map(|b| PinDefinition {
        id: b.key.clone(),
        label: b.label.clone().unwrap_or_else(|| b.key.clone()),
        pin_type: PinType::Any,
        direction: PinDirection::Output,
        required: false,
        description: None,
    }).collect();

    // default_branch 也作为一个端口暴露——switch 在脚本未匹配时会路由到它
    if !pins.iter().any(|p| p.id == self.default_branch) {
        pins.push(PinDefinition {
            id: self.default_branch.clone(),
            label: format!("{}（默认）", self.default_branch),
            pin_type: PinType::Any,
            direction: PinDirection::Output,
            required: false,
            description: Some("脚本返回未匹配任何分支 key 时路由到此".into()),
        });
    }
    pins
}
```

- [x] **Step 3：单测构造 `branches: [{"key": "high"}, {"key": "low"}]`，断言 `output_pins().ids() == {"high","low","default"}`**

- [x] **Step 4：边缘情况测试** —— `branches: []` 时 `output_pins()` 至少包含 `default_branch`。

---

## Task 6：ts-rs 导出 + IPC 透传

**Files:**
- Modify: `crates/core/src/export_bindings.rs` 或对应导出入口（确认现有导出机制位置）
- 验证: `web/src/generated/` 出现 `PinDefinition.ts` / `PinType.ts` / `PinDirection.ts`

- [x] **Step 1：确认 `cargo test -p tauri-bindings --features ts-export export_bindings` 跑通**

ADR-0017 已实施，导出机制完备；新加的 `#[cfg_attr(feature = "ts-export", derive(TS), ts(export))]` 应自动经 `nazh_core::export_bindings::export_all()` 触发。

- [x] **Step 2：检查生成的 `PinType.ts` 递归类型可用**

`Array(Box<PinType>)` 期望生成 `{ Array: PinType }` 形式的递归 union。如 ts-rs 处理不当（已知 ts-rs 对深递归枚举偶有问题），改为 tagged enum：

```rust
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum PinType {
    Any, Bool, Integer, Float, String, Json, Binary,
    Array { inner: Box<PinType> },
    Custom { name: String },
}
```

`tag = "kind"` 让 TS 类型变成可辨识联合，前端用 `switch (pin.kind)` 分派比 ts-rs 默认的 `{ Array: ... }` map 形式好用。**这个改动如真要做，会反向影响 Task 0 的类型定义**——所以执行 Task 0 时**先按上面 tagged 形式直接定义**，省一次返工。

- [x] **Step 3：commit `web/src/generated/` 的 diff**

---

## Task 7：（可选）IPC `describe_node_pins` 命令

> **是否做：可推迟到 Phase 2**。Phase 1 的最小目标是"部署期校验"，IPC 透出 pin schema 主要服务于前端画布，前端不消费就先不做。
> 若执行：~30 行壳层代码 + 一份 ts-rs 类型，工作量小但增加了 phase 1 的 IPC 契约面，更倾向**留到 phase 2**。

**Files (若执行):**
- Modify: `crates/tauri-bindings/src/lib.rs`
- Modify: `src-tauri/src/lib.rs`
- Modify: `web/src/lib/tauri.ts`

- [ ] **Step 1**（推迟到 Phase 2）：定义 `DescribeNodePinsRequest { node_type: String, config: Value }` 与响应 `DescribeNodePinsResponse { input_pins, output_pins }`
- [ ] **Step 2**（推迟到 Phase 2）：壳层命令 `describe_node_pins`，内部 `registry.create(WorkflowNodeDefinition { type: node_type, config, ... })` → 调 `input_pins()` / `output_pins()` → 序列化返回。**注意：实例化要求 config 合法**——失败时返回错误而不是默认 pins。
- [ ] **Step 3**（推迟到 Phase 2）：导出类型 + 前端 `tauri.ts` wrapper

**判断**：本 plan 默认**不执行 Task 7**，phase 2 prerequisite。

---

## Task 8：文档与状态同步

- [x] **Step 1：`docs/adr/0010-pin-声明系统.md` 状态：提议中 → 已实施**（2026-04-XX 完成日期）。
- [x] **Step 2：`docs/adr/README.md` 索引更新 ADR-0010 状态**。
- [x] **Step 3：根 `AGENTS.md`：**
  - "Project Status / Current batch of ADRs" 段：ADR-0010 加 "已实施（2026-04-XX，phase 1）"
  - "ADR Execution Order" 表第 3 行（ADR-0010）打勾，加"phase 1 已落地，phase 2 前端可视化 + phase 3 协议节点收紧另立 plan"备注
- [x] **Step 4：`crates/core/AGENTS.md`** 新增"Pin 声明系统"章节（在 `NodeCapabilities` 章节之后）。
- [x] **Step 5：`crates/nodes-flow/AGENTS.md`** 新增 pin 声明表格（Task 3 Step 4 已经做过——确认存在）。
- [x] **Step 6：本 plan 顶部 Status 改为 `merged in <SHA>`**，记录最终 commit。

---

## 验收 checklist

- [x] `cargo test --workspace` 全绿（含新加测试）
- [x] `cargo clippy --workspace --all-targets -- -D warnings` 全绿
- [x] `cargo fmt --all -- --check` 通过
- [x] `cargo test -p tauri-bindings --features ts-export export_bindings` 跑通，`web/src/generated/` 含 `PinDefinition.ts` / `PinType.ts` / `PinDirection.ts`
- [x] 前端 `tsc --noEmit` 通过（`npm run build` 等价的 type-check 阶段，Phase 1 不消费 Pin 类型，仅生成的 `.ts` 文件需要可编译）
- [x] `tests/workflow.rs` 现有 E2E 全部仍通过（默认 Any 不破坏存量）
- [x] 新 E2E：构造 `String → Integer` 错配 DAG，`deploy_workflow` 返回 `Err(IncompatiblePinTypes)`
- [x] 集成测试覆盖：`tests/workflow.rs::deploy_拒绝引用未声明_pin_id_的边` 用一条 `source_port_id: "ghost"` 的边触发 `UnknownPin`（与原计划等价，未单独发演示 PR）

---

## Phase 2 / Phase 3 预告（不在本 plan）

- **Phase 2：前端端口可视化**
  - IPC `describe_node_pins` 命令（Task 7）
  - FlowGram 画布按 pin schema 渲染多端口
  - 类型着色（按 `PinType` → 颜色映射）
  - AI 脚本生成把 pin schema 喂进 prompt
- **Phase 3：协议节点 pin 收紧**
  - `modbusRead` / `modbusWrite` 用 `Custom("modbus-register")`
  - `mqttClient` subscribe 模式输出 `Json`，publish 模式输入 `Json` / `String` / `Binary`
  - `httpClient` 输入/输出按 method 区分
  - `sqlWriter` 输入 `Array(Json)` 表示批量行
- **JSON Schema 引入（独立 ADR）**：当 phase 3 协议节点的 `Custom` 类型超过 ~6 个、且至少 2 个需要结构校验时触发新 ADR 评估 `schemars` 引入。

---

## 时间估算

- Task 0：~1.5 小时（类型定义 + 兼容矩阵 + 单测）
- Task 1：~30 分钟（trait 加默认实现 + doc）
- Task 2：~2 小时（校验器 + deploy.rs 阶段 0.5 + 测试）
- Task 3：~30 分钟
- Task 4：~15 分钟
- Task 5：~45 分钟（动态 pin + 单测）
- Task 6：~30 分钟（导出 + 检查 TS）
- Task 7：跳过（推迟到 phase 2）
- Task 8：~30 分钟（文档同步）

合计 phase 1 约 **6 小时**串行实施时间，可作为 1 个 PR 提交。
