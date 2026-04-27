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
    CancellationToken, ContextRef, DataStore, EngineError, ExecutionEvent, LifecycleGuard,
    SharedResources, VariableDeclaration, WorkflowContext, WorkflowNodeDefinition,
};
use serde::{Deserialize, Deserializer, Serialize};
use tokio::sync::mpsc;
#[cfg(feature = "ts-export")]
use ts_rs::TS;

/// 从前端 AST 反序列化得到的顶层工作流定义。
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
    /// ADR-0012：工作流级共享变量声明（`name → { type, initial }`）。
    /// 旧图 JSON 中无此字段时反序列化为 `None`（`#[serde(default)]` 兜底），
    /// 消费方调用 `.unwrap_or_default()` 得到空表。
    #[serde(default)]
    #[cfg_attr(feature = "ts-export", ts(optional))]
    pub variables: Option<HashMap<String, VariableDeclaration>>,
}

/// 工作流 DAG 中连接两个节点的有向边。
#[derive(Debug, Clone, Serialize)]
#[cfg_attr(feature = "ts-export", derive(TS), ts(export))]
pub struct WorkflowEdge {
    pub from: String,
    pub to: String,
    #[serde(default)]
    #[cfg_attr(feature = "ts-export", ts(optional))]
    pub source_port_id: Option<String>,
    #[serde(default)]
    #[cfg_attr(feature = "ts-export", ts(optional))]
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
///
/// 持有节点的 [`LifecycleGuard`] 集合（按部署顺序），撤销时由 [`shutdown`]
/// 按逆序优雅清理；若调用方未显式 shutdown，guards 在结构 drop 时通过 RAII
/// 兜底取消（仅 cancel token，不等待任务退出，详见 [`LifecycleGuard`]）。
///
/// `shutdown_token` 是工作流根 token；guards 中各节点持有其派生子 token——
/// 调用根 token 的 cancel 会沿派生链广播到所有节点。
///
/// [`shutdown`]: WorkflowDeployment::shutdown
pub struct WorkflowDeployment {
    pub(crate) ingress: WorkflowIngress,
    pub(crate) streams: WorkflowStreams,
    pub(crate) lifecycle_guards: Vec<(String, LifecycleGuard)>,
    pub(crate) shutdown_token: CancellationToken,
    /// 部署时构造的共享资源句柄（含 `WorkflowVariables` 等），供 IPC 读取共享状态。
    pub(crate) shared_resources: SharedResources,
}

/// [`WorkflowDeployment::into_parts`] 的返回类型——按字段名访问而非位置解构，
/// 让未来增减字段不破坏调用方源码兼容性。
pub struct WorkflowDeploymentParts {
    pub ingress: WorkflowIngress,
    pub streams: WorkflowStreams,
    pub lifecycle_guards: Vec<(String, LifecycleGuard)>,
    pub shutdown_token: CancellationToken,
    /// 部署时构造的共享资源句柄，随 parts 传递给壳层保存。
    pub shared_resources: SharedResources,
}

/// 拓扑分析结果（仅模块内部使用）。
pub(crate) struct WorkflowTopology {
    pub(crate) root_nodes: Vec<String>,
    pub(crate) downstream: HashMap<String, Vec<WorkflowEdge>>,
    /// 完整拓扑序（Kahn 算法输出顺序），用于按依赖顺序调用 `on_deploy`，
    /// 撤销时按逆序 shutdown。
    pub(crate) deployment_order: Vec<String>,
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

    /// 拆分为入口、流、生命周期 guards 与撤销根 token。
    ///
    /// **注意**：丢弃返回的 guards 会立即 cancel 所有节点的 lifecycle token——
    /// 长连接节点（MQTT 订阅 / Timer / Serial 监听）会随之停止。调用方需要
    /// 持有 guards 直至撤销，并在撤销时 cancel `shutdown_token` 让所有节点
    /// 同时收到取消信号（再串行 await guard.shutdown 等待 cleanup 完成）。
    pub fn into_parts(self) -> WorkflowDeploymentParts {
        WorkflowDeploymentParts {
            ingress: self.ingress,
            streams: self.streams,
            lifecycle_guards: self.lifecycle_guards,
            shutdown_token: self.shutdown_token,
            shared_resources: self.shared_resources,
        }
    }

    /// 显式撤销整张图：cancel 根 token 后按**逆部署序** shutdown 每个 guard。
    ///
    /// 调用此方法是确定性等待清理完成的唯一方式；未调用时 `Drop` 仅触发
    /// cancel 不等待，可能与下次 deploy 的资源借出竞态。
    pub async fn shutdown(self) {
        self.shutdown_token.cancel();
        for (_, guard) in self.lifecycle_guards.into_iter().rev() {
            guard.shutdown().await;
        }
    }

    /// 返回内部 [`DataStore`] 的引用。
    pub fn store(&self) -> &Arc<dyn DataStore> {
        &self.streams.store
    }
}

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
        assert!(
            graph.variables.unwrap_or_default().is_empty(),
            "缺省 variables 应为 None（等价空表）"
        );
    }

    #[test]
    fn variables_字段为_null_时反序列化为_none() {
        // `Option<HashMap>` 对应 JSON null → None；前端不会主动产出 null，
        // 但外部修改工程文件时能容错而不是报错。
        let json = serde_json::json!({ "nodes": {}, "edges": [], "variables": null });
        let graph: WorkflowGraph = serde_json::from_value(json).unwrap();
        assert!(
            graph.variables.is_none(),
            "variables: null 应反序列化为 None"
        );
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
        let vars = graph.variables.unwrap();
        assert_eq!(vars.len(), 2);
        assert_eq!(vars["setpoint"].variable_type, nazh_core::PinType::Float);
        assert_eq!(vars["mode"].initial, serde_json::Value::from("auto"));
    }
}
