//! per-deployment 审批注册表。

use chrono::{DateTime, Utc};
use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use tokio::sync::oneshot;
use uuid::Uuid;

use nazh_core::EngineError;

use super::form::FormSchemaField;

/// 审批 ID = UUID。
pub type ApprovalId = Uuid;

/// 人工响应动作。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub enum ResponseAction {
    Approved,
    Rejected,
}

/// 人工响应。
#[derive(Debug)]
pub struct HumanLoopResponse {
    pub action: ResponseAction,
    pub form_data: serde_json::Value,
    pub comment: Option<String>,
    pub responded_by: Option<String>,
}

/// 审批槽——存储在 `DashMap` 中，IPC 命令通过 ID 查找。
pub struct ApprovalSlot {
    pub workflow_id: String,
    pub node_id: String,
    pub node_label: String,
    pub form_schema: Vec<FormSchemaField>,
    pub pending_since: DateTime<Utc>,
    pub approval_timeout_ms: Option<u64>,
    pub default_action: super::config::DefaultAction,
    /// oneshot sender——调用 `send()` 唤醒阻塞的 `transform()`。
    pub responder: oneshot::Sender<HumanLoopResponse>,
}

/// Pending 审批摘要（IPC 列表用）。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PendingApprovalSummary {
    pub approval_id: String,
    pub workflow_id: String,
    pub node_id: String,
    pub node_label: String,
    pub pending_since: String,
    pub timeout_ms: Option<u64>,
    pub form_schema: serde_json::Value,
}

/// per-deployment 审批注册表。
pub struct ApprovalRegistry {
    slots: DashMap<ApprovalId, ApprovalSlot>,
}

impl ApprovalRegistry {
    pub fn new() -> Self {
        Self {
            slots: DashMap::new(),
        }
    }

    /// 注册 pending 审批，返回 `approval_id` + Receiver。
    pub fn create_slot(
        &self,
        slot: ApprovalSlot,
    ) -> (ApprovalId, oneshot::Receiver<HumanLoopResponse>) {
        let approval_id = Uuid::new_v4();
        let (tx, rx) = oneshot::channel();
        // 逐字段拆解传入 slot，丢弃其 dummy responder，使用真正的 oneshot pair
        let real_slot = ApprovalSlot {
            workflow_id: slot.workflow_id,
            node_id: slot.node_id,
            node_label: slot.node_label,
            form_schema: slot.form_schema,
            pending_since: slot.pending_since,
            approval_timeout_ms: slot.approval_timeout_ms,
            default_action: slot.default_action,
            responder: tx,
        };
        self.slots.insert(approval_id, real_slot);
        (approval_id, rx)
    }

    /// 人工响应——IPC 命令调用。
    pub fn respond(
        &self,
        approval_id: ApprovalId,
        response: HumanLoopResponse,
    ) -> Result<(), EngineError> {
        let (_, slot) = self.slots.remove(&approval_id).ok_or_else(|| {
            EngineError::invalid_graph(format!("审批 `{approval_id}` 不存在或已响应"))
        })?;
        slot.responder
            .send(response)
            .map_err(|_| EngineError::invalid_graph("审批响应发送失败（receiver 已被 drop）"))?;
        Ok(())
    }

    /// 清理 workflow 的所有 pending 审批（undeploy 时调用）。
    /// 移除 matching slots 后 sender 被 drop，receiver.await 返回 Err。
    pub fn cleanup_workflow(&self, workflow_id: &str) {
        self.slots.retain(|_, slot| slot.workflow_id != workflow_id);
    }

    /// 列出 pending 审批（IPC 命令用）。
    pub fn list_pending(&self, workflow_id: Option<&str>) -> Vec<PendingApprovalSummary> {
        self.slots
            .iter()
            .filter(|entry| workflow_id.is_none_or(|wid| entry.value().workflow_id == wid))
            .map(|entry| {
                let id = entry.key();
                let slot = entry.value();
                PendingApprovalSummary {
                    approval_id: id.to_string(),
                    workflow_id: slot.workflow_id.clone(),
                    node_id: slot.node_id.clone(),
                    node_label: slot.node_label.clone(),
                    pending_since: slot.pending_since.to_rfc3339(),
                    timeout_ms: slot.approval_timeout_ms,
                    form_schema: serde_json::to_value(&slot.form_schema)
                        .unwrap_or(serde_json::Value::Null),
                }
            })
            .collect()
    }
}

impl Default for ApprovalRegistry {
    fn default() -> Self {
        Self::new()
    }
}
