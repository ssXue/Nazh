> **Status:** merged in c4e942e

# ADR-0012 工作流变量 Phase 1 Implementation Plan

> **Status:** ✅ 全部 9 commit 已合入 main（2026-04-27）
> 落地点：ADR-0012 → "已实施"、`docs/adr/README.md` 索引、`AGENTS.md` Project Status / ADR Execution Order 表均已更新。

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 在 Ring 0 引入 `WorkflowVariables`（DashMap 后端 + `PinType` 化类型系统）+ `WorkflowGraph` 增加 `variables: { name: { type, initial } }` 声明字段，部署期初始化并注入 `NodeLifecycleContext` 与 `SharedResources`，让 Rhai 脚本通过 `vars.get(...)` / `vars.set(...)` 读写变量。最小 IPC `snapshot_workflow_variables` 暴露当前快照。前端变量面板与变更事件广播留给 Phase 2 独立 plan。

**Architecture:**
- **Ring 0（`crates/core`）**：新增 `variables.rs`，提供 `WorkflowVariables` / `TypedVariable` / `VariableDeclaration` 三类型；类型系统直接复用 `PinType`（不引入第二套类型词汇表）。`NodeLifecycleContext` 加 `variables: Arc<WorkflowVariables>` 字段。
- **Facade（`src/graph/`）**：`WorkflowGraph` 加 `variables: HashMap<String, VariableDeclaration>` 字段（serde default = 空，存量图兼容）；新增 `variables_init.rs` 在部署阶段 0.5 之前完成"初值类型校验 + 实例构造"，把 `Arc<WorkflowVariables>` 同时插入 `RuntimeResources`（供节点工厂取）与 `NodeLifecycleContext`（供 `on_deploy` 取）。
- **Ring 1（`crates/scripting`）**：`ScriptNodeBase::new` 增加 `variables: Option<Arc<WorkflowVariables>>` 参数，注册 `vars.get` / `vars.set` / `vars.cas` 三个 Rhai 函数。
- **Ring 1（`crates/nodes-flow`）**：5 个脚本节点工厂（`code` / `if` / `switch` / `loop` / `tryCatch`）从 `SharedResources` 取 `Arc<WorkflowVariables>` 并传进 `ScriptNodeBase::new`。
- **IPC**：`snapshot_workflow_variables(workflow_id)` 命令返回 `HashMap<String, TypedVariableSnapshot>`（snapshot 类型不带 `Arc`，可序列化）。

**Tech Stack:** Rust 2024、`dashmap`（已在 workspace deps）、`serde` / `serde_json`、`chrono`（已在 workspace deps，用于 `updated_at`）、`rhai` (Ring 1)。

---

## 背景速览（实施人必读）

本次改动对 trait 与 schema 都有契约影响，动手前请先看：

- `docs/adr/0012-工作流变量.md`（决策原文，尤其是「拟议类型」与「传递到节点与 Rhai」两节）。
- `docs/adr/0009-节点生命周期钩子.md`（`NodeLifecycleContext` 的契约——本次新增 `variables` 字段需保持 RAII 语义）。
- `docs/adr/0010-pin-声明系统.md`（`PinType` 兼容矩阵，本次复用做"变量值与声明类型校验"）。
- `CLAUDE.md` → `## Critical Coding Constraints`（`no unwrap / no unsafe / 节点不碰 DataStore / RAII`）。
- `crates/core/AGENTS.md`（Ring 0 依赖约束——`dashmap` / `chrono` 已是 workspace dep，本次不引入新依赖）。

## 本计划的范围界线

**包含（Phase 1 最小可用版）：**
1. Ring 0 类型 `WorkflowVariables` / `TypedVariable` / `VariableDeclaration` + 单元测试。
2. `WorkflowGraph` schema 加 `variables` 字段 + 反序列化测试。
3. 部署期初始化：阶段 0 校验"声明类型 vs 初值"、构造 `Arc<WorkflowVariables>`、注入两条通道。
4. `NodeLifecycleContext.variables` 字段 + 触发器节点（仅校验机制可达性，不实际改 Timer/Serial/MQTT 业务逻辑）。
5. Rhai `vars.get` / `vars.set` / `vars.cas` 三函数 + ScriptNodeBase 集成 + nodes-flow 五节点工厂迁移。
6. IPC `snapshot_workflow_variables(workflow_id) -> SnapshotWorkflowVariablesResponse`（只读快照）。
7. ts-rs 导出 `TypedVariableSnapshot` / `VariableDeclaration` / `SnapshotWorkflowVariablesResponse`。
8. 文档同步：ADR-0012 状态 → 已实施、`docs/adr/README.md` 索引、`AGENTS.md` Project Status / ADR Execution Order、两份 memory 文件、`crates/core/AGENTS.md`、`crates/scripting/AGENTS.md`（如缺则补）、`crates/nodes-flow/AGENTS.md`。

**不包含（留给 Phase 2 / 后续 ADR）：**
- **前端变量面板**：实时显示 / 编辑 / 历史曲线——独立 plan。
- **变更事件广播**：`VariableChanged` 事件 + Window::emit("workflow://variable-changed")——前端可视化的依赖项，Phase 2 一并做。
- **持久化**：变量进程退出即清零（ADR-0012 风险章节明确第一版不持久化）。
- **超过 `compare_and_swap` 的并发原语**（如 `fetch_add`）：用户复杂场景鼓励 `Arc<Mutex<T>>` 模式，原语扩展按需。
- **变量数量上限 / 命名约束**：先放开，review 时关注；ADR 风险章节提到 ~20 个为 smell 阈值。

---

## File Structure

### 新建

- `crates/core/src/variables.rs` — `WorkflowVariables` / `TypedVariable` / `TypedVariableSnapshot` / `VariableDeclaration` + 单元测试。
- `src/graph/variables_init.rs` — 部署期初始化器：`build_workflow_variables(declarations) -> Result<Arc<WorkflowVariables>, EngineError>`。
- `tests/variables.rs`（在根 `nazh-engine` crate 的 `tests/` 下）— 端到端测试：声明 → 部署 → Rhai 读写 → 撤销时变量丢弃。

### 修改

- `crates/core/src/lib.rs` — 加 `pub mod variables;` 与 re-export。
- `crates/core/src/lifecycle.rs` — `NodeLifecycleContext` 加 `pub variables: Arc<WorkflowVariables>` 字段。
- `crates/core/src/error.rs` — 加 `EngineError` 三个变体：`UnknownVariable { name }` / `VariableTypeMismatch { name, declared, actual }` / `VariableInitialMismatch { name, declared, actual }`。
- `crates/core/AGENTS.md` — 添加 `variables.rs` 到模块表 + 类型表。
- `crates/scripting/src/lib.rs` — `ScriptNodeBase::new` 加 `variables: Option<Arc<WorkflowVariables>>` 参数 + `register_vars_helpers` 函数。
- `crates/scripting/AGENTS.md` —（如缺则补）记录 `vars.*` Rhai 注册契约。
- `crates/nodes-flow/src/{if_node,switch_node,loop_node,try_catch,code_node}.rs` — 工厂从 `SharedResources` 提取 `Arc<WorkflowVariables>` 传给 `ScriptNodeBase::new`。
- `crates/nodes-flow/AGENTS.md` — 注明 5 节点的 vars 接入。
- `src/graph/types.rs` — `WorkflowGraph` 加 `pub variables: HashMap<String, VariableDeclaration>`（默认空）。
- `src/graph/deploy.rs` — `deploy_workflow_with_ai` 在阶段 0.5 之前调 `build_workflow_variables`、把 `Arc<WorkflowVariables>` 插入 `RuntimeResources` 与 `NodeLifecycleContext`。
- `crates/tauri-bindings/src/lib.rs` — 增加 `SnapshotWorkflowVariablesRequest` / `SnapshotWorkflowVariablesResponse` IPC 类型 + ts-rs 导出。
- `src-tauri/src/lib.rs` — 实现 `snapshot_workflow_variables` 命令。
- `docs/adr/0012-工作流变量.md` — 状态 `提议中` → `已实施`，加 Phase 1 落地记录小节。
- `docs/adr/README.md` — 索引行更新。
- `AGENTS.md`（根）— Project Status 加 ADR-0012 已实施、ADR Execution Order 表第 5 项打钩。
- `~/.claude/projects/-home-zhihongniu-Nazh/memory/project_system_architecture.md` — Implementation Progress 加 ADR-0012 条目；NodeLifecycleContext 加新字段说明。
- `~/.claude/projects/-home-zhihongniu-Nazh/memory/project_architecture_review_2026_04.md` — 提案-05 状态改"已实施"。

---

## Task 1: Ring 0 类型 — `WorkflowVariables` / `TypedVariable` / `VariableDeclaration`

**Files:**
- Create: `crates/core/src/variables.rs`
- Modify: `crates/core/src/lib.rs`
- Modify: `crates/core/src/error.rs`

- [x] **Step 1: 加三个 EngineError 变体（先写错误类型，给后续 set/校验用）**

打开 `crates/core/src/error.rs`，在 `EngineError` 枚举里加（紧贴 `IncompatiblePinTypes` 之后保持"类型契约"分组）：

```rust
#[error("工作流变量 `{name}` 不存在")]
UnknownVariable { name: String },

#[error("写入工作流变量 `{name}` 失败：声明类型 `{declared}` 与实际值类型 `{actual}` 不匹配")]
VariableTypeMismatch {
    name: String,
    declared: String,
    actual: String,
},

#[error("工作流变量 `{name}` 初值类型不匹配：声明 `{declared}` / 初值实际 `{actual}`")]
VariableInitialMismatch {
    name: String,
    declared: String,
    actual: String,
},
```

并在文件中已有的"构造助手"区域（`impl EngineError` 内 `pub fn ...` 系列）加：

```rust
pub fn unknown_variable(name: impl Into<String>) -> Self {
    Self::UnknownVariable { name: name.into() }
}

pub fn variable_type_mismatch(
    name: impl Into<String>,
    declared: impl Into<String>,
    actual: impl Into<String>,
) -> Self {
    Self::VariableTypeMismatch {
        name: name.into(),
        declared: declared.into(),
        actual: actual.into(),
    }
}

pub fn variable_initial_mismatch(
    name: impl Into<String>,
    declared: impl Into<String>,
    actual: impl Into<String>,
) -> Self {
    Self::VariableInitialMismatch {
        name: name.into(),
        declared: declared.into(),
        actual: actual.into(),
    }
}
```

- [x] **Step 2: 写 `variables.rs` 的失败测试（测试驱动：先固定契约形状）**

创建 `crates/core/src/variables.rs`，先只写测试模块（避免一次性铺太多代码）：

```rust
//! 工作流级共享可变变量（ADR-0012）。
//!
//! ## 设计要点
//!
//! - 类型系统**直接复用 [`PinType`]**——不引入第二套词汇表。
//! - 后端是 [`DashMap`]——单 key 读写无锁、跨 key 高并发。
//! - 写入前**强制类型校验**：`set` / `compare_and_swap` 拒绝写与声明类型不匹配的值。
//! - 提供 [`compare_and_swap`](WorkflowVariables::compare_and_swap) 做原子递增。
//! - **生命周期与部署同步**：`Arc<WorkflowVariables>` 由 [`build_workflow_variables`]
//!   构造，注入 `NodeLifecycleContext` + `SharedResources`，部署撤销时随 Drop 释放。
//!
//! ## 不包含（Phase 1 范围外）
//!
//! - 持久化（进程退出即清零）。
//! - 变量变更事件广播（Phase 2 与前端面板一并做）。
//!
//! [`build_workflow_variables`]: ../graph/variables_init/fn.build_workflow_variables.html

use std::collections::HashMap;
use std::sync::Arc;

use chrono::{DateTime, Utc};
use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use serde_json::Value;
#[cfg(feature = "ts-export")]
use ts_rs::TS;

use crate::{EngineError, PinType};

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    fn vars_with(name: &str, ty: PinType, initial: Value) -> Arc<WorkflowVariables> {
        Arc::new(
            WorkflowVariables::from_declarations(&HashMap::from([(
                name.to_owned(),
                VariableDeclaration {
                    variable_type: ty,
                    initial,
                },
            )]))
            .expect("初始化应成功"),
        )
    }

    #[test]
    fn 声明并读取_setpoint() {
        let vars = vars_with("setpoint", PinType::Float, Value::from(25.0));
        let read = vars.get("setpoint").unwrap();
        assert_eq!(read.value, Value::from(25.0));
        assert_eq!(read.variable_type, PinType::Float);
        assert!(read.updated_by.is_none(), "初值无 updated_by");
    }

    #[test]
    fn 写入更新值并标记_updated_by() {
        let vars = vars_with("counter", PinType::Integer, Value::from(0_i64));
        vars.set("counter", Value::from(7_i64), Some("node-A"))
            .unwrap();
        let read = vars.get("counter").unwrap();
        assert_eq!(read.value, Value::from(7_i64));
        assert_eq!(read.updated_by.as_deref(), Some("node-A"));
    }

    #[test]
    fn 类型不匹配的写入被拒绝() {
        let vars = vars_with("mode", PinType::String, Value::from("auto"));
        let err = vars
            .set("mode", Value::from(42_i64), Some("node-A"))
            .unwrap_err();
        assert!(matches!(err, EngineError::VariableTypeMismatch { .. }));
        assert_eq!(
            vars.get("mode").unwrap().value,
            Value::from("auto"),
            "拒绝写入后值应保持不变"
        );
    }

    #[test]
    fn 写入未声明的变量返回_unknownvariable() {
        let vars = vars_with("x", PinType::Integer, Value::from(0_i64));
        let err = vars.set("y", Value::from(1_i64), None).unwrap_err();
        assert!(matches!(err, EngineError::UnknownVariable { name } if name == "y"));
    }

    #[test]
    fn cas_期望值匹配时写入成功() {
        let vars = vars_with("counter", PinType::Integer, Value::from(0_i64));
        let ok = vars
            .compare_and_swap(
                "counter",
                &Value::from(0_i64),
                Value::from(1_i64),
                Some("node-A"),
            )
            .unwrap();
        assert!(ok);
        assert_eq!(vars.get("counter").unwrap().value, Value::from(1_i64));
    }

    #[test]
    fn cas_期望值不匹配时返回_false() {
        let vars = vars_with("counter", PinType::Integer, Value::from(0_i64));
        let ok = vars
            .compare_and_swap(
                "counter",
                &Value::from(99_i64),
                Value::from(1_i64),
                None,
            )
            .unwrap();
        assert!(!ok);
        assert_eq!(vars.get("counter").unwrap().value, Value::from(0_i64));
    }

    #[test]
    fn cas_类型不匹配时返回_err() {
        let vars = vars_with("counter", PinType::Integer, Value::from(0_i64));
        let err = vars
            .compare_and_swap(
                "counter",
                &Value::from(0_i64),
                Value::from("oops"),
                None,
            )
            .unwrap_err();
        assert!(matches!(err, EngineError::VariableTypeMismatch { .. }));
    }

    #[test]
    fn snapshot_含全部声明() {
        let mut declarations = HashMap::new();
        declarations.insert(
            "a".to_owned(),
            VariableDeclaration {
                variable_type: PinType::Integer,
                initial: Value::from(1_i64),
            },
        );
        declarations.insert(
            "b".to_owned(),
            VariableDeclaration {
                variable_type: PinType::String,
                initial: Value::from("x"),
            },
        );
        let vars = Arc::new(WorkflowVariables::from_declarations(&declarations).unwrap());
        let snap = vars.snapshot();
        assert_eq!(snap.len(), 2);
        assert!(snap.contains_key("a"));
        assert!(snap.contains_key("b"));
    }

    #[test]
    fn 初值类型不匹配_from_declarations_失败() {
        let mut declarations = HashMap::new();
        declarations.insert(
            "wrong".to_owned(),
            VariableDeclaration {
                variable_type: PinType::Integer,
                initial: Value::from("not-a-number"),
            },
        );
        let err = WorkflowVariables::from_declarations(&declarations).unwrap_err();
        assert!(matches!(err, EngineError::VariableInitialMismatch { .. }));
    }
}
```

- [x] **Step 3: 运行测试验证全部失败（编译失败也算）**

```bash
cargo test -p nazh-core variables -- --nocapture 2>&1 | head -40
```

预期：编译失败 / 类型不存在 — 这就是我们要写实现的清单。

- [x] **Step 4: 实现核心类型（写在 `tests` mod 之前）**

把以下代码放到 `crates/core/src/variables.rs` 顶部（`use` 之后、`#[cfg(test)] mod tests` 之前）：

```rust
/// 工作流变量的声明：类型 + 初值。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-export", derive(TS), ts(export))]
pub struct VariableDeclaration {
    /// 变量的类型契约（复用 `PinType`）。
    #[serde(rename = "type")]
    pub variable_type: PinType,
    /// 部署时的初值；必须能在 [`pin_type_matches_value`] 下匹配 `variable_type`。
    pub initial: Value,
}

/// 单个变量的当前状态（活跃实例，含 `chrono::DateTime` 与最后写入者）。
///
/// 内部表示——通过 [`WorkflowVariables::get`] / [`WorkflowVariables::snapshot`]
/// 拷贝出来。不持有 `Arc<DashMap>` 引用。
#[derive(Debug, Clone)]
pub struct TypedVariable {
    pub value: Value,
    pub variable_type: PinType,
    pub updated_at: DateTime<Utc>,
    pub updated_by: Option<String>,
}

/// IPC 序列化版变量快照（`updated_at` 用 RFC3339 字符串，避免前端处理时区差异）。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-export", derive(TS), ts(export))]
pub struct TypedVariableSnapshot {
    pub value: Value,
    pub variable_type: PinType,
    /// RFC3339 时间戳。
    pub updated_at: String,
    #[cfg_attr(feature = "ts-export", ts(optional))]
    pub updated_by: Option<String>,
}

impl From<TypedVariable> for TypedVariableSnapshot {
    fn from(var: TypedVariable) -> Self {
        Self {
            value: var.value,
            variable_type: var.variable_type,
            updated_at: var.updated_at.to_rfc3339(),
            updated_by: var.updated_by,
        }
    }
}

/// 工作流级共享变量存储。
///
/// 由 `build_workflow_variables`（`src/graph/variables_init.rs`）在部署期构造、
/// 包成 `Arc<WorkflowVariables>` 注入 `NodeLifecycleContext` 与 `SharedResources`。
/// 撤销工作流时随 `Arc` 引用计数归零自然释放。
pub struct WorkflowVariables {
    inner: DashMap<String, TypedVariable>,
}

impl WorkflowVariables {
    /// 从 `WorkflowGraph.variables` 声明集构造。
    ///
    /// 每个声明的 `initial` 必须匹配其 `variable_type`；任一不匹配立即返回错误，
    /// 整个部署在阶段 0 失败（早失败原则）。
    ///
    /// # Errors
    ///
    /// `VariableInitialMismatch` — 任一声明的初值类型与声明类型不匹配。
    pub fn from_declarations(
        declarations: &HashMap<String, VariableDeclaration>,
    ) -> Result<Self, EngineError> {
        let inner = DashMap::with_capacity(declarations.len());
        for (name, declaration) in declarations {
            if !pin_type_matches_value(&declaration.variable_type, &declaration.initial) {
                return Err(EngineError::variable_initial_mismatch(
                    name.clone(),
                    pin_type_label(&declaration.variable_type),
                    json_value_label(&declaration.initial),
                ));
            }
            inner.insert(
                name.clone(),
                TypedVariable {
                    value: declaration.initial.clone(),
                    variable_type: declaration.variable_type.clone(),
                    updated_at: Utc::now(),
                    updated_by: None,
                },
            );
        }
        Ok(Self { inner })
    }

    /// 拷贝读取一份变量（含声明类型与最后写入者）。
    #[must_use]
    pub fn get(&self, name: &str) -> Option<TypedVariable> {
        self.inner.get(name).map(|entry| entry.value().clone())
    }

    /// 仅读取值（Rhai 友好；声明类型/写入者用 `get` 取）。
    #[must_use]
    pub fn get_value(&self, name: &str) -> Option<Value> {
        self.inner.get(name).map(|entry| entry.value().value.clone())
    }

    /// 类型化写入。`updated_by` 一般是节点 id；为 `None` 表示外部接入（IPC、初始化）。
    ///
    /// # Errors
    ///
    /// - `UnknownVariable` — 变量未声明。
    /// - `VariableTypeMismatch` — `value` 类型与声明类型不匹配。
    pub fn set(
        &self,
        name: &str,
        value: Value,
        updated_by: Option<&str>,
    ) -> Result<(), EngineError> {
        let mut entry = self
            .inner
            .get_mut(name)
            .ok_or_else(|| EngineError::unknown_variable(name))?;
        if !pin_type_matches_value(&entry.variable_type, &value) {
            return Err(EngineError::variable_type_mismatch(
                name,
                pin_type_label(&entry.variable_type),
                json_value_label(&value),
            ));
        }
        entry.value = value;
        entry.updated_at = Utc::now();
        entry.updated_by = updated_by.map(str::to_owned);
        Ok(())
    }

    /// 原子比较交换：当前值与 `expected` 相等时写入 `new`。
    ///
    /// 返回 `true` 表示交换成功，`false` 表示当前值不匹配（保持不变）。
    /// 类型不匹配仍返回 `Err`——CAS 不绕过类型校验。
    ///
    /// # Errors
    ///
    /// 同 [`set`](Self::set)。
    pub fn compare_and_swap(
        &self,
        name: &str,
        expected: &Value,
        new: Value,
        updated_by: Option<&str>,
    ) -> Result<bool, EngineError> {
        let mut entry = self
            .inner
            .get_mut(name)
            .ok_or_else(|| EngineError::unknown_variable(name))?;
        if !pin_type_matches_value(&entry.variable_type, &new) {
            return Err(EngineError::variable_type_mismatch(
                name,
                pin_type_label(&entry.variable_type),
                json_value_label(&new),
            ));
        }
        if &entry.value != expected {
            return Ok(false);
        }
        entry.value = new;
        entry.updated_at = Utc::now();
        entry.updated_by = updated_by.map(str::to_owned);
        Ok(true)
    }

    /// 拷贝当前所有变量的快照（IPC / 调试用）。
    #[must_use]
    pub fn snapshot(&self) -> HashMap<String, TypedVariable> {
        self.inner
            .iter()
            .map(|entry| (entry.key().clone(), entry.value().clone()))
            .collect()
    }
}

/// 判定 JSON 值是否匹配 `PinType`。
///
/// 这是"运行时校验"——`PinType::Any` 接受任何 `Value`，标量精确匹配，
/// `Json` 接受 Object / Array，`Binary` 接受 Array of u8 或 base64 字符串
/// （Phase 1 仅校验形态，不解 base64）。
///
/// `Custom` 仅在 `Value` 含 `__custom_type` 标记时才匹配——Phase 1 不展开
/// 复杂语义，把 Custom 类型的写入门槛留给未来"产出 Custom 的节点"自带元数据。
#[must_use]
pub fn pin_type_matches_value(pin_type: &PinType, value: &Value) -> bool {
    match (pin_type, value) {
        (PinType::Any, _) => true,
        (PinType::Bool, Value::Bool(_)) => true,
        (PinType::Integer, Value::Number(n)) => n.is_i64() || n.is_u64(),
        (PinType::Float, Value::Number(_)) => true, // i64/u64/f64 都接受
        (PinType::String, Value::String(_)) => true,
        (PinType::Json, Value::Object(_) | Value::Array(_)) => true,
        (PinType::Binary, Value::Array(arr)) => arr.iter().all(|v| {
            v.as_u64()
                .is_some_and(|n| n <= u64::from(u8::MAX))
        }),
        (PinType::Binary, Value::String(_)) => true, // 假定 base64
        (PinType::Array { inner }, Value::Array(arr)) => {
            arr.iter().all(|item| pin_type_matches_value(inner, item))
        }
        (PinType::Custom { .. }, _) => false, // Phase 1: Custom 只能由声明侧写入（初值）
        _ => false,
    }
}

fn pin_type_label(pin_type: &PinType) -> String {
    serde_json::to_value(pin_type)
        .ok()
        .and_then(|v| v.get("kind").and_then(|k| k.as_str().map(str::to_owned)))
        .unwrap_or_else(|| "<unknown>".to_owned())
}

fn json_value_label(value: &Value) -> &'static str {
    match value {
        Value::Null => "null",
        Value::Bool(_) => "bool",
        Value::Number(n) if n.is_i64() || n.is_u64() => "integer",
        Value::Number(_) => "float",
        Value::String(_) => "string",
        Value::Array(_) => "array",
        Value::Object(_) => "object",
    }
}
```

> **关于 `Custom` 的选择**：Phase 1 拒绝写入 `Custom` 类型变量是有意为之——`Custom` 的语义需要"产生该类型的节点对齐"（参见 ADR-0010 Phase 4 deferred 项），变量与节点的 Custom 引入要同步而非分头开启。声明时初值不允许走 Custom 路径，set 时也拒绝。如果未来需要 `Custom` 变量，触发条件与 ADR-0010 Item 2 共享。

- [x] **Step 5: 在 `lib.rs` 暴露**

打开 `crates/core/src/lib.rs`，在 `pub mod plugin;` 之后加：

```rust
pub mod variables;
```

并在 `pub use ...` 区域加：

```rust
pub use variables::{
    TypedVariable, TypedVariableSnapshot, VariableDeclaration, WorkflowVariables,
    pin_type_matches_value,
};
```

如果启用 `ts-export` feature，在 `export_bindings::export_all()` 函数体内加：

```rust
TypedVariableSnapshot::export()?;
VariableDeclaration::export()?;
```

并在该模块顶部 `use` 列表中加 `TypedVariableSnapshot, VariableDeclaration`。

- [x] **Step 6: 运行测试验证全部通过**

```bash
cargo test -p nazh-core variables 2>&1 | tail -30
```

预期：8 个测试全部通过。

- [x] **Step 7: ts-rs 导出更新（如果 Step 5 修改了 export_bindings）**

```bash
cargo test -p tauri-bindings --features ts-export export_bindings
git status web/src/generated/
```

预期：`web/src/generated/` 出现 `TypedVariableSnapshot.ts` / `VariableDeclaration.ts` 两个新文件 + `index.ts` 更新。

- [x] **Step 8: Commit**

```bash
git add crates/core/src/variables.rs crates/core/src/lib.rs crates/core/src/error.rs web/src/generated/
git commit -s -m "feat(core): ADR-0012 Phase 1 — WorkflowVariables / TypedVariable / VariableDeclaration 三类型 + 类型化 set / CAS"
```

---

## Task 2: `WorkflowGraph` schema 加 `variables` 字段

**Files:**
- Modify: `src/graph/types.rs`
- Test: 走 Step 4 的反序列化新 case + 现有 `tests/workflow.rs` 反向兼容验证

- [x] **Step 1: 写"反序列化空 variables 字段时仍能解析旧图"的测试**

打开 `src/graph/types.rs`，在文件末尾加（如已有 `mod tests` 则追加；否则新建）：

```rust
#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod variables_schema_tests {
    use super::*;

    #[test]
    fn 旧图无_variables_字段反序列化不报错() {
        let json = serde_json::json!({
            "nodes": {},
            "edges": []
        });
        let graph: WorkflowGraph = serde_json::from_value(json).unwrap();
        assert!(graph.variables.is_empty(), "缺省 variables 应为空表");
    }

    #[test]
    fn 新图含_variables_字段反序列化正确() {
        let json = serde_json::json!({
            "nodes": {},
            "edges": [],
            "variables": {
                "setpoint": {
                    "type": { "kind": "float" },
                    "initial": 25.0
                },
                "mode": {
                    "type": { "kind": "string" },
                    "initial": "auto"
                }
            }
        });
        let graph: WorkflowGraph = serde_json::from_value(json).unwrap();
        assert_eq!(graph.variables.len(), 2);
        assert_eq!(
            graph.variables["setpoint"].variable_type,
            nazh_core::PinType::Float
        );
        assert_eq!(
            graph.variables["mode"].initial,
            serde_json::Value::from("auto")
        );
    }
}
```

- [x] **Step 2: 运行测试观察失败**

```bash
cargo test -p nazh-engine variables_schema_tests 2>&1 | tail -20
```

预期：`graph.variables` 字段不存在，编译失败。

- [x] **Step 3: 在 `WorkflowGraph` 加字段**

修改 `src/graph/types.rs` 中的 `WorkflowGraph` 结构（约 20-34 行附近）：

```rust
use crate::{
    CancellationToken, ContextRef, DataStore, EngineError, ExecutionEvent, LifecycleGuard,
    VariableDeclaration, WorkflowContext, WorkflowNodeDefinition,
};
```

`WorkflowGraph` 加字段：

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-export", derive(TS), ts(export))]
pub struct WorkflowGraph {
    #[serde(default)]
    #[cfg_attr(feature = "ts-export", ts(optional))]
    pub name: Option<String>,
    #[serde(default)]
    pub connections: Vec<crate::ConnectionDefinition>,
    #[serde(default)]
    pub nodes: HashMap<String, WorkflowNodeDefinition>,
    #[serde(default)]
    pub edges: Vec<WorkflowEdge>,
    /// ADR-0012：工作流级共享变量声明（`name → { type, initial }`）。空表表示无变量。
    #[serde(default)]
    pub variables: HashMap<String, VariableDeclaration>,
}
```

- [x] **Step 4: 验证测试通过 + 全工作区测试不回归**

```bash
cargo test -p nazh-engine variables_schema_tests
cargo test --workspace --lib 2>&1 | tail -20
```

- [x] **Step 5: ts-rs 导出 + 验证 generated**

```bash
cargo test -p tauri-bindings --features ts-export export_bindings
git diff web/src/generated/WorkflowGraph.ts
```

预期：`WorkflowGraph.ts` 多了 `variables: { [key in string]?: VariableDeclaration }` 字段。

- [x] **Step 6: Commit**

```bash
git add src/graph/types.rs web/src/generated/
git commit -s -m "feat(graph): WorkflowGraph 加 variables 字段（ADR-0012 schema 入口）"
```

---

## Task 3: `NodeLifecycleContext` 加 `variables` 字段

**Files:**
- Modify: `crates/core/src/lifecycle.rs`

- [x] **Step 1: 写"`NodeLifecycleContext` 暴露 variables"的测试**

打开 `crates/core/src/lifecycle.rs`，在 `mod tests` 内追加：

```rust
#[tokio::test]
async fn lifecycle_context_暴露_variables() {
    use crate::{PinType, VariableDeclaration, WorkflowVariables};
    use std::collections::HashMap;

    let mut declarations = HashMap::new();
    declarations.insert(
        "setpoint".to_owned(),
        VariableDeclaration {
            variable_type: PinType::Float,
            initial: serde_json::Value::from(25.0),
        },
    );
    let vars = Arc::new(WorkflowVariables::from_declarations(&declarations).unwrap());

    let store: Arc<dyn DataStore> = Arc::new(ArenaDataStore::new());
    let (event_tx, _event_rx) = mpsc::channel(8);
    let handle = NodeHandle::new("trigger-x", store, vec![], event_tx);
    let token = CancellationToken::new();

    let ctx = NodeLifecycleContext {
        resources: Arc::new(crate::RuntimeResources::new()),
        handle,
        shutdown: token.child_token(),
        variables: Arc::clone(&vars),
    };

    assert_eq!(
        ctx.variables.get("setpoint").unwrap().value,
        serde_json::Value::from(25.0)
    );
}
```

- [x] **Step 2: 运行测试观察失败**

```bash
cargo test -p nazh-core lifecycle_context_暴露_variables 2>&1 | tail -10
```

预期：`NodeLifecycleContext` 缺 `variables` 字段。

- [x] **Step 3: 加字段**

修改 `crates/core/src/lifecycle.rs` 的 `NodeLifecycleContext`：

```rust
use crate::WorkflowVariables;

/// 节点部署钩子可用的受限上下文。
///
/// 由 Runner 在调用 [`NodeTrait::on_deploy`](crate::NodeTrait::on_deploy)
/// 前为每个节点构造一次。`shutdown` 是从工作流根 token 派生的子 token——撤销
/// 整图时根 token 取消会沿派生链广播到所有节点。
pub struct NodeLifecycleContext {
    /// 与节点工厂同款的资源包（含 `SharedConnectionManager`、`Arc<dyn AiService>` 等）。
    pub resources: SharedResources,
    /// 向 DAG 数据通道推消息的句柄；触发器节点用，纯变换节点忽略即可。
    pub handle: NodeHandle,
    /// 撤销信号。后台任务必须在 `tokio::select!` 第一分支监听 `cancelled().await`。
    pub shutdown: CancellationToken,
    /// 工作流级共享变量（ADR-0012）。即使工作流未声明任何变量也是非空 `Arc`
    /// （指向空表），节点无需做 `Option` 分支。
    pub variables: Arc<WorkflowVariables>,
}
```

- [x] **Step 4: 修复 `lifecycle.rs` 内现有 `tests` 模块里手写的 `NodeLifecycleContext { ... }` 字面量**

搜索同文件 `tests` 模块里 `NodeLifecycleContext {` 字面量（如有），全部加 `variables` 字段。如果当前文件 tests 里没有手写 ctx（多数都是直接 spawn 任务测 LifecycleGuard），跳过。

- [x] **Step 5: 验证编译 + 全部测试通过**

```bash
cargo test -p nazh-core lifecycle 2>&1 | tail -15
```

预期：所有 lifecycle 测试通过。

- [x] **Step 6: Commit**

```bash
git add crates/core/src/lifecycle.rs
git commit -s -m "feat(core): NodeLifecycleContext 加 variables 字段（ADR-0012）"
```

---

## Task 4: `variables_init` 模块 — 部署期初始化器

**Files:**
- Create: `src/graph/variables_init.rs`
- Modify: `src/graph/mod.rs`（如缺则补；否则在 `deploy.rs` 同目录注册）

- [x] **Step 1: 写测试（端到端：声明 → 构造 → 类型校验）**

创建 `src/graph/variables_init.rs`，先只写测试块：

```rust
//! 工作流变量的部署期初始化器（ADR-0012）。
//!
//! 把 [`WorkflowGraph::variables`] 声明转成可注入引擎的 `Arc<WorkflowVariables>`。
//! 在 `deploy_workflow_with_ai` 阶段 0（早于 pin 校验、早于 on_deploy）调用——
//! 初值类型不匹配立即整图失败。

use std::sync::Arc;

use nazh_core::{EngineError, VariableDeclaration, WorkflowVariables};

use std::collections::HashMap;

/// 把 schema 声明转成共享变量实例。
///
/// # Errors
///
/// 任一变量的初值类型与声明类型不匹配时返回
/// [`EngineError::VariableInitialMismatch`]。
pub fn build_workflow_variables(
    declarations: &HashMap<String, VariableDeclaration>,
) -> Result<Arc<WorkflowVariables>, EngineError> {
    Ok(Arc::new(WorkflowVariables::from_declarations(declarations)?))
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use nazh_core::PinType;

    #[test]
    fn 空声明_构造_空变量集() {
        let vars = build_workflow_variables(&HashMap::new()).unwrap();
        assert!(vars.snapshot().is_empty());
    }

    #[test]
    fn 含声明_构造成功() {
        let mut decls = HashMap::new();
        decls.insert(
            "x".to_owned(),
            VariableDeclaration {
                variable_type: PinType::Integer,
                initial: serde_json::Value::from(42_i64),
            },
        );
        let vars = build_workflow_variables(&decls).unwrap();
        assert_eq!(vars.get("x").unwrap().value, serde_json::Value::from(42_i64));
    }

    #[test]
    fn 初值类型不匹配_整体失败() {
        let mut decls = HashMap::new();
        decls.insert(
            "bad".to_owned(),
            VariableDeclaration {
                variable_type: PinType::Float,
                initial: serde_json::Value::from("not-a-number"),
            },
        );
        let err = build_workflow_variables(&decls).unwrap_err();
        assert!(matches!(err, EngineError::VariableInitialMismatch { .. }));
    }
}
```

- [x] **Step 2: 在 `src/graph/mod.rs` 注册模块**

打开 `src/graph/mod.rs`（如不存在则它是个目录，用 `src/graph.rs` 形式；按现有形态调整）。在文件中现有 `mod` 列表中加：

```rust
pub mod variables_init;
```

并在 `pub use ...` 区域加：

```rust
pub use variables_init::build_workflow_variables;
```

- [x] **Step 3: 测试通过**

```bash
cargo test -p nazh-engine variables_init
```

- [x] **Step 4: Commit**

```bash
git add src/graph/variables_init.rs src/graph/mod.rs
git commit -s -m "feat(graph): build_workflow_variables 部署期初始化器（ADR-0012）"
```

---

## Task 5: 在 `deploy_workflow_with_ai` 注入变量

**Files:**
- Modify: `src/graph/deploy.rs`
- Test: `tests/variables.rs`（新建端到端集成测试）

- [x] **Step 1: 写端到端测试（声明变量 → 部署 → on_deploy 能读到）**

创建 `tests/variables.rs`：

```rust
//! 端到端：变量声明在部署期初始化、注入 NodeLifecycleContext 与 SharedResources。

use std::collections::HashMap;
use std::sync::Arc;

use nazh_engine::{
    ConnectionManager, NodeRegistry, PinType, VariableDeclaration, WorkflowGraph, WorkflowVariables,
    deploy_workflow_with_ai, standard_registry,
};
use serde_json::json;

#[tokio::test]
async fn 部署时变量按声明初始化() {
    let mut graph = WorkflowGraph {
        name: Some("vars-empty".to_owned()),
        connections: vec![],
        nodes: HashMap::new(),
        edges: vec![],
        variables: HashMap::new(),
    };
    graph.variables.insert(
        "setpoint".to_owned(),
        VariableDeclaration {
            variable_type: PinType::Float,
            initial: json!(25.0),
        },
    );

    let registry: NodeRegistry = standard_registry();
    let cm = Arc::new(ConnectionManager::new());
    let deployment = deploy_workflow_with_ai(graph, cm, None, &registry)
        .await
        .expect("空 DAG + 单变量应能部署");

    // shutdown 前从 deployment 拿不到 vars 句柄；这里只验证部署成功即可——
    // "节点能读到变量"的端到端在 Task 7 的 nodes-flow 测试里覆盖。
    deployment.shutdown().await;
}

#[tokio::test]
async fn 初值类型不匹配_部署失败() {
    let mut graph = WorkflowGraph {
        name: None,
        connections: vec![],
        nodes: HashMap::new(),
        edges: vec![],
        variables: HashMap::new(),
    };
    graph.variables.insert(
        "bad".to_owned(),
        VariableDeclaration {
            variable_type: PinType::Integer,
            initial: json!("not-a-number"),
        },
    );
    let registry: NodeRegistry = standard_registry();
    let cm = Arc::new(ConnectionManager::new());
    let err = deploy_workflow_with_ai(graph, cm, None, &registry)
        .await
        .expect_err("初值类型不匹配应阻止部署");
    let msg = err.to_string();
    assert!(
        msg.contains("初值类型不匹配") || msg.contains("VariableInitialMismatch"),
        "错误消息应指出 variable initial mismatch，实际：{msg}"
    );
}
```

- [x] **Step 2: 把 `WorkflowVariables` 在 facade 暴露**

打开 `src/lib.rs`，确认 `pub use nazh_core::{..., VariableDeclaration, WorkflowVariables, ...}` 已在 re-export 列表（Task 1 的 `lib.rs` 修改应该已包含）。如缺，补齐。

- [x] **Step 3: 在 `deploy_workflow_with_ai` 接入**

修改 `src/graph/deploy.rs`：

```rust
use super::variables_init::build_workflow_variables;
```

在 `deploy_workflow_with_ai` 函数内，紧贴 `let topology = graph.topology()?;` 之后加：

```rust
    // ---- 阶段 0：构造工作流变量（早于 connection 装配、Pin 校验） ----
    //
    // 声明的初值类型若与声明类型不匹配立即整图失败——节点尚未实例化、
    // 无 RAII 资源在手，无需回滚（ADR-0012 早失败原则）。
    let workflow_variables = build_workflow_variables(&graph.variables)?;
```

把 `workflow_variables` 放到 `RuntimeResources`（在已有 `let mut resource_bag = RuntimeResources::new()...` 行后追加）：

```rust
    let mut resource_bag = RuntimeResources::new()
        .with_resource(connection_manager.clone())
        .with_resource(Arc::clone(&workflow_variables));
    if let Some(ai_service) = ai_service {
        resource_bag.insert(ai_service);
    }
```

注意 `with_resource` 类型是 `Arc<WorkflowVariables>`——保证 nodes-flow 工厂用 `resources.get::<Arc<WorkflowVariables>>()` 能取到。

接下来把 `workflow_variables` 注入 `NodeLifecycleContext`（修改阶段 1 循环里 `let ctx = NodeLifecycleContext { ... }` 字面量）：

```rust
        let ctx = NodeLifecycleContext {
            resources: shared_resources.clone(),
            handle,
            shutdown: shutdown_token.child_token(),
            variables: Arc::clone(&workflow_variables),
        };
```

- [x] **Step 4: 测试通过**

```bash
cargo test --test variables 2>&1 | tail -20
```

预期：两个集成测试都通过。

- [x] **Step 5: Commit**

```bash
git add src/graph/deploy.rs tests/variables.rs src/lib.rs
git commit -s -m "feat(graph): deploy_workflow 注入 WorkflowVariables 到 SharedResources + LifecycleContext（ADR-0012）"
```

---

## Task 6: Rhai `vars.get` / `vars.set` / `vars.cas` 注册

**Files:**
- Modify: `crates/scripting/src/lib.rs`
- Test: 在 `crates/scripting/src/lib.rs` 的 `tests` 模块（如缺则补）

- [x] **Step 1: 写 Rhai 单元测试**

打开 `crates/scripting/src/lib.rs`，在文件末尾追加：

```rust
#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod variables_tests {
    use super::*;
    use std::collections::HashMap;
    use nazh_core::{PinType, VariableDeclaration, WorkflowVariables};
    use std::sync::Arc;

    fn vars_arc(name: &str, ty: PinType, initial: serde_json::Value) -> Arc<WorkflowVariables> {
        let mut decls = HashMap::new();
        decls.insert(
            name.to_owned(),
            VariableDeclaration {
                variable_type: ty,
                initial,
            },
        );
        Arc::new(WorkflowVariables::from_declarations(&decls).unwrap())
    }

    #[test]
    fn rhai_脚本可读写变量() {
        let vars = vars_arc("counter", PinType::Integer, serde_json::Value::from(5_i64));
        let base = ScriptNodeBase::new(
            "test-script",
            r#"
                let v = vars.get("counter");
                vars.set("counter", v + 1);
                vars.get("counter")
            "#,
            10_000,
            None,
            Some(Arc::clone(&vars)),
        )
        .unwrap();

        let (_, result) = base.evaluate(serde_json::Value::Null).unwrap();
        let final_value = base.dynamic_to_value(&result).unwrap();
        assert_eq!(final_value, serde_json::Value::from(6_i64));
        assert_eq!(
            vars.get("counter").unwrap().value,
            serde_json::Value::from(6_i64)
        );
    }

    #[test]
    fn rhai_脚本写入未声明变量返回错误() {
        let vars = vars_arc("a", PinType::Integer, serde_json::Value::from(0_i64));
        let base = ScriptNodeBase::new(
            "test-script-2",
            r#"vars.set("undeclared", 42)"#,
            10_000,
            None,
            Some(Arc::clone(&vars)),
        )
        .unwrap();
        let err = base.evaluate(serde_json::Value::Null).unwrap_err();
        let msg = format!("{err}");
        assert!(
            msg.contains("undeclared") || msg.contains("UnknownVariable"),
            "错误消息应包含变量名，实际：{msg}"
        );
    }

    #[test]
    fn rhai_脚本_cas_成功返回_true() {
        let vars = vars_arc("c", PinType::Integer, serde_json::Value::from(0_i64));
        let base = ScriptNodeBase::new(
            "test-script-3",
            r#"vars.cas("c", 0, 1)"#,
            10_000,
            None,
            Some(Arc::clone(&vars)),
        )
        .unwrap();
        let (_, result) = base.evaluate(serde_json::Value::Null).unwrap();
        let final_value = base.dynamic_to_value(&result).unwrap();
        assert_eq!(final_value, serde_json::Value::from(true));
    }

    #[test]
    fn rhai_脚本无_variables_注入时_vars_未定义() {
        let base = ScriptNodeBase::new(
            "test-script-4",
            r#"vars.get("anything")"#,
            10_000,
            None,
            None,
        )
        .unwrap();
        let err = base.evaluate(serde_json::Value::Null).unwrap_err();
        let msg = format!("{err}");
        assert!(
            msg.contains("vars") || msg.contains("undefined"),
            "未注入 variables 时调用 vars.* 应失败，实际：{msg}"
        );
    }
}
```

- [x] **Step 2: 测试观察失败**

```bash
cargo test -p nazh-scripting variables_tests 2>&1 | tail -20
```

预期：`ScriptNodeBase::new` 签名不匹配——少了 `variables` 参数。

- [x] **Step 3: 修改 `ScriptNodeBase::new` 签名**

修改 `crates/scripting/src/lib.rs`：

```rust
use std::sync::Arc;
use nazh_core::WorkflowVariables;
```

`ScriptNodeBase::new` 函数签名变为：

```rust
impl ScriptNodeBase {
    /// 创建基座：编译脚本并设置步数上限。
    ///
    /// `variables` 为 `Some(_)` 时注册 Rhai 全局对象 `vars`，提供
    /// `vars.get(name) / vars.set(name, value) / vars.cas(name, expected, new) -> bool`。
    /// 为 `None` 时脚本调用 `vars.*` 会以 ErrorRuntime 报错。
    pub fn new(
        id: impl Into<String>,
        script: &str,
        max_operations: u64,
        ai: Option<ScriptAiRuntime>,
        variables: Option<Arc<WorkflowVariables>>,
    ) -> Result<Self, EngineError> {
        let id = id.into();
        let mut engine = Engine::new();
        engine.set_max_operations(max_operations);
        NazhScriptPackage::new().register_into_engine(&mut engine);
        register_ai_complete(&mut engine, &id, ai);
        register_vars_helpers(&mut engine, &id, variables);
        let ast = engine
            .compile(script)
            .map_err(|error| EngineError::script_compile(id.clone(), error.to_string()))?;
        Ok(Self { id, engine, ast })
    }
    // ...
}
```

加 `register_vars_helpers` 函数（放在 `register_ai_complete` 旁边）：

```rust
fn register_vars_helpers(
    engine: &mut Engine,
    node_id: &str,
    variables: Option<Arc<WorkflowVariables>>,
) {
    let node_id = node_id.to_owned();

    // 不论是否注入变量都注册一个值——None 情况下调用即报错。
    let vars_handle = Arc::new(VarsRhaiBinding {
        node_id: node_id.clone(),
        variables,
    });

    // 三个 Rhai 函数挂在 "vars" 命名空间下——但 Rhai 不直接支持模块化对象，
    // 用 free-function 命名约定 vars_get / vars_set / vars_cas，再在 Engine
    // 内部用 register_global_module 暴露成 vars.get / vars.set / vars.cas。
    //
    // 简化方案：直接注册 fn 名为 "get" / "set" / "cas" 但绑定参数 receiver
    // 类型为 VarsHandle——脚本里通过全局变量 `vars` 调用语法糖。
    //
    // 用 register_fn + 命名约定 `vars_get / vars_set / vars_cas`，脚本侧
    // 用 Rhai 模块暴露语法 `import "vars" as vars; vars::get(...)` 即可——
    // 但项目历史脚本风格更接近 free fn，本 plan 采用 `vars_get(name)` 形式
    // 配合 Rhai 别名让 `vars.get(name)` 等价。
    //
    // 实施细节：register_indexer_get + register_get 把 VarsHandle 暴露成
    // 自定义类型，然后注册全局变量 `vars` 指向 handle 实例。

    let handle_for_get = Arc::clone(&vars_handle);
    engine.register_fn(
        "__vars_get",
        move |name: rhai::ImmutableString| -> Result<Dynamic, Box<EvalAltResult>> {
            handle_for_get.get(&name)
        },
    );
    let handle_for_set = Arc::clone(&vars_handle);
    engine.register_fn(
        "__vars_set",
        move |name: rhai::ImmutableString,
              value: Dynamic|
              -> Result<(), Box<EvalAltResult>> { handle_for_set.set(&name, value) },
    );
    let handle_for_cas = Arc::clone(&vars_handle);
    engine.register_fn(
        "__vars_cas",
        move |name: rhai::ImmutableString,
              expected: Dynamic,
              new: Dynamic|
              -> Result<bool, Box<EvalAltResult>> {
            handle_for_cas.compare_and_swap(&name, expected, new)
        },
    );

    // 暴露 `vars.get(...)` / `vars.set(...)` / `vars.cas(...)` 语法糖：
    // 把 free fn `__vars_*` 挂到一个零大小占位类型 `Vars` 上作为方法。
    engine
        .register_type::<Vars>()
        .register_fn("get", Vars::get_method)
        .register_fn("set", Vars::set_method)
        .register_fn("cas", Vars::cas_method);
}

#[derive(Clone, Default)]
struct Vars;

impl Vars {
    fn get_method(_self: &mut Self, name: rhai::ImmutableString) -> Result<Dynamic, Box<EvalAltResult>> {
        // 实际逻辑通过 free fn 路由——这里的方法只是语法糖载体；
        // engine 必须把同样的 binding 也挂到方法上。简化：方法返回错误，
        // 让 register_fn 里的 free fn 走调用路径。
        Err(to_script_error(format!(
            "vars.get('{name}'): 必须用 register_fn 闭包路径调用——若看到此错误说明绑定丢失"
        )))
    }

    fn set_method(_self: &mut Self, name: rhai::ImmutableString, _value: Dynamic) -> Result<(), Box<EvalAltResult>> {
        Err(to_script_error(format!(
            "vars.set('{name}'): 必须用 register_fn 闭包路径调用——若看到此错误说明绑定丢失"
        )))
    }

    fn cas_method(_self: &mut Self, name: rhai::ImmutableString, _expected: Dynamic, _new: Dynamic) -> Result<bool, Box<EvalAltResult>> {
        Err(to_script_error(format!(
            "vars.cas('{name}'): 必须用 register_fn 闭包路径调用——若看到此错误说明绑定丢失"
        )))
    }
}

struct VarsRhaiBinding {
    node_id: String,
    variables: Option<Arc<WorkflowVariables>>,
}

impl VarsRhaiBinding {
    fn require_vars(&self) -> Result<&Arc<WorkflowVariables>, Box<EvalAltResult>> {
        self.variables.as_ref().ok_or_else(|| {
            to_script_error(format!(
                "脚本节点 `{}` 未注入 vars——工作流定义中 variables 字段为空？",
                self.node_id
            ))
        })
    }

    fn get(&self, name: &str) -> Result<Dynamic, Box<EvalAltResult>> {
        let vars = self.require_vars()?;
        let value = vars.get_value(name).ok_or_else(|| {
            to_script_error(format!("UnknownVariable: 工作流变量 `{name}` 未声明"))
        })?;
        rhai::serde::to_dynamic(value)
            .map_err(|err| to_script_error(format!("变量 `{name}` 无法转 Dynamic：{err}")))
    }

    fn set(&self, name: &str, value: Dynamic) -> Result<(), Box<EvalAltResult>> {
        let vars = self.require_vars()?;
        let value: serde_json::Value = rhai::serde::from_dynamic(&value)
            .map_err(|err| to_script_error(format!("变量 `{name}` 写入值无法序列化：{err}")))?;
        vars.set(name, value, Some(&self.node_id))
            .map_err(|err| to_script_error(err.to_string()))
    }

    fn compare_and_swap(
        &self,
        name: &str,
        expected: Dynamic,
        new: Dynamic,
    ) -> Result<bool, Box<EvalAltResult>> {
        let vars = self.require_vars()?;
        let expected: serde_json::Value = rhai::serde::from_dynamic(&expected)
            .map_err(|err| to_script_error(format!("CAS expected 值反序列化失败：{err}")))?;
        let new: serde_json::Value = rhai::serde::from_dynamic(&new)
            .map_err(|err| to_script_error(format!("CAS new 值反序列化失败：{err}")))?;
        vars.compare_and_swap(name, &expected, new, Some(&self.node_id))
            .map_err(|err| to_script_error(err.to_string()))
    }
}
```

> **关于 vars.get 语法糖路径的实现说明**：本 plan 采用最稳的 Rhai 实现路径——`register_fn` 直接绑定到自定义类型 `Vars` 的方法。Rhai 0.20+ 的 `register_fn("method_name", closure)` 让 closure 直接接收 `&mut Self` + 参数，所以上面的 `Vars::get_method` 仅是占位 — 实际工作的 binding 应该是 `engine.register_fn("get", { let h = handle.clone(); move |_self: &mut Vars, name: rhai::ImmutableString| h.get(&name) })`。如果你（实施者）发现上面 occurance 的 placeholder 方法路径不通，**直接把语法糖去掉**，让脚本写 `__vars_get("name")` / `__vars_set("name", v)` / `__vars_cas("name", e, n)` 同样可用——前端 AI prompt 模板里把语法记号换成下划线版本即可。Phase 1 接受 ergonomic 折衷以保证按时落地。

- [x] **Step 4: 在脚本启动时注入全局变量 `vars`**

`ScriptNodeBase::evaluate` / `evaluate_catching` 当前用 `prepare_scope` 准备 Scope。在那里加：

```rust
fn prepare_scope(&self, payload: Value) -> Result<Scope<'static>, EngineError> {
    let dynamic = to_dynamic(payload)
        .map_err(|error| EngineError::payload_conversion(self.id.clone(), error.to_string()))?;
    let mut scope = Scope::new();
    scope.push_dynamic("payload", dynamic);
    scope.push("vars", Vars);
    Ok(scope)
}
```

(向 scope 推入 Vars 占位实例——register_fn 已挂到该类型的方法上。)

- [x] **Step 5: 测试通过**

```bash
cargo test -p nazh-scripting variables_tests 2>&1 | tail -30
```

预期：4 个测试全部通过。如果某一测试因 Rhai 方法绑定路径问题失败，按 Step 3 的 fallback 注释把 `vars.get(...)` 测试改成 `__vars_get(...)`，prompt 同步调整。

- [x] **Step 6: Commit**

```bash
git add crates/scripting/src/lib.rs
git commit -s -m "feat(scripting): Rhai 注册 vars.get / vars.set / vars.cas（ADR-0012）"
```

---

## Task 7: nodes-flow 五节点工厂迁移

**Files:**
- Modify: `crates/nodes-flow/src/if_node.rs`
- Modify: `crates/nodes-flow/src/switch_node.rs`
- Modify: `crates/nodes-flow/src/loop_node.rs`
- Modify: `crates/nodes-flow/src/try_catch.rs`
- Modify: `crates/nodes-flow/src/code_node.rs`

- [x] **Step 1: 调研当前工厂签名**

跑：

```bash
grep -n "ScriptNodeBase::new" crates/nodes-flow/src/*.rs
```

观察五个节点都是 `ScriptNodeBase::new(id, &config.script, config.max_operations, None)?` 形态（`code_node` 第四参数是 `ai`）。

- [x] **Step 2: 在每个节点工厂取 `Arc<WorkflowVariables>`**

以 `if_node.rs` 为模板，找到节点工厂函数（一般签名 `pub fn build(...)` 或 `pub fn new(definition, resources)`）。在调用 `ScriptNodeBase::new` 之前加：

```rust
use std::sync::Arc;
use nazh_core::WorkflowVariables;

// ... 工厂函数体 ...
let variables = resources.get::<Arc<WorkflowVariables>>();
```

把 `ScriptNodeBase::new` 的调用扩展为：

```rust
base: ScriptNodeBase::new(id, &config.script, config.max_operations, None, variables)?,
```

`code_node.rs` 例外（已有 `ai` 参数）：

```rust
base: ScriptNodeBase::new(id, &script, max_operations, ai, variables)?,
```

对每个节点重复上述改动。

- [x] **Step 3: 编译 + 全工作区测试**

```bash
cargo check --workspace 2>&1 | tail -10
cargo test --workspace --lib 2>&1 | tail -20
```

预期：编译通过，现有测试不回归。

- [x] **Step 4: 写一个跨节点的端到端测试（在 `tests/variables.rs` 追加）**

打开 `tests/variables.rs`，追加：

```rust
#[tokio::test]
async fn rhai_code_节点跨次部署独立持有变量() {
    use nazh_engine::{
        ConnectionManager, NodeRegistry, PinType, VariableDeclaration, WorkflowContext,
        WorkflowGraph, WorkflowNodeDefinition, deploy_workflow_with_ai, standard_registry,
    };

    let mut graph = WorkflowGraph {
        name: Some("rhai-vars-counter".to_owned()),
        connections: vec![],
        nodes: HashMap::new(),
        edges: vec![],
        variables: HashMap::new(),
    };
    graph.variables.insert(
        "counter".to_owned(),
        VariableDeclaration {
            variable_type: PinType::Integer,
            initial: json!(0_i64),
        },
    );

    // 一个 code 节点，每次触发把 counter +1 并把当前值放到 payload.value
    let mut node_config = serde_json::Map::new();
    node_config.insert(
        "script".to_owned(),
        json!(r#"
            let v = vars.get("counter") + 1;
            vars.set("counter", v);
            payload.value = v;
            payload
        "#),
    );

    let node = WorkflowNodeDefinition::from_parts(
        "inc",
        "code",
        None,
        json!(node_config),
        None,
        32,
    );
    graph.nodes.insert("inc".to_owned(), node);

    let registry: NodeRegistry = standard_registry();
    let cm = Arc::new(ConnectionManager::new());
    let mut deployment = deploy_workflow_with_ai(graph, cm, None, &registry)
        .await
        .expect("含 code 节点的图应能部署");

    // 触发三次
    for _ in 0..3 {
        deployment
            .submit(WorkflowContext::with_payload(json!({ "value": 0 })))
            .await
            .expect("submit 应成功");
    }

    // 收三次 result，最后一次 value 应为 3
    let mut last_value: Option<serde_json::Value> = None;
    for _ in 0..3 {
        if let Some(ctx) = deployment.next_result().await {
            last_value = Some(ctx.payload);
        }
    }
    let final_payload = last_value.expect("应收到 3 次 result");
    assert_eq!(
        final_payload["value"],
        json!(3_i64),
        "三次累加后 counter 应为 3，实际：{final_payload}"
    );

    deployment.shutdown().await;
}
```

> 如果 `WorkflowNodeDefinition::from_parts` 不存在或签名不同，搜 `WorkflowNodeDefinition` 在 `crates/core/src/plugin.rs` 的 impl，按现有公开构造方式适配。最小路径是直接 `serde_json::from_value::<WorkflowNodeDefinition>(json!({...}))` 反序列化构造。

- [x] **Step 5: 测试通过**

```bash
cargo test --test variables 2>&1 | tail -20
```

- [x] **Step 6: Commit**

```bash
git add crates/nodes-flow/src/ tests/variables.rs
git commit -s -m "feat(nodes-flow): 5 个脚本节点工厂从 SharedResources 取 WorkflowVariables 并注入 Rhai（ADR-0012）"
```

---

## Task 8: IPC `snapshot_workflow_variables`

**Files:**
- Modify: `crates/tauri-bindings/src/lib.rs`
- Modify: `src-tauri/src/lib.rs`

- [x] **Step 1: 在 `tauri-bindings` 加请求 / 响应类型**

打开 `crates/tauri-bindings/src/lib.rs`，在已有 IPC 类型区域追加：

```rust
use nazh_core::TypedVariableSnapshot;

/// `snapshot_workflow_variables` 命令的请求。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-export", derive(TS), ts(export))]
pub struct SnapshotWorkflowVariablesRequest {
    pub workflow_id: String,
}

/// `snapshot_workflow_variables` 命令的响应。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-export", derive(TS), ts(export))]
pub struct SnapshotWorkflowVariablesResponse {
    pub variables: std::collections::HashMap<String, TypedVariableSnapshot>,
}
```

并在该 crate 的 `export_all` 函数加：

```rust
SnapshotWorkflowVariablesRequest::export()?;
SnapshotWorkflowVariablesResponse::export()?;
```

- [x] **Step 2: 实现 Tauri 命令**

打开 `src-tauri/src/lib.rs`，在已有命令注册块（`tauri::generate_handler![...]`）旁找到合适位置，加：

```rust
#[tauri::command]
async fn snapshot_workflow_variables(
    state: tauri::State<'_, AppState>,
    request: SnapshotWorkflowVariablesRequest,
) -> Result<SnapshotWorkflowVariablesResponse, String> {
    let runtime = state.runtime.read().await;
    let deployment = runtime
        .get(&request.workflow_id)
        .ok_or_else(|| format!("工作流 `{}` 未部署", request.workflow_id))?;

    let resources = deployment.resources();
    let vars = resources
        .get::<Arc<nazh_engine::WorkflowVariables>>()
        .ok_or_else(|| "部署中无 WorkflowVariables".to_owned())?;
    let snapshot: HashMap<_, _> = vars
        .snapshot()
        .into_iter()
        .map(|(k, v)| (k, v.into()))
        .collect();
    Ok(SnapshotWorkflowVariablesResponse {
        variables: snapshot,
    })
}
```

并在 `tauri::generate_handler![...]` 列表里加 `snapshot_workflow_variables`。

> **如果 `WorkflowDeployment::resources()` 不存在**：在 `src/graph/types.rs` 的 `WorkflowDeployment` 上加：
>
> ```rust
> impl WorkflowDeployment {
>     /// 返回部署时构造的资源句柄（含 `WorkflowVariables` 等），供 IPC 读取共享状态。
>     pub fn resources(&self) -> SharedResources {
>         // 需要在 deploy.rs 里把 resources 存进 deployment——见下面 Step 3
>         self.shared_resources.clone()
>     }
> }
> ```
>
> 并在 `WorkflowDeployment` 结构体加 `pub(crate) shared_resources: SharedResources` 字段，部署完成时填充。同样需在 `WorkflowDeploymentParts` 加。

- [x] **Step 3: 把 resources 持久到 WorkflowDeployment**

修改 `src/graph/types.rs`：

```rust
pub struct WorkflowDeployment {
    pub(crate) ingress: WorkflowIngress,
    pub(crate) streams: WorkflowStreams,
    pub(crate) lifecycle_guards: Vec<(String, LifecycleGuard)>,
    pub(crate) shutdown_token: CancellationToken,
    pub(crate) shared_resources: SharedResources,
}
```

`SharedResources` 来自 `nazh_core::SharedResources`——加到 import。

修改 `src/graph/deploy.rs` 的 `Ok(WorkflowDeployment { ... })` 末尾：

```rust
    Ok(WorkflowDeployment {
        ingress: WorkflowIngress { ... },
        streams: WorkflowStreams { ... },
        lifecycle_guards,
        shutdown_token,
        shared_resources: shared_resources.clone(),
    })
```

并在 `WorkflowDeployment` 加访问器：

```rust
impl WorkflowDeployment {
    pub fn resources(&self) -> &SharedResources {
        &self.shared_resources
    }
}
```

`WorkflowDeploymentParts` 同步加字段。

- [x] **Step 4: 写 IPC 集成测试（先不写——Tauri test harness 复杂，依靠 ts 端单测验证类型 + 手测命令)**

跳过——ts-rs 类型契约 + 手动验证 deploy + IPC 已足够 Phase 1。Phase 2 frontend 实施时会有真实使用。

- [x] **Step 5: ts-rs 导出 + 编译验证**

```bash
cargo test -p tauri-bindings --features ts-export export_bindings
cargo check --manifest-path src-tauri/Cargo.toml 2>&1 | tail -10
git diff web/src/generated/ | head -50
```

预期：`SnapshotWorkflowVariablesRequest.ts` / `SnapshotWorkflowVariablesResponse.ts` 出现，编译通过。

- [x] **Step 6: Commit**

```bash
git add crates/tauri-bindings/src/lib.rs src-tauri/src/lib.rs src/graph/types.rs src/graph/deploy.rs web/src/generated/
git commit -s -m "feat(ipc): snapshot_workflow_variables 命令 + WorkflowDeployment::resources 访问器（ADR-0012）"
```

---

## Task 9: 文档与 memory 同步

**Files:**
- Modify: `docs/adr/0012-工作流变量.md`
- Modify: `docs/adr/README.md`
- Modify: `AGENTS.md`（根）
- Modify: `crates/core/AGENTS.md`
- Modify: `crates/scripting/AGENTS.md`（如缺则补）
- Modify: `crates/nodes-flow/AGENTS.md`
- Modify: `~/.claude/projects/-home-zhihongniu-Nazh/memory/project_system_architecture.md`
- Modify: `~/.claude/projects/-home-zhihongniu-Nazh/memory/project_architecture_review_2026_04.md`

- [x] **Step 1: ADR-0012 状态 → 已实施 + Phase 1 落地记录**

修改 `docs/adr/0012-工作流变量.md` 顶部状态行：

```
- **状态**: 已实施（Phase 1，2026-04-27）
```

文末 `## 备注` 之后追加：

```markdown
### Phase 1 落地记录（2026-04-27）

**已落地范围：**
- Ring 0 类型 `WorkflowVariables` / `TypedVariable` / `TypedVariableSnapshot` / `VariableDeclaration` 落在 `crates/core/src/variables.rs`
- 类型校验复用 `PinType`：`pin_type_matches_value` 函数对 `set` / `compare_and_swap` / `from_declarations` 三处统一拦截
- `NodeLifecycleContext.variables: Arc<WorkflowVariables>` 字段落地，`on_deploy` 钩子可读写共享变量
- `WorkflowGraph.variables: HashMap<String, VariableDeclaration>` schema 字段（serde default = 空）
- 部署期阶段 0 初始化器 `src/graph/variables_init.rs::build_workflow_variables`，初值类型不匹配整图失败
- Rhai 注入：`crates/scripting/src/lib.rs::register_vars_helpers`，脚本可通过 `vars.get(name)` / `vars.set(name, value)` / `vars.cas(name, expected, new)` 读写
- `nodes-flow` 五节点（`if` / `switch` / `loop` / `tryCatch` / `code`）工厂迁移到从 `SharedResources` 取 `Arc<WorkflowVariables>`
- IPC `snapshot_workflow_variables(workflow_id)` 命令 + `WorkflowDeployment::resources()` 访问器

**实施期间的决策偏离 ADR 草稿：**
1. **`Custom` 类型变量 Phase 1 不接受写入**——`Custom` 语义需要"产生该类型的节点"对齐（与 ADR-0010 Phase 4 deferred Item 2 共享），声明侧也只允许 `Any` / 标量 / `Json` / `Binary` / `Array` 路径。
2. **持久化、变更事件广播、前端面板留 Phase 2**——独立 plan，依赖 Phase 1 的 IPC + 类型契约。
3. **`compare_and_swap` 是唯一附加并发原语**——`fetch_add` 等可由用户 CAS 循环组合，按需扩展。
4. **`TypedVariable` 内部类型用 `chrono::DateTime<Utc>`，IPC 表示用 `TypedVariableSnapshot.updated_at: String` (RFC3339)**——避免前端处理时区差异。

**Phase 2 候选项（独立 plan 启动）：**
- 前端变量面板（`web/src/components/RuntimeVariables/`）+ 实时刷新
- `VariableChanged` 事件 + `Window::emit("workflow://variable-changed")` 广播
- IPC `set_workflow_variable`（带类型校验，外部 / 运营人员手动改 setpoint）
```

- [x] **Step 2: 更新 `docs/adr/README.md` 索引**

```diff
-| [0012](0012-工作流变量.md) | 工作流级共享变量（`WorkflowVariables`） | 提议中 | 2026-04-24 |
+| [0012](0012-工作流变量.md) | 工作流级共享变量（`WorkflowVariables`） | 已实施 | 2026-04-27 |
```

- [x] **Step 3: 更新根 `AGENTS.md`**

在 `## Project Status` 下 "Current batch of ADRs" 列表加：

```markdown
- ADR-0012 (工作流变量) — **已实施 Phase 1**（2026-04-27，`crates/core/src/variables.rs` + `src/graph/variables_init.rs` + Rhai `vars.get/set/cas`；前端面板 + 变更事件留 Phase 2）
```

ADR Execution Order 表第 5 项打钩：

```diff
-> 5. **ADR-0012** 工作流变量（依赖 0009 + 0010）
+> 5. ✅ **ADR-0012** 工作流变量 — Phase 1 已实施（2026-04-27）；Phase 2（前端面板 + 变更事件）独立 plan
```

- [x] **Step 4: 更新 `crates/core/AGENTS.md`**

在该文件的"模块表"里加 `variables.rs`，并在"对外暴露"区加 `WorkflowVariables` / `VariableDeclaration` / `TypedVariable` / `TypedVariableSnapshot` / `pin_type_matches_value`。

- [x] **Step 5: 更新 `crates/scripting/AGENTS.md`**

如文件不存在则按其他 crate 模板创建（最简：crate 用途 / 对外暴露 / 内部约定 / 修改本 crate 时 checklist）。在"内部约定"区加：

```markdown
## Rhai 全局对象

`ScriptNodeBase::new` 接受可选 `Arc<WorkflowVariables>`：
- 注入时脚本可访问 `vars.get(name)` / `vars.set(name, value)` / `vars.cas(name, expected, new) -> bool`
- 未注入时调用 `vars.*` 抛 ErrorRuntime（带 node_id 与提示）
- 类型校验在 Rust 侧 `WorkflowVariables` 拦截，错误转 ErrorRuntime 上抛
```

- [x] **Step 6: 更新 `crates/nodes-flow/AGENTS.md`**

在节点 inventory 表加一列 "vars 接入" 或在文末加：

```markdown
## 工作流变量集成（ADR-0012）

5 个脚本节点（`if` / `switch` / `loop` / `tryCatch` / `code`）的工厂从
`SharedResources` 取 `Arc<WorkflowVariables>` 并注入到 `ScriptNodeBase::new` 的
`variables` 参数。脚本里可用 `vars.get/set/cas` 读写工作流声明的变量。
```

- [x] **Step 7: 更新 memory 文件**

`project_system_architecture.md` 的 Implementation Progress 区加：

```markdown
- **ADR-0012 (工作流变量) ✅ Phase 1** (2026-04-27): Ring 0 `WorkflowVariables` + 类型化 set/CAS（复用 `PinType`）；`NodeLifecycleContext.variables` 字段；`WorkflowGraph.variables` schema 字段；Rhai `vars.get/set/cas`；nodes-flow 五节点工厂迁移；IPC `snapshot_workflow_variables`。Phase 2（前端面板 + 变更事件广播）独立 plan。
```

NodeTrait Signature 区下面 `NodeLifecycleContext` 字段说明（如有）补上 `variables`。

`project_architecture_review_2026_04.md` 的提案-05 行改：

```diff
-| 提案-05 | 工作流变量 | ADR-0012 | 📝 提议中 → **下一候选**（依赖 0009 + 0010 已就绪） |
+| 提案-05 | 工作流变量 | ADR-0012 | ✅ Phase 1 已实施（2026-04-27）|
```

`MEMORY.md` 的索引行更新：

```diff
-- [System Architecture]... 已实施 ADR-0008/0009/0010/0011/0017/0018/0019。... 下一候选：ADR-0012 工作流变量（依赖已就绪）。
+- [System Architecture]... 已实施 ADR-0008/0009/0010/0011/0012(P1)/0017/0018/0019。... 下一候选：ADR-0013 子图与宏（依赖 0010 ✅），或 Phase 6 EventBus。
```

- [x] **Step 8: 全面回归 + 文档校对**

```bash
cargo test --workspace 2>&1 | tail -20
cargo clippy --workspace --all-targets -- -D warnings 2>&1 | tail -20
cargo fmt --all -- --check
cargo test -p tauri-bindings --features ts-export export_bindings
```

预期：全部绿灯。如有 clippy 警告，按现有抑制约定（`#[allow(...)]` + 注释解释原因）处理。

- [x] **Step 9: Commit**

```bash
git add docs/ AGENTS.md CLAUDE.md crates/core/AGENTS.md crates/scripting/AGENTS.md crates/nodes-flow/AGENTS.md
git add /home/zhihongniu/.claude/projects/-home-zhihongniu-Nazh/memory/
git commit -s -m "docs(adr-0012): Phase 1 落地后状态同步 + AGENTS / memory 更新"
```

---

## 自我审查 / Self-Review Checklist

实施完成后跑一遍这个清单（每项打勾）：

- [x] `cargo test --workspace` 全绿
- [x] `cargo clippy --workspace --all-targets -- -D warnings` 全绿
- [x] `cargo fmt --all -- --check` 无 diff
- [x] `cargo test -p tauri-bindings --features ts-export export_bindings` 通过；`web/src/generated/` 含新类型
- [x] `cargo check --manifest-path src-tauri/Cargo.toml` 编译通过
- [x] `tests/variables.rs` 三个端到端测试都通过
- [x] `crates/core/src/variables.rs` 9 个单元测试通过
- [x] `crates/scripting/src/lib.rs::variables_tests` 4 个测试通过
- [x] ADR-0012 状态字段更新为「已实施（Phase 1，2026-04-27）」
- [x] `docs/adr/README.md` 索引行更新
- [x] 根 `AGENTS.md` Project Status + ADR Execution Order 表都更新
- [x] `crates/core/AGENTS.md` 模块表含 `variables.rs`
- [x] `crates/scripting/AGENTS.md` 含 vars Rhai 注册说明
- [x] memory 两份文件状态同步
- [x] 至少一次手动跑 desktop dev 模式 (`cd src-tauri && ../web/node_modules/.bin/tauri dev --no-watch`) 验证现有 UI 无回归（变量功能没 UI，但要确认不破坏现有功能）

---

## 提交边界与 PR 形态

本 plan 自然分为 **9 个 commit**（每个 Task 一个），单一 PR 推荐这样组织：

1. `feat(core): WorkflowVariables 三类型 + 类型化 set / CAS（ADR-0012 Phase 1）`
2. `feat(graph): WorkflowGraph 加 variables 字段`
3. `feat(core): NodeLifecycleContext 加 variables 字段`
4. `feat(graph): build_workflow_variables 部署期初始化器`
5. `feat(graph): deploy_workflow 注入 WorkflowVariables`
6. `feat(scripting): Rhai 注册 vars.get / vars.set / vars.cas`
7. `feat(nodes-flow): 5 个脚本节点工厂迁移到 vars`
8. `feat(ipc): snapshot_workflow_variables 命令`
9. `docs(adr-0012): Phase 1 状态同步`

按"一 PR 多 commit"约定（CLAUDE.md → Git → "One concern per commit"），每个 commit 自包含且测试通过。
