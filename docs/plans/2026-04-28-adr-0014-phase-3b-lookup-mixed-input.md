> **Status:** merged — ADR-0014 Phase 3b implemented

# ADR-0014 Phase 3b 实施计划：`lookup` 节点 + 混合输入 payload 合并语义补完

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 把 ADR-0014 spec 第九章 Phase 3 范围内、Phase 3 plan 显式 deferred 出去的两件事补回来——（1）引入第三个 pure-form 节点 `lookup`（配置驱动的 table 查表），让 spec 用例 3 "UE5 表达式树" 的三节点全部到齐；（2）正式拍板"混合输入节点（Exec ▶in + Data ●xxx 同时存在）"的 payload 合并语义并加合约测试。**前置条件**：Phase 3 (`docs/plans/2026-04-28-adr-0014-phase-3-pure-nodes.md`) 已落地（`is_pure_form` / `pull_data_inputs` / `crates/nodes-pure/` / 前端 `isPureForm`）。

**Architecture:**
- **`lookup` 节点（`crates/nodes-pure/src/lookup.rs`）**：仍为 pure-form——单 Data Any 输入 (`key`)、单 Data Any 输出 (`out`)；config 携带 `table: HashMap<String, Value>` + 可选 `default: Option<Value>`。语义：`out = table.get(stringify(key)).cloned().or(default).ok_or(KeyMissing)`。Stringify 规则：标量 (`Bool`/`Integer`/`Float`/`String`) 用 `to_string`，其他用 `serde_json::to_string` 后 trim 引号。
- **混合输入 payload 合并语义**：Phase 3 的 `pull_data_inputs` 已实现 `merge_payload` 函数，但只覆盖了"被触发节点全是 Data 输入"和"无 Data 输入"两端。本 Phase 显式覆盖**真混合**（Exec payload 是 Object + Data 输入合并到同 Object）的合约：
  - 规则 1：Exec payload 是 Object → Data 值以 pin id 为键插入；**键冲突时 Data 覆盖 Exec**（Data 是声明的依赖，更精确）
  - 规则 2：Exec payload 是标量/数组 → 重写为 `{"in": exec_payload, <data_pin_id>: ...}`
  - 规则 3：键名 `"in"` 保留给 Exec 主输入，Data pin id 不得为 `"in"`（部署期校验拒绝）
  - **节点作者契约**：混合输入节点的 transform 必须从 `payload.<data_pin_id>` 读 Data 输入（不是从 root payload 读），符合 Phase 3 已建立的 `c2f` 风格。
- **测试基建**：新 stub 节点 `MixedFormatterNode`（Exec ▶in + Data ●ext，输出 `out` Exec），覆盖 Phase 3 已有的 `pull_data_inputs` 在真混合场景下行为。集成测试 `tests/pin_kind_phase3b.rs` 复用 Phase 3 的 `CelsiusSourceNode` 模式。
- **前端**：`web/src/components/flowgram/nodes/lookup.ts` 节点定义（"纯计算"分类，与 c2f / minutesSince 同 palette）；config schema 编辑器 (`SelectedNodeDraft.lookupTable: Record<string, Value>`)，沿用 `code` 节点的"键值表编辑器"模式（如有）或新建简单的 React 组件。
- **跨语言契约**：fixture `tests/fixtures/mixed_input_merge.jsonc` 列举 6 种 (exec, data) 合并场景的预期产出，Rust + Vitest 共享。

**Tech Stack:** Rust（`crates/nodes-pure/src/lookup.rs`，`crates/core/src/pin.rs` 部署期校验扩展，`src/graph/pin_validator.rs` 加 `data_pin_id != "in"` 检查），TypeScript / React（`web/src/components/flowgram/nodes/lookup.ts`，settings panel 的 lookup table 编辑器组件），Vitest，Playwright，ts-rs（无新类型）。

---

## File Structure

| 操作 | 路径 | 责任 |
|------|------|------|
| 创建 | `crates/nodes-pure/src/lookup.rs` | `LookupNode` + `LookupNodeConfig` + 单元测试（含 stringify 规则） |
| 修改 | `crates/nodes-pure/src/lib.rs` | `mod lookup; pub use lookup::*;` + `register("lookup", PURE, ...)` |
| 修改 | `crates/nodes-pure/AGENTS.md` | 节点目录表加 `lookup` 行 |
| 修改 | `src/lib.rs` | re-export `LookupNode` / `LookupNodeConfig` |
| 修改 | `src/registry.rs` | `pure_plugin_注册全部纯计算节点` 测试加 `lookup` 断言 |
| 修改 | `src/graph/pin_validator.rs` | 新增校验：Data 输入 pin id 不得为字面量 `"in"`（与混合输入合并保留键冲突）+ 单测 |
| 修改 | `crates/core/src/error.rs` | 新增 `EngineError::ReservedPinId { node, pin, reason }` |
| 修改 | `src/graph/pull.rs` | `merge_payload` 注释升级到正式契约语言（保留键冲突规则）；加跨语言 fixture 单测 |
| 创建 | `tests/fixtures/mixed_input_merge.jsonc` | 6 case fixture（混合输入 payload 合并） |
| 创建 | `crates/core/tests/mixed_input_merge_contract.rs` | Rust 侧消费 fixture |
| 修改 | `web/src/lib/__tests__/pin-validator.test.ts` 或新文件 `merge-payload.test.ts` | TS 侧消费同一 fixture（在前端纯函数化 `mergePullPayload`） |
| 创建 | `web/src/lib/merge-payload.ts` | 前端 `mergePullPayload(execPayload, dataValues)` 纯函数（与 Rust 同语义）；用于 AI prompt / 调试视图预览 |
| 创建 | `tests/pin_kind_phase3b.rs` | 集成测试：`source(Exec) → mixedFormatter(Exec ▶in + Data ●ext) → sink`，断言 transform 收到 merge 后 payload |
| 创建 | `web/src/components/flowgram/nodes/lookup.ts` | NodeDefinition：单 Data Any 输入 / 单 Data Any 输出 + config schema |
| 创建/修改 | `web/src/components/flowgram/FlowgramNodeSettingsPanel.tsx` | 加 `LookupTableEditor` 子组件（key/value 行 + 增删 + JSON value 编辑） |
| 修改 | `web/src/components/flowgram/flowgram-node-library.ts` | import + ALL_DEFS 加 `lookupDef` |
| 修改 | `web/src/components/flowgram/nodes/settings-shared.ts` | `SelectedNodeDraft` 加 `lookupTable?: Record<string, JsonValue>` + `lookupDefault?: JsonValue` |
| 创建 | `web/e2e/pin-kind-lookup.spec.ts` | Playwright DOM 烟雾：拖入 lookup 后断言 `data-pure-form='true'` + table editor 可见 |
| 修改 | `docs/adr/0014-执行边与数据边分离.md` | 实施进度追加 Phase 3b 段（commit 范围 + 完成的 deferred 项） |
| 修改 | `AGENTS.md` | ADR-0014 状态行、ADR Execution Order #8 同步 |

---

## Out of scope

1. **`lookup` 节点的 hot-reload**——config 改动后实时生效。Phase 3b 的 `lookup` 仍是部署期固化（与 ADR-0011 PURE 缓存策略冲突时再 revisit）
2. **复杂 key 类型（Object / Array）的 stringify**——本 Phase 仅支持标量 key；复杂 key 触发 `EngineError::Node` 并明确报错信息
3. **`formatJson` 节点本身**——本 Phase 用 stub `MixedFormatterNode` 验证混合输入合并语义；`formatJson` 作为生产节点的引入留给后续（如果有需求）
4. **多个 Data 输入到同一 pin id 的歧义**——部署期 `pin_validator` 已经禁止"同一 input pin 多个上游边"（ADR-0010 Phase 1 校验），不在本 Phase 范围
5. **跨工作流 lookup 表**——纯节点不读 `WorkflowVariables`（per `crates/nodes-pure/AGENTS.md` 内部约定）

---

## Task 1: 部署期校验 — Data 输入 pin id 不得为字面量 `"in"`

**Files:**
- Modify: `crates/core/src/error.rs` — 新增 `ReservedPinId`
- Modify: `src/graph/pin_validator.rs` — 加规则 + 单测

- [ ] **Step 1: `crates/core/src/error.rs` 加 variant**

```rust
    /// ADR-0014 Phase 3b：Data 输入引脚使用了保留的 pin id。
    /// `"in"` 是混合输入节点 payload 合并约定中 Exec 主输入的固定键，
    /// Data 输入若也用 `"in"` 会覆盖 Exec payload，违反节点作者契约。
    #[error("节点 `{node}` 的引脚 `{pin}` 使用了保留 id：{reason}")]
    ReservedPinId {
        node: String,
        pin: String,
        reason: String,
    },
```

re-export from `crates/core/src/lib.rs` if not via blanket re-export.

- [ ] **Step 2: `src/graph/pin_validator.rs` 加校验循环**

定位 `validate_pin_compatibility` 函数（grep），在末尾加：

```rust
    // ADR-0014 Phase 3b：Data 输入引脚 id 不得为 "in"
    for (node_id, node) in nodes_by_id {
        for pin in node.input_pins() {
            if pin.kind == PinKind::Data && pin.id == "in" {
                return Err(EngineError::ReservedPinId {
                    node: node_id.clone(),
                    pin: pin.id.clone(),
                    reason: "Data 输入 pin id 不得为 \"in\"——保留给混合输入 payload 合并的 Exec 主输入键".to_owned(),
                });
            }
        }
    }
```

- [ ] **Step 3: 加单元测试**

在 pin_validator.rs 的 `#[cfg(test)]` 模块加：

```rust
#[test]
fn data_输入_pin_id_为_in_时拒绝部署() {
    use async_trait::async_trait;
    use std::collections::HashMap;
    use std::sync::Arc;

    struct BadNode;
    #[async_trait]
    impl NodeTrait for BadNode {
        fn id(&self) -> &str { "bad" }
        fn kind(&self) -> &str { "bad" }
        fn input_pins(&self) -> Vec<PinDefinition> {
            vec![PinDefinition {
                id: "in".to_owned(),
                label: "in".to_owned(),
                pin_type: PinType::Json,
                direction: PinDirection::Input,
                required: false,
                kind: PinKind::Data,
                description: None,
            }]
        }
        async fn transform(&self, _: uuid::Uuid, p: serde_json::Value) -> Result<NodeExecution, EngineError> {
            Ok(NodeExecution::single(p))
        }
    }

    let nodes: HashMap<String, Arc<dyn NodeTrait>> = HashMap::from([
        ("bad".to_owned(), Arc::new(BadNode) as Arc<dyn NodeTrait>),
    ]);
    let edges = vec![];
    let err = validate_pin_compatibility(&nodes, &edges).unwrap_err();
    assert!(matches!(err, EngineError::ReservedPinId { .. }));
}
```

- [ ] **Step 4: 跑测试 + commit**

```bash
cargo test -p nazh-engine pin_validator
git add crates/core/src/error.rs src/graph/pin_validator.rs
git commit -s -m "feat(graph): ADR-0014 Phase 3b Data 输入 pin id 不得为字面量 \"in\""
```

---

## Task 2: `LookupNode` 实现 + 单元测试

**Files:**
- Create: `crates/nodes-pure/src/lookup.rs`
- Modify: `crates/nodes-pure/src/lib.rs`

- [ ] **Step 1: 创建 `crates/nodes-pure/src/lookup.rs`**

```rust
//! `lookup` 节点：根据 key 在 config 携带的查找表中取 value。
//!
//! pure-form：单 Data Any 输入 (`key`)、单 Data Any 输出 (`out`)。
//! key stringify 规则（与混合输入 payload 合并语义对齐）：
//! - 标量 (Bool/Integer/Float/String)：直接 `to_string` / 字符串去外层引号
//! - 其他：拒绝（返回 `EngineError::Node`，明确报错"lookup key 必须是标量"）

use async_trait::async_trait;
use nazh_core::{
    EngineError, NodeExecution, NodeTrait, PinDefinition, PinDirection, PinKind, PinType,
};
use serde::Deserialize;
use serde_json::Value;
use std::collections::HashMap;
use uuid::Uuid;

#[derive(Debug, Clone, Deserialize)]
pub struct LookupNodeConfig {
    /// 查找表 — key 是字符串，value 任意 JSON。
    #[serde(default)]
    pub table: HashMap<String, Value>,
    /// 未命中时回退 — `None` 表示直接返回错误。
    #[serde(default)]
    pub default: Option<Value>,
}

pub struct LookupNode {
    id: String,
    config: LookupNodeConfig,
}

impl LookupNode {
    pub fn new(id: String, config: LookupNodeConfig) -> Self {
        Self { id, config }
    }

    fn data_input() -> PinDefinition {
        PinDefinition {
            id: "key".to_owned(),
            label: "查找键".to_owned(),
            pin_type: PinType::Any,
            direction: PinDirection::Input,
            required: true,
            kind: PinKind::Data,
            description: Some("标量值（Bool/Integer/Float/String）".to_owned()),
        }
    }

    fn data_output() -> PinDefinition {
        PinDefinition {
            id: "out".to_owned(),
            label: "查找结果".to_owned(),
            pin_type: PinType::Any,
            direction: PinDirection::Output,
            required: false,
            kind: PinKind::Data,
            description: Some("命中的 value，或 default（若配置）".to_owned()),
        }
    }

    fn stringify_key(value: &Value) -> Result<String, EngineError> {
        match value {
            Value::String(s) => Ok(s.clone()),
            Value::Bool(b) => Ok(b.to_string()),
            Value::Number(n) => Ok(n.to_string()),
            _ => Err(EngineError::node_error(
                "lookup",
                format!(
                    "lookup key 必须是标量（Bool/Integer/Float/String），收到 {value:?}"
                ),
            )),
        }
    }
}

#[async_trait]
impl NodeTrait for LookupNode {
    fn id(&self) -> &str {
        &self.id
    }
    fn kind(&self) -> &str {
        "lookup"
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
        let key_value = payload.get("key").ok_or_else(|| {
            EngineError::node_error(
                self.id.clone(),
                "lookup 节点期望 payload.key 存在（由 pull collector 注入）",
            )
        })?;
        let key = Self::stringify_key(key_value)?;
        let value = self.config.table.get(&key).cloned().or_else(|| self.config.default.clone());
        match value {
            Some(v) => Ok(NodeExecution::single(serde_json::json!({ "out": v }))),
            None => Err(EngineError::node_error(
                self.id.clone(),
                format!("lookup 表未命中 key=`{key}` 且无 default 配置"),
            )),
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    fn n(table: serde_json::Value, default: Option<Value>) -> LookupNode {
        let cfg: LookupNodeConfig = serde_json::from_value(serde_json::json!({
            "table": table,
            "default": default,
        }))
        .unwrap();
        LookupNode::new("lk".to_owned(), cfg)
    }

    #[tokio::test]
    async fn 命中字符串_key_返回对应_value() {
        let node = n(serde_json::json!({"alpha": 1, "beta": 2}), None);
        let r = node
            .transform(Uuid::nil(), serde_json::json!({"key": "alpha"}))
            .await
            .unwrap();
        assert_eq!(r.outputs[0].payload, serde_json::json!({"out": 1}));
    }

    #[tokio::test]
    async fn 命中数字_key_自动_stringify() {
        let node = n(serde_json::json!({"42": "answer"}), None);
        let r = node
            .transform(Uuid::nil(), serde_json::json!({"key": 42}))
            .await
            .unwrap();
        assert_eq!(r.outputs[0].payload, serde_json::json!({"out": "answer"}));
    }

    #[tokio::test]
    async fn 命中布尔_key_stringify_为_true_或_false() {
        let node = n(serde_json::json!({"true": "yes", "false": "no"}), None);
        let r = node
            .transform(Uuid::nil(), serde_json::json!({"key": true}))
            .await
            .unwrap();
        assert_eq!(r.outputs[0].payload, serde_json::json!({"out": "yes"}));
    }

    #[tokio::test]
    async fn 未命中_有_default_返回_default() {
        let node = n(serde_json::json!({}), Some(serde_json::json!("fallback")));
        let r = node
            .transform(Uuid::nil(), serde_json::json!({"key": "missing"}))
            .await
            .unwrap();
        assert_eq!(r.outputs[0].payload, serde_json::json!({"out": "fallback"}));
    }

    #[tokio::test]
    async fn 未命中_无_default_返回错误() {
        let node = n(serde_json::json!({}), None);
        let err = node
            .transform(Uuid::nil(), serde_json::json!({"key": "missing"}))
            .await
            .unwrap_err();
        assert!(matches!(err, EngineError::Node { .. }));
    }

    #[tokio::test]
    async fn 复杂_key_直接报错() {
        let node = n(serde_json::json!({}), None);
        let err = node
            .transform(Uuid::nil(), serde_json::json!({"key": [1, 2, 3]}))
            .await
            .unwrap_err();
        assert!(matches!(err, EngineError::Node { .. }));
    }

    #[test]
    fn lookup_是_pure_form() {
        let node = n(serde_json::json!({}), None);
        assert!(nazh_core::is_pure_form(&node));
    }
}
```

- [ ] **Step 2: 在 `crates/nodes-pure/src/lib.rs` 注册**

```rust
mod lookup;
pub use lookup::{LookupNode, LookupNodeConfig};

// register 函数追加：
        registry.register_with_capabilities("lookup", NodeCapabilities::PURE, |def, _res| {
            let config: LookupNodeConfig = def.parse_config()?;
            Ok(Arc::new(LookupNode::new(def.id().to_owned(), config)))
        });
```

- [ ] **Step 3: 跑测试**

```bash
cargo test -p nodes-pure lookup
```

Expected: 7 tests PASS（6 transform + 1 pure_form 断言）。

- [ ] **Step 4: commit**

```bash
git add crates/nodes-pure/src/lookup.rs crates/nodes-pure/src/lib.rs
git commit -s -m "feat(nodes-pure): 实现 lookup 节点（pure-form 配置驱动表查找）"
```

---

## Task 3: facade re-export + registry 契约扩展 + crate AGENTS 同步

**Files:**
- Modify: `src/lib.rs`
- Modify: `src/registry.rs`
- Modify: `crates/nodes-pure/AGENTS.md`

- [ ] **Step 1: `src/lib.rs` 加 re-export**

```rust
pub use nodes_pure::{C2fNode, LookupNode, LookupNodeConfig, MinutesSinceNode, PurePlugin};
```

- [ ] **Step 2: `src/registry.rs` 测试扩展**

```rust
    #[test]
    fn pure_plugin_注册全部纯计算节点() {
        let registry = standard_registry();
        let types = registry.registered_types();

        for expected in ["c2f", "minutesSince", "lookup"] {
            assert!(
                types.contains(&expected),
                "PurePlugin 缺少节点类型: {expected}"
            );
        }
    }
```

- [ ] **Step 3: `crates/nodes-pure/AGENTS.md` 节点目录表加 lookup 行**

```markdown
| `lookup` | `key: Any` (Data) | `out: Any` (Data) | `PURE` |
```

- [ ] **Step 4: 跑测试 + commit**

```bash
cargo test --workspace
git add src/lib.rs src/registry.rs crates/nodes-pure/AGENTS.md
git commit -s -m "feat: facade + registry 注册 lookup 节点"
```

---

## Task 4: 跨语言 fixture — 混合输入 payload 合并 6 case

**Files:**
- Create: `tests/fixtures/mixed_input_merge.jsonc`
- Create: `crates/core/tests/mixed_input_merge_contract.rs`

> **注**：`merge_payload` 函数在 `src/graph/pull.rs` 内（Phase 3 实现）。本 Task 把它的"约定"提升到合约 fixture——任一方修改函数语义必须同步 fixture。

- [ ] **Step 1: 创建 `tests/fixtures/mixed_input_merge.jsonc`**

```jsonc
// ADR-0014 Phase 3b：混合输入节点 payload 合并 6 case 合约。
//
// 合并规则：
// - exec 是 Object → data 值以 pin_id 为键插入；键冲突时 data 覆盖 exec
// - exec 是非 Object → 重写为 {"in": exec, ...data 键}
// - 仍然保留：未声明 Data 输入时 merged === exec
//
// Rust + TS 各自消费同一份 fixture，CI 红线为漂移。
[
  {
    "name": "纯 Exec 触发无 Data 输入",
    "exec_payload": {"value": 25.0},
    "data_values": {},
    "merged": {"value": 25.0}
  },
  {
    "name": "Object Exec + 单 Data 输入键不冲突",
    "exec_payload": {"trace": "abc", "value": 25.0},
    "data_values": {"temp_f": 77.0},
    "merged": {"trace": "abc", "value": 25.0, "temp_f": 77.0}
  },
  {
    "name": "Object Exec + Data 键冲突，Data 覆盖",
    "exec_payload": {"value": 25.0, "label": "from-exec"},
    "data_values": {"label": "from-data"},
    "merged": {"value": 25.0, "label": "from-data"}
  },
  {
    "name": "标量 Exec + Data → 包装为 in 键",
    "exec_payload": 42,
    "data_values": {"ext": "ext-val"},
    "merged": {"in": 42, "ext": "ext-val"}
  },
  {
    "name": "数组 Exec + Data → 包装为 in 键",
    "exec_payload": [1, 2, 3],
    "data_values": {"ext": "ext-val"},
    "merged": {"in": [1, 2, 3], "ext": "ext-val"}
  },
  {
    "name": "多个 Data 输入合并",
    "exec_payload": {"trace": "abc"},
    "data_values": {"a": 1, "b": "hi", "c": [1, 2]},
    "merged": {"trace": "abc", "a": 1, "b": "hi", "c": [1, 2]}
  }
]
```

- [ ] **Step 2: 创建 `crates/core/tests/mixed_input_merge_contract.rs`**

> 由于 `merge_payload` 当前在 facade `src/graph/pull.rs`（不在 nazh-core），合约测试放 facade 而非 core。修改：

实际放 `tests/mixed_input_merge_contract.rs`（仓库根 `tests/`，与其他集成测试同级）。

```rust
//! ADR-0014 Phase 3b：mixed_input_merge fixture 跨语言契约（Rust 端）。

#![allow(clippy::unwrap_used)]

use serde::Deserialize;
use serde_json::Value;
// merge_payload 不是 pub —— 通过 pull_data_inputs 间接覆盖 + 直接函数测试要求暴露。
// 决策：把 merge_payload 升级为 pub(crate) 改为 `#[doc(hidden)] pub`，由本测试单独路径访问。

// 简化：本 Phase 把 merge_payload 改 pub
use nazh_engine::__test_only_merge_payload as merge_payload;

#[derive(Deserialize)]
struct Case {
    name: String,
    exec_payload: Value,
    data_values: serde_json::Map<String, Value>,
    merged: Value,
}

#[test]
fn fixture_穷尽_6_case() {
    let raw = std::fs::read_to_string("tests/fixtures/mixed_input_merge.jsonc").unwrap();
    let stripped: String = raw
        .lines()
        .map(|l| if let Some(idx) = l.find("//") { &l[..idx] } else { l })
        .collect::<Vec<_>>()
        .join("\n");
    let cases: Vec<Case> = serde_json::from_str(&stripped).unwrap();
    assert_eq!(cases.len(), 6);

    for case in cases {
        let actual = merge_payload(case.exec_payload.clone(), case.data_values.clone());
        assert_eq!(
            actual, case.merged,
            "case `{}` 合并结果不匹配",
            case.name
        );
    }
}
```

- [ ] **Step 3: 在 `src/graph/pull.rs` 暴露 test-only 入口**

`src/lib.rs` 加：

```rust
#[doc(hidden)]
pub use crate::graph::pull::merge_payload as __test_only_merge_payload;
```

`src/graph/pull.rs` 把 `fn merge_payload` 从私有提升为 `pub(crate) fn merge_payload`。

- [ ] **Step 4: 跑契约测试**

```bash
cargo test --test mixed_input_merge_contract
```

Expected: PASS。

- [ ] **Step 5: commit**

```bash
git add tests/fixtures/mixed_input_merge.jsonc tests/mixed_input_merge_contract.rs src/graph/pull.rs src/lib.rs
git commit -s -m "test(graph): ADR-0014 Phase 3b 混合输入 payload 合并 6 case 跨语言 fixture"
```

---

## Task 5: 前端 `mergePullPayload` 函数 + Vitest fixture 共享

**Files:**
- Create: `web/src/lib/merge-payload.ts`
- Create: `web/src/lib/__tests__/merge-payload.test.ts`

- [ ] **Step 1: 创建 `web/src/lib/merge-payload.ts`**

```typescript
/**
 * ADR-0014 Phase 3b：与 Rust `src/graph/pull.rs::merge_payload` 等价的 TS 实现。
 *
 * 用途：AI prompt 生成预览、前端调试视图模拟"transform 期看到的 payload"。
 * 不是运行期路径——运行期 payload 合并由 Rust Runner 完成，前端只在解释/预览
 * 场景使用本函数。
 *
 * 合约 fixture：`tests/fixtures/mixed_input_merge.jsonc`（仓库根，与 Rust 共享）。
 */
export function mergePullPayload(
  execPayload: unknown,
  dataValues: Record<string, unknown>,
): unknown {
  if (
    execPayload !== null &&
    typeof execPayload === 'object' &&
    !Array.isArray(execPayload)
  ) {
    return { ...(execPayload as Record<string, unknown>), ...dataValues };
  }
  return { in: execPayload, ...dataValues };
}
```

- [ ] **Step 2: 创建 `web/src/lib/__tests__/merge-payload.test.ts`**

```typescript
import { describe, expect, it } from 'vitest';
import { readFileSync } from 'node:fs';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { mergePullPayload } from '../merge-payload';

const __dirname = dirname(fileURLToPath(import.meta.url));

describe('mergePullPayload — fixture parity with Rust merge_payload', () => {
  const raw = readFileSync(
    resolve(__dirname, '../../../../tests/fixtures/mixed_input_merge.jsonc'),
    'utf8',
  );
  const stripped = raw
    .split('\n')
    .map((line) => {
      const idx = line.indexOf('//');
      return idx >= 0 ? line.slice(0, idx) : line;
    })
    .join('\n');
  const cases = JSON.parse(stripped) as Array<{
    name: string;
    exec_payload: unknown;
    data_values: Record<string, unknown>;
    merged: unknown;
  }>;

  it.each(cases)('$name', (c) => {
    expect(mergePullPayload(c.exec_payload, c.data_values)).toEqual(c.merged);
  });
});
```

- [ ] **Step 3: 跑 Vitest**

```bash
npm --prefix web run test -- merge-payload
```

Expected: 6 case 全 PASS。

- [ ] **Step 4: commit**

```bash
git add web/src/lib/merge-payload.ts web/src/lib/__tests__/merge-payload.test.ts
git commit -s -m "feat(web): mergePullPayload 函数 + 跨语言 fixture 共享（ADR-0014 Phase 3b）"
```

---

## Task 6: 端到端集成测试 — 真混合输入节点拉链

**Files:**
- Create: `tests/pin_kind_phase3b.rs`

- [ ] **Step 1: 创建 `tests/pin_kind_phase3b.rs`**

```rust
//! ADR-0014 Phase 3b：真混合输入（Exec ▶in + Data ●ext）节点端到端测试。
//!
//! 验证 Phase 3 实现的 `pull_data_inputs` 在混合场景下正确合并 payload，
//! 节点 transform 收到既含 Exec push 内容又含 Data pull 值的 payload。

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::{collections::HashMap, sync::Arc, time::Duration};

use async_trait::async_trait;
use nazh_core::{
    EngineError, NodeCapabilities, NodeExecution, NodeRegistry, NodeTrait, PinDefinition,
    PinDirection, PinKind, PinType,
};
use nazh_engine::{
    standard_registry, ConnectionManager, WorkflowEdge, WorkflowGraph, WorkflowNodeDefinition,
};
use serde_json::{json, Value};
use tokio::time::timeout;
use uuid::Uuid;

// ---- stub：Exec 触发 + Data 输出（提供 lookup 的 key）----

struct KeyEmitterNode {
    id: String,
    key: String,
}

#[async_trait]
impl NodeTrait for KeyEmitterNode {
    fn id(&self) -> &str { &self.id }
    fn kind(&self) -> &str { "keyEmitter" }
    fn input_pins(&self) -> Vec<PinDefinition> {
        vec![PinDefinition::default_input()]
    }
    fn output_pins(&self) -> Vec<PinDefinition> {
        vec![
            PinDefinition::default_output(),
            PinDefinition::output_named_data(
                "key",
                "key",
                PinType::Any,
                "测试用：写入 Data 缓存的常量 key",
            ),
        ]
    }
    async fn transform(&self, _: Uuid, _: Value) -> Result<NodeExecution, EngineError> {
        Ok(NodeExecution::single(json!({ "key": self.key })))
    }
}

// ---- stub：混合输入（Exec ▶in + Data ●ext），输出合并后 payload 给 sink ----

struct MixedFormatterNode {
    id: String,
    captured: tokio::sync::mpsc::Sender<Value>,
}

#[async_trait]
impl NodeTrait for MixedFormatterNode {
    fn id(&self) -> &str { &self.id }
    fn kind(&self) -> &str { "mixedFormatter" }
    fn input_pins(&self) -> Vec<PinDefinition> {
        vec![
            PinDefinition::default_input(),
            PinDefinition {
                id: "ext".to_owned(),
                label: "ext".to_owned(),
                pin_type: PinType::Any,
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
    async fn transform(&self, _: Uuid, payload: Value) -> Result<NodeExecution, EngineError> {
        // 把 transform 看到的合并 payload 上报
        self.captured.send(payload.clone()).await.ok();
        Ok(NodeExecution::single(payload))
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn 混合输入节点的_transform_收到合并后_payload() {
    let (tx, mut rx) = tokio::sync::mpsc::channel::<Value>(4);

    let mut registry = standard_registry();
    {
        let tx = tx.clone();
        registry.register_with_capabilities(
            "mixedFormatter",
            NodeCapabilities::empty(),
            move |def, _res| {
                Ok(Arc::new(MixedFormatterNode {
                    id: def.id().to_owned(),
                    captured: tx.clone(),
                }))
            },
        );
    }
    registry.register_with_capabilities(
        "keyEmitter",
        NodeCapabilities::empty(),
        |def, _res| {
            let key = def
                .config()
                .get("key")
                .and_then(|v| v.as_str())
                .unwrap_or("alpha")
                .to_owned();
            Ok(Arc::new(KeyEmitterNode {
                id: def.id().to_owned(),
                key,
            }))
        },
    );

    // 图：emitter(Exec out → mixed.in)
    //     emitter(Data key → lookup.key)
    //     lookup(Data out → mixed.ext)
    let nodes = HashMap::from([
        (
            "emitter".to_owned(),
            WorkflowNodeDefinition::new(
                "emitter".to_owned(),
                "keyEmitter".to_owned(),
                json!({ "key": "alpha" }),
            ),
        ),
        (
            "lk".to_owned(),
            WorkflowNodeDefinition::new(
                "lk".to_owned(),
                "lookup".to_owned(),
                json!({
                    "table": { "alpha": "lookup-hit" },
                    "default": null,
                }),
            ),
        ),
        (
            "mixed".to_owned(),
            WorkflowNodeDefinition::new(
                "mixed".to_owned(),
                "mixedFormatter".to_owned(),
                json!({}),
            ),
        ),
    ]);
    let edges = vec![
        WorkflowEdge {
            from: "emitter".to_owned(),
            to: "mixed".to_owned(),
            source_port_id: Some("out".to_owned()),
            target_port_id: Some("in".to_owned()),
        },
        WorkflowEdge {
            from: "emitter".to_owned(),
            to: "lk".to_owned(),
            source_port_id: Some("key".to_owned()),
            target_port_id: Some("key".to_owned()),
        },
        WorkflowEdge {
            from: "lk".to_owned(),
            to: "mixed".to_owned(),
            source_port_id: Some("out".to_owned()),
            target_port_id: Some("ext".to_owned()),
        },
    ];
    let graph = WorkflowGraph {
        name: Some("p3b".to_owned()),
        nodes,
        edges,
        connections: vec![],
        variables: None,
    };

    let cm = Arc::new(tokio::sync::RwLock::new(ConnectionManager::new()));
    let mut deployment = nazh_engine::deploy_workflow(graph, cm, &registry)
        .await
        .unwrap();

    let root = deployment
        .ingress
        .root_senders
        .get("emitter")
        .unwrap()
        .clone();
    let trace = Uuid::new_v4();
    let did = deployment.ingress.store.write(json!({}), 1).unwrap();
    root.send(nazh_core::ContextRef::new(trace, did, None))
        .await
        .unwrap();

    let merged = timeout(Duration::from_secs(5), rx.recv())
        .await
        .unwrap()
        .unwrap();

    // emitter Exec out 推 `{key: "alpha"}` 给 mixed.in；
    // lookup 拉 emitter.key="alpha"，table 命中 "lookup-hit"，
    // mixed.transform 收到 merge_payload({key: "alpha"}, {ext: "lookup-hit"})
    //                  = {key: "alpha", ext: "lookup-hit"}
    assert_eq!(
        merged.get("key").and_then(|v| v.as_str()),
        Some("alpha")
    );
    assert_eq!(
        merged.get("ext").and_then(|v| v.as_str()),
        Some("lookup-hit")
    );

    deployment.shutdown().await;
}
```

- [ ] **Step 2: 跑测试**

```bash
cargo test --test pin_kind_phase3b -- --nocapture
```

Expected: PASS。

- [ ] **Step 3: commit**

```bash
git add tests/pin_kind_phase3b.rs
git commit -s -m "test(adr-0014): Phase 3b 混合输入节点端到端集成测试（Exec + Data via lookup）"
```

---

## Task 7: 前端 lookup 节点定义 + LookupTableEditor

**Files:**
- Create: `web/src/components/flowgram/nodes/lookup.ts`
- Modify: `web/src/components/flowgram/nodes/settings-shared.ts` — `SelectedNodeDraft.lookupTable`
- Modify: `web/src/components/flowgram/FlowgramNodeSettingsPanel.tsx` — 新增 `LookupTableEditor` 子组件 + 接入
- Modify: `web/src/components/flowgram/flowgram-node-library.ts` — import + ALL_DEFS

- [ ] **Step 1: 创建 `web/src/components/flowgram/nodes/lookup.ts`**

按 c2f / minutesSince 节点定义模板（参考 Phase 3 plan Task 11）：

```typescript
import type { NodeDefinition } from './shared';
import { defineNode } from './shared';

export const lookupDef: NodeDefinition = defineNode({
  kind: 'lookup',
  label: '表查找',
  category: 'pure-compute',
  glyph: 'database',
  description:
    'pure-form 配置驱动表查找。输入 key（标量），按 config.table 查 value；未命中走 config.default 或报错。',
  inputPins: [
    {
      id: 'key',
      label: '查找键',
      pinType: { kind: 'any' },
      kind: 'data',
      required: true,
    },
  ],
  outputPins: [
    {
      id: 'out',
      label: '查找结果',
      pinType: { kind: 'any' },
      kind: 'data',
      required: false,
    },
  ],
  defaultConfig: {
    table: {},
    default: null,
  },
});
```

- [ ] **Step 2: `settings-shared.ts` 加 `SelectedNodeDraft` 字段**

```typescript
export interface SelectedNodeDraft {
  // ... 现有字段
  lookupTable?: Record<string, unknown>;
  lookupDefault?: unknown;
}
```

`readNodeDraft` 从 `node.config` 中读取 `table` / `default` 写入 draft：

```typescript
if (kind === 'lookup') {
  draft.lookupTable = (config.table as Record<string, unknown>) ?? {};
  draft.lookupDefault = config.default ?? null;
}
```

`buildNodeConfig` 反向写入：

```typescript
if (kind === 'lookup') {
  return {
    table: draft.lookupTable ?? {},
    default: draft.lookupDefault ?? null,
  };
}
```

- [ ] **Step 3: `FlowgramNodeSettingsPanel.tsx` 加 `LookupTableEditor` 子组件**

```tsx
function LookupTableEditor({
  table,
  defaultValue,
  onTableChange,
  onDefaultChange,
}: {
  table: Record<string, unknown>;
  defaultValue: unknown;
  onTableChange: (next: Record<string, unknown>) => void;
  onDefaultChange: (next: unknown) => void;
}) {
  const entries = Object.entries(table);

  const updateRow = (oldKey: string, newKey: string, newValue: string) => {
    const next = { ...table };
    delete next[oldKey];
    try {
      next[newKey] = JSON.parse(newValue);
    } catch {
      next[newKey] = newValue;
    }
    onTableChange(next);
  };
  const removeRow = (key: string) => {
    const next = { ...table };
    delete next[key];
    onTableChange(next);
  };
  const addRow = () => onTableChange({ ...table, '': null });

  return (
    <div className="lookup-table-editor">
      <label className="lookup-table-editor__label">查找表</label>
      <div className="lookup-table-editor__rows">
        {entries.map(([key, value]) => (
          <div key={key} className="lookup-table-editor__row">
            <input
              className="lookup-table-editor__key"
              defaultValue={key}
              onBlur={(e) => updateRow(key, e.target.value, JSON.stringify(value))}
            />
            <input
              className="lookup-table-editor__value"
              defaultValue={JSON.stringify(value)}
              onBlur={(e) => updateRow(key, key, e.target.value)}
            />
            <button type="button" onClick={() => removeRow(key)}>×</button>
          </div>
        ))}
      </div>
      <button type="button" className="lookup-table-editor__add" onClick={addRow}>
        + 添加行
      </button>
      <label className="lookup-table-editor__default-label">未命中默认值（可空）</label>
      <input
        className="lookup-table-editor__default"
        defaultValue={JSON.stringify(defaultValue)}
        onBlur={(e) => {
          try { onDefaultChange(JSON.parse(e.target.value)); }
          catch { onDefaultChange(e.target.value); }
        }}
      />
    </div>
  );
}
```

接入到 settings panel 主组件——定位 `kind === 'lookup'` 分支并渲染 `LookupTableEditor`。

- [ ] **Step 4: `flowgram-node-library.ts` import + ALL_DEFS**

```typescript
import { lookupDef } from './nodes/lookup';
const ALL_DEFS: NodeDefinition[] = [/* ... */, c2fDef, minutesSinceDef, lookupDef];
```

- [ ] **Step 5: 启动 dev server 手动验证**

```bash
cd src-tauri && ../web/node_modules/.bin/tauri dev --no-watch
```

拖入"表查找"节点，settings 面板出现 LookupTableEditor，可加/改/删表行，节点头部仍是 pure-form 绿头（继承 Phase 3 视觉）。

- [ ] **Step 6: commit**

```bash
git add web/src/components/flowgram/
git commit -s -m "feat(web): lookup 节点定义 + LookupTableEditor 设置面板（ADR-0014 Phase 3b）"
```

---

## Task 8: E2E DOM 烟雾

**Files:**
- Create: `web/e2e/pin-kind-lookup.spec.ts`

- [ ] **Step 1: 参考 Phase 3 Task 13 的 `pin-kind-pure-nodes.spec.ts` 模板**

```typescript
import { expect, test } from '@playwright/test';

test.describe('ADR-0014 Phase 3b — lookup 节点视觉 + 设置面板烟雾', () => {
  test('拖入 lookup 后节点 DOM 携带 data-pure-form=true', async ({ page }) => {
    await page.goto('/');
    await expect(page.locator('.flowgram-canvas')).toBeVisible();

    const paletteItem = page.locator('[data-node-kind="lookup"]').first();
    await paletteItem.dragTo(page.locator('.flowgram-canvas'));

    const node = page.locator('.flowgram-node-card[data-node-kind="lookup"]');
    await expect(node).toBeVisible();
    await expect(node).toHaveAttribute('data-pure-form', 'true');

    // 选中节点 → 设置面板出现 LookupTableEditor
    await node.click();
    await expect(page.locator('.lookup-table-editor')).toBeVisible();
  });
});
```

- [ ] **Step 2: 跑 E2E**

```bash
npm --prefix web run test:e2e -- pin-kind-lookup
```

Expected: PASS。

- [ ] **Step 3: commit**

```bash
git add web/e2e/pin-kind-lookup.spec.ts
git commit -s -m "test(e2e): ADR-0014 Phase 3b lookup 节点 + 设置面板烟雾"
```

---

## Task 9: 文档更新

**Files:**
- Modify: `docs/adr/0014-执行边与数据边分离.md` — 实施进度追加 Phase 3b
- Modify: `AGENTS.md` — 状态行 + ADR Execution Order 同步

- [ ] **Step 1: ADR 文档实施进度章节加 Phase 3b**

定位 Phase 3 条目，紧邻其后加：

```markdown
- ✅ **Phase 3b（YYYY-MM-DD）**：补完 Phase 3 spec 范围内 deferred 的 2 项——`lookup`
  节点（pure-form，配置驱动 table 查找，3 标量 key stringify 规则 + default fallback）
  + 混合输入 payload 合并语义正式拍板（Exec Object → Data 键插入；Exec 标量/数组
  → 包装到 `"in"` 键；Data 键冲突时覆盖 Exec）。部署期校验拒绝 Data 输入 pin id
  使用保留字面量 `"in"`。跨语言 fixture `tests/fixtures/mixed_input_merge.jsonc`
  6 case 穷尽。集成测试 `tests/pin_kind_phase3b.rs` 用 `keyEmitter → lookup →
  mixedFormatter` 三段拉链端到端验证。
```

- [ ] **Step 2: AGENTS.md ADR-0014 状态行 + 执行顺序同步**

把"Phase 3 plan 已写、待执行"改为"Phase 1+2+3 已实施 / Phase 3b plan 已写、待执行"（如果 Phase 3 已实施）或"Phase 1+2+3+3b 已实施"（如果一并落地）。

- [ ] **Step 3: 全量验证 + commit**

```bash
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all -- --check
npm --prefix web run test
git add docs/adr/0014-执行边与数据边分离.md AGENTS.md
git commit -s -m "docs(adr-0014): Phase 3b 落地后状态同步"
```

---

## Self-Review

### Spec coverage

- ✅ `lookup` 节点（spec 用例 3 第三节点）—— Task 2
- ✅ 混合输入节点 payload 合并语义（Phase 3 plan 显式 deferred 项 #2）—— Task 4 + 5 + 6
- ✅ 部署期校验：Data 输入 pin id 不得为 `"in"`—— Task 1（防止合并语义被破坏）

### Placeholder scan

- 已检：所有 task 都给出实际代码或具体 grep 引用
- Task 5 / Task 7 中的 React 组件 className（`lookup-table-editor` 等）需对照前端现有 CSS 命名风格——若仓库用 BEM 或 Tailwind，按实际改

### Type consistency

- `LookupNodeConfig { table: HashMap<String, Value>, default: Option<Value> }` —— Task 2 / Task 6 / Task 7 一致
- `mergePullPayload(execPayload, dataValues)` (TS) ↔ `merge_payload(exec_payload, data_values)` (Rust) —— 完全镜像
- 6 case fixture 字段 `name / exec_payload / data_values / merged` —— Rust + TS 测试代码一致

### 已知风险

- **Task 4 Step 3 把 `merge_payload` 暴露为 `pub(crate)` + facade `__test_only_*`**：稍 hacky，但比把整个 `pull` 模块改 pub 干净。Phase 4 把 `merge_payload` 内化到一个更明确的 `PayloadMerger` 类型时再 polish。
- **Task 7 LookupTableEditor 的 onBlur defaultValue 模式**：每次重新渲染 input 不会重置——需要 verify 编辑流不会丢掉中间状态。如有问题，改受控组件 + `useState` 临时持有。

---

## Implementation note

每条 task 单 commit，sign-off + 中文 commit msg + 无 `--no-verify`。Phase 3b 预期 9 commits。**实施前置**：Phase 3 必须已落地（`is_pure_form` / `pull_data_inputs` / `crates/nodes-pure/`）。
