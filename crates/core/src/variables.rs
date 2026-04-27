//! 工作流级共享可变变量（ADR-0012）。
//!
//! ## 设计要点
//!
//! - 类型系统**直接复用 [`PinType`]**——不引入第二套词汇表。
//! - 后端是 [`DashMap`]——单 key 读写无锁、跨 key 高并发。
//! - 写入前**强制类型校验**：`set` / `compare_and_swap` 拒绝写与声明类型不匹配的值。
//! - 提供 [`compare_and_swap`](WorkflowVariables::compare_and_swap) 做原子递增。
//! - **生命周期与部署同步**：`Arc<WorkflowVariables>` 由 `build_workflow_variables`
//!   构造（在 `src/graph/variables_init.rs`），注入 `NodeLifecycleContext` +
//!   `SharedResources`，部署撤销时随 Drop 释放。
//!
//! ## 不包含（Phase 1 范围外）
//!
//! - 持久化（进程退出即清零）。
//! - 变量变更事件广播（Phase 2 与前端面板一并做）。

use std::collections::HashMap;
use std::hash::BuildHasher;

use chrono::{DateTime, Utc};
use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use serde_json::Value;
#[cfg(feature = "ts-export")]
use ts_rs::TS;

use crate::{EngineError, PinType};

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
#[serde(rename_all = "camelCase")]
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
#[derive(Debug)]
pub struct WorkflowVariables {
    inner: DashMap<String, TypedVariable>,
}

impl WorkflowVariables {
    /// 构造一个无任何声明的空变量集。
    ///
    /// 用于尚未启用变量声明的工作流——任意 `set` / `compare_and_swap` 都会因
    /// `UnknownVariable` 错误立即失败，对节点透明。
    #[must_use]
    pub fn empty() -> Self {
        Self {
            inner: DashMap::new(),
        }
    }

    /// 从 `WorkflowGraph.variables` 声明集构造。
    ///
    /// 每个声明的 `initial` 必须匹配其 `variable_type`；任一不匹配立即返回错误，
    /// 整个部署在阶段 0 失败（早失败原则）。
    ///
    /// # Errors
    ///
    /// `VariableInitialMismatch` — 任一声明的初值类型与声明类型不匹配。
    pub fn from_declarations<S: BuildHasher>(
        declarations: &HashMap<String, VariableDeclaration, S>,
    ) -> Result<Self, EngineError> {
        let inner = DashMap::with_capacity(declarations.len());
        for (name, declaration) in declarations {
            if !pin_type_matches_value(&declaration.variable_type, &declaration.initial) {
                return Err(EngineError::variable_initial_mismatch(
                    name.clone(),
                    declaration.variable_type.to_string(),
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
        self.inner
            .get(name)
            .map(|entry| entry.value().value.clone())
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
                entry.variable_type.to_string(),
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
                entry.variable_type.to_string(),
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
/// **`Custom` 在 Phase 1 完全拒绝**——既不能在 `from_declarations` 通过初值校验，
/// 也不能在 `set` / `compare_and_swap` 写入。命名类型语义需要"产出 Custom 输出
/// 的节点对齐"（参见 ADR-0010 Phase 4 deferred Item 2），变量与节点的 `Custom`
/// 引入要同步而非分头开启；触发条件就绪后将由专门 ADR 升级。
#[must_use]
pub fn pin_type_matches_value(pin_type: &PinType, value: &Value) -> bool {
    match (pin_type, value) {
        // Any 接受一切；Bool/Float/String/Json/Binary(base64字符串) 形态匹配
        (PinType::Any, _)
        | (PinType::Bool, Value::Bool(_))
        | (PinType::Float, Value::Number(_)) // i64/u64/f64 都接受
        | (PinType::String | PinType::Binary, Value::String(_)) // String 精确 / Binary base64假定
        | (PinType::Json, Value::Object(_) | Value::Array(_)) => true,

        (PinType::Integer, Value::Number(n)) => n.is_i64() || n.is_u64(),

        // Binary 字节数组：每个元素必须在 u8 范围内
        (PinType::Binary, Value::Array(arr)) => {
            arr.iter().all(|v| v.as_u64().is_some_and(|n| u8::try_from(n).is_ok()))
        }

        // 同质数组：递归校验每个元素
        (PinType::Array { inner }, Value::Array(arr)) => {
            arr.iter().all(|item| pin_type_matches_value(inner, item))
        }

        // Phase 1: Custom 完全拒绝（声明初值与运行时写入皆然），见函数级 doc
        _ => false,
    }
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

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use std::sync::Arc;

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
            .compare_and_swap("counter", &Value::from(99_i64), Value::from(1_i64), None)
            .unwrap();
        assert!(!ok);
        assert_eq!(vars.get("counter").unwrap().value, Value::from(0_i64));
    }

    #[test]
    fn cas_类型不匹配时返回_err() {
        let vars = vars_with("counter", PinType::Integer, Value::from(0_i64));
        let err = vars
            .compare_and_swap("counter", &Value::from(0_i64), Value::from("oops"), None)
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
    fn empty_构造器写入任意键都报_unknownvariable() {
        let vars = WorkflowVariables::empty();
        let err = vars.set("any-key", Value::from(1_i64), None).unwrap_err();
        assert!(
            matches!(err, EngineError::UnknownVariable { ref name } if name == "any-key"),
            "empty() 构造器写入任意键应返回 UnknownVariable，实际：{err}"
        );
        let cas_err = vars
            .compare_and_swap("any-key", &Value::from(0_i64), Value::from(1_i64), None)
            .unwrap_err();
        assert!(matches!(cas_err, EngineError::UnknownVariable { .. }));
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
