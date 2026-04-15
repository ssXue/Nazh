//! 工作流部署编排：校验、实例化并将 DAG 部署为并发 Tokio 任务。
//!
//! 每个节点获得独立的任务，通过 MPSC 通道连接。叶节点将结果写入结果流；
//! 所有节点向事件流发送执行事件。连接资源由外部共享连接管理器提供。

use std::{collections::HashMap, time::Duration};

use tokio::sync::mpsc;

use super::runner::run_node;
use super::types::{
    DownstreamTarget, WorkflowDeployment, WorkflowGraph, WorkflowIngress, WorkflowStreams,
};
use crate::registry::NodeRegistry;
use crate::{EngineError, SharedConnectionManager};

/// 校验、实例化并将工作流图部署为并发 Tokio 任务。
///
/// # Errors
///
/// DAG 校验失败、节点实例化失败或不在 Tokio 运行时中调用时返回错误。
pub async fn deploy_workflow(
    graph: WorkflowGraph,
    connection_manager: SharedConnectionManager,
    registry: &NodeRegistry,
) -> Result<WorkflowDeployment, EngineError> {
    let topology = graph.topology()?;

    connection_manager
        .upsert_connections(graph.connections)
        .await;

    let runtime = tokio::runtime::Handle::try_current()
        .map_err(|_| EngineError::invalid_graph("deploy_workflow 必须在 Tokio 运行时中调用"))?;

    let mut senders = HashMap::new();
    let mut receivers = HashMap::new();

    for (node_id, node_definition) in &graph.nodes {
        let (sender, receiver) = mpsc::channel(node_definition.buffer.max(1));
        senders.insert(node_id.clone(), sender);
        receivers.insert(node_id.clone(), receiver);
    }

    let event_capacity = graph.nodes.len().max(1) * 16;
    let (event_tx, event_rx) = mpsc::channel(event_capacity);
    let (result_tx, result_rx) = mpsc::channel(event_capacity);

    for (node_id, node_definition) in &graph.nodes {
        let node = registry.create(node_definition, connection_manager.clone())?;
        let input_rx = receivers
            .remove(node_id)
            .ok_or_else(|| EngineError::invalid_graph("节点接收端缺失"))?;

        let downstream_senders = topology
            .downstream
            .get(node_id)
            .into_iter()
            .flat_map(|edges| edges.iter())
            .filter_map(|edge| {
                senders
                    .get(&edge.to)
                    .cloned()
                    .map(|sender| DownstreamTarget {
                        source_port_id: edge.source_port_id.clone(),
                        sender,
                    })
            })
            .collect::<Vec<_>>();

        runtime.spawn(run_node(
            node,
            node_definition.timeout_ms.map(Duration::from_millis),
            input_rx,
            downstream_senders,
            result_tx.clone(),
            event_tx.clone(),
        ));
    }

    let root_senders = topology
        .root_nodes
        .iter()
        .filter_map(|node_id| {
            senders
                .get(node_id)
                .cloned()
                .map(|sender| (node_id.clone(), sender))
        })
        .collect::<HashMap<_, _>>();

    drop(result_tx);
    drop(event_tx);

    Ok(WorkflowDeployment {
        ingress: WorkflowIngress {
            root_nodes: topology.root_nodes,
            root_senders,
        },
        streams: WorkflowStreams {
            event_rx,
            result_rx,
        },
    })
}
