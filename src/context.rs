//! 在工作流 DAG 中流转的数据信封。

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

/// 通过 Tokio MPSC 通道在节点之间传递的不可变数据载体。
///
/// 每个上下文携带唯一的 `trace_id` 用于全链路追踪，
/// `timestamp` 在每次变换时刷新，`payload` 承载动态 JSON 数据。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
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
