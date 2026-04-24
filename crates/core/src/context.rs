//! 在工作流 DAG 中流转的数据信封。
//!
//! [`WorkflowContext`] 是 IPC 边界上的完整载体（含 payload），
//! [`ContextRef`] 是引擎内部通道中传递的轻量引用（~64 字节），
//! 实际数据存储在 [`DataStore`](crate::DataStore) 中。

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
#[cfg(feature = "ts-export")]
use ts_rs::TS;
use uuid::Uuid;

use crate::data::DataId;

/// 通过 Tokio MPSC 通道在节点之间传递的不可变数据载体。
///
/// 每个上下文携带唯一的 `trace_id` 用于全链路追踪，
/// `timestamp` 在每次变换时刷新，`payload` 承载动态 JSON 数据。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[cfg_attr(feature = "ts-export", derive(TS), ts(export))]
pub struct WorkflowContext {
    pub trace_id: Uuid,
    pub timestamp: DateTime<Utc>,
    pub payload: Value,
}

impl WorkflowContext {
    /// 创建新上下文，自动生成 trace ID 并记录当前时间戳。
    pub fn new(payload: Value) -> Self {
        Self {
            trace_id: Uuid::new_v4(),
            timestamp: Utc::now(),
            payload,
        }
    }

    /// 从已有部件重建上下文，保留原始 trace ID。
    pub fn from_parts(trace_id: Uuid, timestamp: DateTime<Utc>, payload: Value) -> Self {
        Self {
            trace_id,
            timestamp,
            payload,
        }
    }

    /// 替换 payload 并刷新时间戳，消费 `self`。
    #[must_use]
    pub fn with_payload(mut self, payload: Value) -> Self {
        self.payload = payload;
        self.timestamp = Utc::now();
        self
    }

    /// 仅刷新时间戳，不改变 payload。
    #[must_use]
    pub fn touch(mut self) -> Self {
        self.timestamp = Utc::now();
        self
    }
}

/// 引擎内部通道中传递的轻量引用。
///
/// 仅含追踪信息和数据面 ID（~64 字节），实际 payload 存储在
/// [`DataStore`](crate::DataStore) 中。在 DAG 扇出时，多个下游通道
/// 共享同一个 `data_id`，实现零拷贝。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContextRef {
    /// 全链路追踪标识。
    pub trace_id: Uuid,
    /// 数据创建/变换时间戳。
    pub timestamp: DateTime<Utc>,
    /// 指向 `DataStore` 中实际数据的标识。
    pub data_id: DataId,
    /// 产出此数据的源节点 ID（根节点入口为 None）。
    pub source_node: Option<String>,
}

impl ContextRef {
    /// 从 `trace_id` 和 `data_id` 创建新引用。
    #[must_use]
    pub fn new(trace_id: Uuid, data_id: DataId, source_node: Option<String>) -> Self {
        Self {
            trace_id,
            timestamp: Utc::now(),
            data_id,
            source_node,
        }
    }

    /// 从已有 `WorkflowContext` 和 `DataId` 构造引用。
    #[must_use]
    pub fn from_context(ctx: &WorkflowContext, data_id: DataId) -> Self {
        Self {
            trace_id: ctx.trace_id,
            timestamp: ctx.timestamp,
            data_id,
            source_node: None,
        }
    }
}
