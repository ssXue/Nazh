//! 工作流图的数据结构定义与句柄方法。
//!
//! 本文件包含所有序列化/反序列化结构体、可观测事件枚举，
//! 以及已部署工作流的入口 ([`WorkflowIngress`])、流 ([`WorkflowStreams`])
//! 和组合句柄 ([`WorkflowDeployment`])。
//!
//! 内部通道传递 [`ContextRef`]（~64 字节），实际数据存储在 [`DataStore`] 中。
//! 外部 API（`submit` / `next_result`）仍使用 [`WorkflowContext`]，转换在边界层完成。

use std::{collections::HashMap, sync::Arc};

use crate::{
    ContextRef, DataStore, EngineError, ExecutionEvent, WorkflowContext, WorkflowNodeDefinition,
};
use serde::{Deserialize, Deserializer, Serialize};
use tokio::sync::mpsc;
use ts_rs::TS;

/// 从前端 AST 反序列化得到的顶层工作流定义。
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct WorkflowGraph {
    #[serde(default)]
    #[ts(optional)]
    pub name: Option<String>,
    #[serde(default)]
    pub connections: Vec<crate::ConnectionDefinition>,
    #[serde(default)]
    pub nodes: HashMap<String, WorkflowNodeDefinition>,
    #[serde(default)]
    pub edges: Vec<WorkflowEdge>,
}

/// 工作流 DAG 中连接两个节点的有向边。
#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct WorkflowEdge {
    pub from: String,
    pub to: String,
    #[serde(default)]
    #[ts(optional)]
    pub source_port_id: Option<String>,
    #[serde(default)]
    #[ts(optional)]
    pub target_port_id: Option<String>,
}

impl<'de> Deserialize<'de> for WorkflowEdge {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        struct WorkflowEdgeInput {
            #[serde(alias = "source")]
            from: String,
            #[serde(alias = "target")]
            to: String,
            #[serde(default, alias = "sourcePortID")]
            source_port_id: Option<String>,
            #[serde(default, alias = "targetPortID")]
            target_port_id: Option<String>,
        }

        let input = WorkflowEdgeInput::deserialize(deserializer)?;
        Ok(Self {
            from: input.from,
            to: input.to,
            source_port_id: input.source_port_id,
            target_port_id: input.target_port_id,
        })
    }
}

/// 入口句柄，用于向已部署工作流的根节点提交数据。
#[derive(Clone)]
pub struct WorkflowIngress {
    pub(crate) root_nodes: Vec<String>,
    pub(crate) root_senders: HashMap<String, mpsc::Sender<ContextRef>>,
    pub(crate) store: Arc<dyn DataStore>,
}

/// 已部署工作流的事件流和结果流接收端。
pub struct WorkflowStreams {
    pub(crate) event_rx: mpsc::Receiver<ExecutionEvent>,
    pub(crate) result_rx: mpsc::Receiver<ContextRef>,
    pub(crate) store: Arc<dyn DataStore>,
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
    pub(crate) sender: mpsc::Sender<ContextRef>,
}

impl WorkflowIngress {
    /// 将 [`WorkflowContext`] 写入 [`DataStore`] 并向所有根节点发送 [`ContextRef`]。
    ///
    /// # Errors
    ///
    /// 无根节点发送端或通道已关闭时返回错误。
    pub async fn submit(&self, ctx: WorkflowContext) -> Result<(), EngineError> {
        if self.root_senders.is_empty() {
            return Err(EngineError::invalid_graph(
                "已部署的工作流没有任何根节点发送端",
            ));
        }
        let consumer_count = self.root_senders.len();
        let trace_id = ctx.trace_id;
        let timestamp = ctx.timestamp;
        let data_id = self.store.write(ctx.payload, consumer_count)?;
        let ctx_ref = ContextRef {
            trace_id,
            timestamp,
            data_id,
            source_node: None,
        };

        for sender in self.root_senders.values() {
            sender
                .send(ctx_ref.clone())
                .await
                .map_err(|_| EngineError::ChannelClosed {
                    stage: "workflow-ingress".to_owned(),
                })?;
        }
        Ok(())
    }

    /// 向指定根节点发送数据。
    ///
    /// # Errors
    ///
    /// 指定节点不存在或通道已关闭时返回错误。
    pub async fn submit_to(&self, node_id: &str, ctx: WorkflowContext) -> Result<(), EngineError> {
        let sender = self.root_senders.get(node_id).ok_or_else(|| {
            EngineError::invalid_graph(format!("根节点发送端 `{node_id}` 在已部署的工作流中不可用"))
        })?;
        let trace_id = ctx.trace_id;
        let timestamp = ctx.timestamp;
        let data_id = self.store.write(ctx.payload, 1)?;
        let ctx_ref = ContextRef {
            trace_id,
            timestamp,
            data_id,
            source_node: None,
        };
        sender
            .send(ctx_ref)
            .await
            .map_err(|_| EngineError::ChannelClosed {
                stage: "workflow-ingress".to_owned(),
            })
    }

    /// 阻塞式提交，用于同步硬件监听线程。
    ///
    /// # Errors
    ///
    /// 指定节点不存在或通道已关闭时返回错误。
    pub fn blocking_submit_to(
        &self,
        node_id: &str,
        ctx: WorkflowContext,
    ) -> Result<(), EngineError> {
        let sender = self.root_senders.get(node_id).ok_or_else(|| {
            EngineError::invalid_graph(format!("根节点发送端 `{node_id}` 在已部署的工作流中不可用"))
        })?;
        let trace_id = ctx.trace_id;
        let timestamp = ctx.timestamp;
        let data_id = self.store.write(ctx.payload, 1)?;
        let ctx_ref = ContextRef {
            trace_id,
            timestamp,
            data_id,
            source_node: None,
        };
        sender
            .blocking_send(ctx_ref)
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

    /// 从结果流中取出下一个 [`ContextRef`]，从 [`DataStore`] 重建为 [`WorkflowContext`]。
    pub async fn next_result(&mut self) -> Option<WorkflowContext> {
        let ctx_ref = self.result_rx.recv().await?;
        let payload = self.store.read(&ctx_ref.data_id).ok()?;
        self.store.release(&ctx_ref.data_id);
        Some(WorkflowContext::from_parts(
            ctx_ref.trace_id,
            ctx_ref.timestamp,
            (*payload).clone(),
        ))
    }

    /// 拆分为原始接收端和 [`DataStore`]，供需要自行管理生命周期的调用者使用。
    ///
    /// 结果流中的 [`ContextRef`] 需要调用者自行从 [`DataStore`] 重建 [`WorkflowContext`]
    /// 并在使用后调用 `store.release()` 释放数据引用。
    pub fn into_receivers(
        self,
    ) -> (
        mpsc::Receiver<ExecutionEvent>,
        mpsc::Receiver<ContextRef>,
        Arc<dyn DataStore>,
    ) {
        (self.event_rx, self.result_rx, self.store)
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

    /// 返回内部 [`DataStore`] 的引用。
    pub fn store(&self) -> &Arc<dyn DataStore> {
        &self.streams.store
    }
}
