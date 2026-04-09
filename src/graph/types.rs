//! 工作流图的数据结构定义与句柄方法。
//!
//! 本文件包含所有序列化/反序列化结构体、可观测事件枚举，
//! 以及已部署工作流的入口 ([`WorkflowIngress`])、流 ([`WorkflowStreams`])
//! 和组合句柄 ([`WorkflowDeployment`])。

use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio::sync::mpsc;
use crate::{EngineError, ExecutionEvent, WorkflowContext};

/// 从前端 AST 反序列化得到的顶层工作流定义。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowGraph {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub connections: Vec<crate::ConnectionDefinition>,
    #[serde(default)]
    pub nodes: HashMap<String, WorkflowNodeDefinition>,
    #[serde(default)]
    pub edges: Vec<WorkflowEdge>,
}

/// [`WorkflowGraph`] 中的单节点配置。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowNodeDefinition {
    #[serde(default)]
    pub id: String,
    #[serde(rename = "type", alias = "kind")]
    pub node_type: String,
    #[serde(default)]
    pub connection_id: Option<String>,
    #[serde(default)]
    pub config: Value,
    #[serde(default)]
    pub ai_description: Option<String>,
    #[serde(default)]
    pub timeout_ms: Option<u64>,
    #[serde(default = "default_node_buffer")]
    pub buffer: usize,
}

/// 工作流 DAG 中连接两个节点的有向边。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowEdge {
    #[serde(alias = "source")]
    pub from: String,
    #[serde(alias = "target")]
    pub to: String,
    #[serde(default, alias = "sourcePortID")]
    pub source_port_id: Option<String>,
    #[serde(default, alias = "targetPortID")]
    pub target_port_id: Option<String>,
}

/// 入口句柄，用于向已部署工作流的根节点提交数据。
#[derive(Clone)]
pub struct WorkflowIngress {
    pub(crate) root_nodes: Vec<String>,
    pub(crate) root_senders: HashMap<String, mpsc::Sender<WorkflowContext>>,
}

/// 已部署工作流的事件流和结果流接收端。
pub struct WorkflowStreams {
    pub(crate) event_rx: mpsc::Receiver<ExecutionEvent>,
    pub(crate) result_rx: mpsc::Receiver<WorkflowContext>,
}

/// 完整部署的工作流：入口用于提交数据，流用于观测结果。
pub struct WorkflowDeployment {
    pub(crate) ingress: WorkflowIngress,
    pub(crate) streams: WorkflowStreams,
}

/// 拓扑分析结果（仅模块内部使用）。
pub(crate) struct WorkflowTopology {
    pub(crate) root_nodes: Vec<String>,
    pub(crate) downstream: HashMap<String, Vec<WorkflowEdge>>,
}

/// 下游目标通道（仅模块内部使用）。
#[derive(Clone)]
pub(crate) struct DownstreamTarget {
    pub(crate) source_port_id: Option<String>,
    pub(crate) sender: mpsc::Sender<WorkflowContext>,
}

pub(crate) fn default_node_buffer() -> usize {
    32
}

// ── 句柄方法 ──────────────────────────────────────

impl WorkflowIngress {
    /// # Errors
    ///
    /// 无根节点发送端或通道已关闭时返回错误。
    pub async fn submit(&self, ctx: WorkflowContext) -> Result<(), EngineError> {
        if self.root_senders.is_empty() {
            return Err(EngineError::invalid_graph(
                "已部署的工作流没有任何根节点发送端",
            ));
        }
        for sender in self.root_senders.values() {
            sender
                .send(ctx.clone())
                .await
                .map_err(|_| EngineError::ChannelClosed {
                    stage: "workflow-ingress".to_owned(),
                })?;
        }
        Ok(())
    }

    /// # Errors
    ///
    /// 指定节点不存在或通道已关闭时返回错误。
    pub async fn submit_to(&self, node_id: &str, ctx: WorkflowContext) -> Result<(), EngineError> {
        let sender = self.root_senders.get(node_id).ok_or_else(|| {
            EngineError::invalid_graph(format!(
                "根节点发送端 `{node_id}` 在已部署的工作流中不可用"
            ))
        })?;
        sender
            .send(ctx)
            .await
            .map_err(|_| EngineError::ChannelClosed {
                stage: "workflow-ingress".to_owned(),
            })
    }

    pub fn root_nodes(&self) -> &[String] {
        &self.root_nodes
    }
}

impl WorkflowStreams {
    pub async fn next_event(&mut self) -> Option<ExecutionEvent> {
        self.event_rx.recv().await
    }

    pub async fn next_result(&mut self) -> Option<WorkflowContext> {
        self.result_rx.recv().await
    }

    pub fn into_receivers(
        self,
    ) -> (
        mpsc::Receiver<ExecutionEvent>,
        mpsc::Receiver<WorkflowContext>,
    ) {
        (self.event_rx, self.result_rx)
    }
}

impl WorkflowDeployment {
    /// # Errors
    ///
    /// 委托给 [`WorkflowIngress::submit`]，参见其错误说明。
    pub async fn submit(&self, ctx: WorkflowContext) -> Result<(), EngineError> {
        self.ingress.submit(ctx).await
    }

    pub async fn next_event(&mut self) -> Option<ExecutionEvent> {
        self.streams.next_event().await
    }

    pub async fn next_result(&mut self) -> Option<WorkflowContext> {
        self.streams.next_result().await
    }

    pub fn ingress(&self) -> &WorkflowIngress {
        &self.ingress
    }

    pub fn into_parts(self) -> (WorkflowIngress, WorkflowStreams) {
        (self.ingress, self.streams)
    }
}
