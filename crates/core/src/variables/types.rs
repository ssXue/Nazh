use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio::sync::watch;
#[cfg(feature = "ts-export")]
use ts_rs::TS;

use crate::PinType;

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
/// 内部表示——通过 [`WorkflowVariables::get`](super::WorkflowVariables::get) /
/// [`WorkflowVariables::snapshot`](super::WorkflowVariables::snapshot) 拷贝出来。
/// 不持有 `Arc<DashMap>` 引用。
pub struct TypedVariable {
    pub value: Value,
    pub variable_type: PinType,
    /// 部署时的声明初值，`reset()` 恢复到此值。
    pub initial: Value,
    pub updated_at: DateTime<Utc>,
    pub updated_by: Option<String>,
    /// 变更通知 channel。`set()` / `compare_and_swap()` 写入时发送 `(timestamp, value)`。
    pub(super) watch_tx: watch::Sender<Option<(DateTime<Utc>, Value)>>,
}

impl std::fmt::Debug for TypedVariable {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TypedVariable")
            .field("value", &self.value)
            .field("variable_type", &self.variable_type)
            .field("updated_at", &self.updated_at)
            .field("updated_by", &self.updated_by)
            .finish_non_exhaustive()
    }
}

impl Clone for TypedVariable {
    fn clone(&self) -> Self {
        Self {
            value: self.value.clone(),
            variable_type: self.variable_type.clone(),
            initial: self.initial.clone(),
            updated_at: self.updated_at,
            updated_by: self.updated_by.clone(),
            watch_tx: self.watch_tx.clone(),
        }
    }
}

impl TypedVariable {
    /// 构造一个带 watch channel 的活跃变量。
    pub(super) fn new(
        value: Value,
        variable_type: PinType,
        initial: Value,
        updated_at: DateTime<Utc>,
        updated_by: Option<String>,
    ) -> Self {
        let (watch_tx, _) = watch::channel(None);
        Self {
            value,
            variable_type,
            initial,
            updated_at,
            updated_by,
            watch_tx,
        }
    }

    /// 返回当前值的 watch receiver。`changed().await` 在值变更时唤醒。
    #[allow(clippy::type_complexity)]
    pub fn subscribe(&self) -> watch::Receiver<Option<(DateTime<Utc>, Value)>> {
        self.watch_tx.subscribe()
    }
}

/// IPC 序列化版变量快照（`updated_at` 用 RFC3339 字符串，避免前端处理时区差异）。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-export", derive(TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct TypedVariableSnapshot {
    pub value: Value,
    pub variable_type: PinType,
    /// 部署时的声明初值，前端用于"重置"按钮。
    pub initial: Value,
    /// RFC3339 时间戳。
    pub updated_at: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts-export", ts(optional))]
    pub updated_by: Option<String>,
}

impl From<TypedVariable> for TypedVariableSnapshot {
    fn from(var: TypedVariable) -> Self {
        Self {
            value: var.value,
            variable_type: var.variable_type,
            initial: var.initial,
            updated_at: var.updated_at.to_rfc3339(),
            updated_by: var.updated_by,
        }
    }
}
