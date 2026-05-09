//! 工作流级共享可变变量（ADR-0012）。
//!
//! ## 设计要点
//!
//! - 类型系统**直接复用 [`PinType`](crate::PinType)**——不引入第二套词汇表。
//! - 后端是 [`DashMap`]——单 key 读写无锁、跨 key 高并发。
//! - 写入前**强制类型校验**：`set` / `compare_and_swap` 拒绝写与声明类型不匹配的值。
//! - 提供 [`compare_and_swap`](WorkflowVariables::compare_and_swap) 做原子递增。
//! - **生命周期与部署同步**：`Arc<WorkflowVariables>` 由 `build_workflow_variables`
//!   构造（在 `src/graph/variables_init.rs`），注入 `NodeLifecycleContext` +
//!   `SharedResources`，部署撤销时随 Drop 释放。
//! - **写即变更事件（Phase 2）**：通过 [`WorkflowVariables::set_event_sender`] 注入独立
//!   事件通道后，`set` / `compare_and_swap` 检测到值变化时 `try_send` 一条
//!   [`WorkflowVariableEvent::Changed`]。
//!   值未变化时不发事件，避免轮询脚本制造事件刷屏。
//!   变量事件走独立通道（[`WorkflowVariableEvent`]），不混入执行可观测事件
//!   [`ExecutionEvent`](crate::ExecutionEvent)（B1-R0-01/B1-R0-05 关注点分离）。
//!
//! ## 不包含（Phase 1 范围外）
//!
//! - 持久化（进程退出即清零）。

use std::collections::HashMap;
use std::hash::BuildHasher;

use chrono::{DateTime, Utc};
use dashmap::DashMap;
use serde_json::Value;
use tokio::sync::{OnceCell, mpsc, watch};

use crate::EngineError;

mod events;
mod types;
mod value;

pub use events::{WorkflowVariableEvent, emit_variable_event};
pub use types::{TypedVariable, TypedVariableSnapshot, VariableDeclaration};
pub use value::pin_type_matches_value;

use events::EventSink;
use value::json_value_label;

/// 工作流级共享变量存储。
///
/// 由 `build_workflow_variables`（`src/graph/variables_init.rs`）在部署期构造、
/// 包成 `Arc<WorkflowVariables>` 注入 `NodeLifecycleContext` 与 `SharedResources`。
/// 撤销工作流时随 `Arc` 引用计数归零自然释放。
#[derive(Debug)]
pub struct WorkflowVariables {
    inner: DashMap<String, TypedVariable>,
    /// ADR-0012 Phase 2：变量事件发送通道（注入一次）。
    /// 未注入时 `set` / `compare_and_swap` 仍正常工作但不发事件。
    event_sink: OnceCell<EventSink>,
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
            event_sink: OnceCell::new(),
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
                TypedVariable::new(
                    declaration.initial.clone(),
                    declaration.variable_type.clone(),
                    declaration.initial.clone(),
                    Utc::now(),
                    None,
                ),
            );
        }
        Ok(Self {
            inner,
            event_sink: OnceCell::new(),
        })
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

    /// 订阅指定变量的变更通知。返回的 [`watch::Receiver`] 在 `set()` /
    /// `compare_and_swap()` 写入新值时唤醒。
    ///
    /// 变量不存在时返回 `None`。
    #[must_use]
    #[allow(clippy::type_complexity)]
    pub fn subscribe(&self, name: &str) -> Option<watch::Receiver<Option<(DateTime<Utc>, Value)>>> {
        self.inner.get(name).map(|entry| entry.value().subscribe())
    }

    /// 注入变量事件通道。仅可调用一次；重复调用通过 `tracing::warn!` 报告并忽略。
    ///
    /// 设计为 `&self`（非 `&mut`）以便在 `Arc<WorkflowVariables>` 构造完成后注入——
    /// deploy 流程是 `let vars = Arc::new(build_workflow_variables(...)?);` 后再
    /// `vars.set_event_sender(workflow_id, var_event_tx)`。
    ///
    /// 调用方（如 deploy.rs）必须在节点 `on_deploy` 启动之前完成此注入；否则
    /// 节点在注入间隙的写入会因 `event_sink == None` 而漏发事件。
    pub fn set_event_sender(
        &self,
        workflow_id: String,
        sender: mpsc::Sender<WorkflowVariableEvent>,
    ) {
        if self
            .event_sink
            .set(EventSink::new(workflow_id, sender))
            .is_err()
        {
            tracing::warn!("WorkflowVariables event_sink 重复注入，已忽略");
        }
    }

    /// 内部 helper：在 entry 借用已 drop 之后，按 `event_payload` 与 `event_sink` 决定是否 emit
    /// [`WorkflowVariableEvent::Changed`]。`name` 仅用于事件构造与失败日志的上下文。
    ///
    /// 调用约定：调用方必须**已经** drop 了 `DashMap` 的 `&mut Entry` 借用——本函数不做
    /// 锁守卫，假设 caller 已让 shard 锁释放。
    fn try_emit_changed(&self, name: &str, event_payload: Option<(Value, String, Option<String>)>) {
        let Some((value, updated_at, updated_by)) = event_payload else {
            return;
        };
        let Some(sink) = self.event_sink.get() else {
            return;
        };
        let event = WorkflowVariableEvent::Changed {
            workflow_id: sink.workflow_id.clone(),
            name: name.to_owned(),
            value,
            updated_at,
            updated_by,
        };
        sink.emit(name, "changed", event);
    }

    /// 类型化写入。`updated_by` 一般是节点 id；为 `None` 表示外部接入（IPC、初始化）。
    ///
    /// 值变化时（`entry.value != value`）向已注入的事件通道发送
    /// [`WorkflowVariableEvent::Changed`]；
    /// 值未变化或未注入通道时静默跳过。
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
        let value_changed = entry.value != value;
        entry.value = value;
        entry.updated_at = Utc::now();
        entry.updated_by = updated_by.map(str::to_owned);

        let event_payload = if value_changed {
            Some((
                entry.value.clone(),
                entry.updated_at.to_rfc3339(),
                entry.updated_by.clone(),
            ))
        } else {
            None
        };
        if value_changed {
            let _ = entry
                .watch_tx
                .send(Some((entry.updated_at, entry.value.clone())));
        }
        drop(entry);

        self.try_emit_changed(name, event_payload);

        Ok(())
    }

    /// 将变量重置为声明初值。语义上等价于 `set(name, initial, updated_by)`。
    ///
    /// # Errors
    ///
    /// - `UnknownVariable` — 变量未声明。
    pub fn reset(&self, name: &str, updated_by: Option<&str>) -> Result<(), EngineError> {
        let initial = self
            .inner
            .get(name)
            .map(|entry| entry.value().initial.clone())
            .ok_or_else(|| EngineError::unknown_variable(name))?;
        self.set(name, initial, updated_by)
    }

    /// 原子比较交换：当前值与 `expected` 相等时写入 `new`。
    ///
    /// 返回 `true` 表示交换成功，`false` 表示当前值不匹配（保持不变）。
    /// 类型不匹配仍返回 `Err`——CAS 不绕过类型校验。
    /// 交换成功且值变化时向已注入的事件通道发送
    /// [`WorkflowVariableEvent::Changed`]。
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
        let value_changed = entry.value != new;
        entry.value = new;
        entry.updated_at = Utc::now();
        entry.updated_by = updated_by.map(str::to_owned);

        let event_payload = if value_changed {
            Some((
                entry.value.clone(),
                entry.updated_at.to_rfc3339(),
                entry.updated_by.clone(),
            ))
        } else {
            None
        };
        if value_changed {
            let _ = entry
                .watch_tx
                .send(Some((entry.updated_at, entry.value.clone())));
        }
        drop(entry);

        self.try_emit_changed(name, event_payload);

        Ok(true)
    }

    /// 移除指定变量并返回其先前值（ADR-0012 Phase 3）。
    ///
    /// 变量不存在时返回 `None`（不发事件、不报错）。
    /// 移除后 `watch_tx` 发送 `None` 通知订阅者变量已消失；
    /// 同时向已注入的事件通道发送 [`WorkflowVariableEvent::Deleted`]。
    #[must_use]
    pub fn remove(&self, name: &str) -> Option<TypedVariable> {
        let (_, removed) = self.inner.remove(name)?;
        let _ = removed.watch_tx.send(None);
        let sink = self.event_sink.get();
        if let Some(sink) = sink {
            let event = WorkflowVariableEvent::Deleted {
                workflow_id: sink.workflow_id.clone(),
                name: name.to_owned(),
            };
            sink.emit(name, "deleted", event);
        }
        Some(removed)
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

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
#[path = "tests/mod.rs"]
mod tests;
