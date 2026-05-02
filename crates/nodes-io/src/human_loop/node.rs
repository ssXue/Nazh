//! HITL 审批节点：暂停工作流等待人工响应。

use std::sync::Arc;

use async_trait::async_trait;
use chrono::Utc;
use serde_json::{Map, Value, json};
use tokio::time::Duration;
use uuid::Uuid;

use nazh_core::{
    EmptyPolicy, EngineError, NodeExecution, NodeTrait, PinDefinition, PinDirection, PinKind,
    PinType, impl_node_meta,
};

use super::config::{DefaultAction, HumanLoopNodeConfig};
use super::registry::{ApprovalRegistry, ApprovalSlot, HumanLoopResponse, ResponseAction};

/// 审批节点：阻塞 transform 等待人工审批响应，超时走 `default_action`。
pub struct HumanLoopNode {
    id: String,
    config: HumanLoopNodeConfig,
    approval_registry: Arc<ApprovalRegistry>,
    workflow_id: String,
}

impl HumanLoopNode {
    pub fn new(
        id: impl Into<String>,
        config: HumanLoopNodeConfig,
        approval_registry: Arc<ApprovalRegistry>,
        workflow_id: impl Into<String>,
    ) -> Self {
        Self {
            id: id.into(),
            config,
            approval_registry,
            workflow_id: workflow_id.into(),
        }
    }
}

#[async_trait]
impl NodeTrait for HumanLoopNode {
    impl_node_meta!("humanLoop");

    fn output_pins(&self) -> Vec<PinDefinition> {
        vec![
            PinDefinition {
                id: "approve".to_owned(),
                label: "通过".to_owned(),
                pin_type: PinType::Any,
                direction: PinDirection::Output,
                required: false,
                kind: PinKind::Exec,
                description: Some("人工审批通过时路由到此分支".to_owned()),
                empty_policy: EmptyPolicy::default(),
                block_timeout_ms: None,
                ttl_ms: None,
            },
            PinDefinition {
                id: "reject".to_owned(),
                label: "拒绝".to_owned(),
                pin_type: PinType::Any,
                direction: PinDirection::Output,
                required: false,
                kind: PinKind::Exec,
                description: Some("人工审批拒绝时路由到此分支".to_owned()),
                empty_policy: EmptyPolicy::default(),
                block_timeout_ms: None,
                ttl_ms: None,
            },
        ]
    }

    async fn transform(
        &self,
        _trace_id: Uuid,
        payload: Value,
    ) -> Result<NodeExecution, EngineError> {
        let (dummy_tx, _dummy_rx) = tokio::sync::oneshot::channel();
        let slot = ApprovalSlot {
            workflow_id: self.workflow_id.clone(),
            node_id: self.id.clone(),
            node_label: self.config.title.clone().unwrap_or_else(|| self.id.clone()),
            form_schema: self.config.form_schema.clone(),
            pending_since: Utc::now(),
            approval_timeout_ms: self.config.approval_timeout_ms,
            default_action: self.config.default_action.clone(),
            responder: dummy_tx,
        };

        let (approval_id, rx) = self.approval_registry.create_slot(slot);

        tracing::info!(
            node_id = %self.id,
            approval_id = %approval_id,
            "审批节点阻塞，等待人工响应"
        );

        let result: Result<
            Result<HumanLoopResponse, tokio::sync::oneshot::error::RecvError>,
            tokio::time::error::Elapsed,
        > = if let Some(timeout_ms) = self.config.approval_timeout_ms {
            tokio::time::timeout(Duration::from_millis(timeout_ms), rx).await
        } else {
            Ok(rx.await)
        };

        match result {
            Ok(Ok(response)) => {
                tracing::info!(
                    node_id = %self.id,
                    approval_id = %approval_id,
                    action = ?response.action,
                    "审批收到响应"
                );
                Self::handle_response(approval_id, payload, &response)
            }
            Ok(Err(_)) => {
                tracing::warn!(node_id = %self.id, "审批 responder 已关闭，执行默认动作");
                self.handle_timeout(approval_id, payload)
            }
            Err(_) => {
                tracing::info!(node_id = %self.id, "审批超时，执行默认动作");
                self.handle_timeout(approval_id, payload)
            }
        }
    }
}

impl HumanLoopNode {
    #[allow(clippy::unnecessary_wraps)]
    fn handle_response(
        approval_id: Uuid,
        payload: Value,
        response: &HumanLoopResponse,
    ) -> Result<NodeExecution, EngineError> {
        let meta = Self::metadata(approval_id, response);
        let route_target = match response.action {
            ResponseAction::Approved => "approve",
            ResponseAction::Rejected => "reject",
        };
        Ok(NodeExecution::route(payload, [route_target]).with_metadata(meta))
    }

    fn handle_timeout(
        &self,
        approval_id: Uuid,
        payload: Value,
    ) -> Result<NodeExecution, EngineError> {
        match &self.config.default_action {
            DefaultAction::AutoApprove => {
                let mut form_defaults = Map::new();
                for field in &self.config.form_schema {
                    if let Some((name, default)) = field.default_value() {
                        form_defaults.insert(name, default);
                    }
                }
                let meta = Self::timeout_metadata(approval_id, "autoApprove", Some(Value::Object(form_defaults)));
                Ok(NodeExecution::route(payload, ["approve"]).with_metadata(meta))
            }
            DefaultAction::AutoReject => Err(EngineError::invalid_graph(format!(
                "审批 `{approval_id}` 超时，默认拒绝"
            ))),
            DefaultAction::FallbackNode(target) => {
                tracing::warn!(
                    node_id = %self.id,
                    fallback = %target,
                    "FallbackNode 尚未实现，按拒绝处理"
                );
                Err(EngineError::invalid_graph(format!(
                    "审批 `{approval_id}` 超时，fallback 到 `{target}` 尚未实现"
                )))
            }
        }
    }

    fn metadata(approval_id: Uuid, response: &HumanLoopResponse) -> Map<String, Value> {
        let mut m = Map::new();
        m.insert("approval_id".to_owned(), json!(approval_id.to_string()));
        m.insert(
            "action".to_owned(),
            json!(match response.action {
                ResponseAction::Approved => "approved",
                ResponseAction::Rejected => "rejected",
            }),
        );
        m.insert("form_data".to_owned(), response.form_data.clone());
        if let Some(ref comment) = response.comment {
            m.insert("comment".to_owned(), json!(comment));
        }
        if let Some(ref by) = response.responded_by {
            m.insert("responded_by".to_owned(), json!(by));
        }
        Map::from_iter([("human_loop".to_owned(), Value::Object(m))])
    }

    fn timeout_metadata(approval_id: Uuid, action: &str, form_defaults: Option<Value>) -> Map<String, Value> {
        let mut m = Map::new();
        m.insert("approval_id".to_owned(), json!(approval_id.to_string()));
        m.insert("action".to_owned(), json!(action));
        m.insert("timed_out".to_owned(), json!(true));
        if let Some(defaults) = form_defaults {
            m.insert("form_defaults".to_owned(), defaults);
        }
        Map::from_iter([("human_loop".to_owned(), Value::Object(m))])
    }
}
