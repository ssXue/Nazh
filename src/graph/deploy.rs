//! 工作流部署编排：校验、实例化并将 DAG 部署为并发 Tokio 任务。
//!
//! 每个节点获得独立的任务，通过 MPSC 通道连接。通道传递 [`ContextRef`]（~64 字节），
//! 实际数据存储在共享的 [`ArenaDataStore`] 中，实现零拷贝扇出。
//!
//! ## 部署阶段
//!
//! 部署分两个阶段（ADR-0009）：
//! 1. **`on_deploy` 阶段**：按拓扑序为每个节点构造 [`NodeLifecycleContext`] 并调用
//!    [`NodeTrait::on_deploy`]，收集返回的 [`LifecycleGuard`]。任一节点失败则按
//!    逆序释放已收集的 guards（RAII Drop 自动 cancel 内部任务），整图回滚。
//! 2. **spawn 阶段**：为每个节点 spawn `run_node` 循环。该阶段仅消费阶段 1 已
//!    实例化的节点，不再调用 `registry.create`。
//!
//! 触发器节点在 `on_deploy` 中 spawn 的后台任务可能早于 spawn 阶段就调用
//! [`NodeHandle::emit`](nazh_core::NodeHandle::emit)——通过 channel buffer
//! 自然缓冲，待 spawn 阶段完成后下游 `run_node` 自然消费。

use std::{collections::HashMap, sync::Arc, time::Duration};

use ai::AiService;
use tokio::sync::mpsc;

use super::runner::run_node;
use super::types::{
    DownstreamTarget, WorkflowDeployment, WorkflowGraph, WorkflowIngress, WorkflowStreams,
};
use crate::SharedConnectionManager;
use nazh_core::{
    ArenaDataStore, CancellationToken, ContextRef, DataStore, EngineError, NodeHandle,
    NodeLifecycleContext, NodeRegistry, NodeTrait, RuntimeResources, SharedResources,
};

/// 校验、实例化并将工作流图部署为并发 Tokio 任务。
///
/// 内部创建 [`ArenaDataStore`] 作为数据面，所有节点共享同一实例。
///
/// # Errors
///
/// DAG 校验失败、节点实例化失败、节点 `on_deploy` 失败或不在 Tokio 运行时
/// 中调用时返回错误。
pub async fn deploy_workflow(
    graph: WorkflowGraph,
    connection_manager: SharedConnectionManager,
    registry: &NodeRegistry,
) -> Result<WorkflowDeployment, EngineError> {
    deploy_workflow_with_ai(graph, connection_manager, None, registry).await
}

/// 校验、实例化并将工作流图部署为并发 Tokio 任务，并可选注入 AI 服务。
///
/// # Errors
///
/// DAG 校验失败、节点实例化失败、节点 `on_deploy` 失败或不在 Tokio 运行时
/// 中调用时返回错误。
// 函数为两阶段部署的线性主流程（on_deploy 阶段 + spawn 阶段 + ingress 收集）。
// 按 plan「clippy 收支表」预期，ADR-0009 完成后此函数仍超过 100 行 lint 阈值；
// 拆 helper 反而切碎主流程时序，损可读性。Task 1 实施时确认该 #[allow] 保留。
#[allow(clippy::too_many_lines)]
pub async fn deploy_workflow_with_ai(
    graph: WorkflowGraph,
    connection_manager: SharedConnectionManager,
    ai_service: Option<Arc<dyn AiService>>,
    registry: &NodeRegistry,
) -> Result<WorkflowDeployment, EngineError> {
    tracing::info!(
        node_count = graph.nodes.len(),
        edge_count = graph.edges.len(),
        "开始部署工作流 DAG"
    );
    let topology = graph.topology()?;

    connection_manager
        .upsert_connections(graph.connections)
        .await;

    let runtime = tokio::runtime::Handle::try_current()
        .map_err(|_| EngineError::invalid_graph("deploy_workflow 必须在 Tokio 运行时中调用"))?;

    // 创建共享 DataStore（数据面）
    let store: Arc<dyn DataStore> = Arc::new(ArenaDataStore::new());

    let mut senders = HashMap::new();
    let mut receivers = HashMap::new();

    for (node_id, node_definition) in &graph.nodes {
        let (sender, receiver) = mpsc::channel::<ContextRef>(node_definition.buffer().max(1));
        senders.insert(node_id.clone(), sender);
        receivers.insert(node_id.clone(), receiver);
    }

    let event_capacity = graph.nodes.len().max(1) * 16;
    let (event_tx, event_rx) = mpsc::channel(event_capacity);
    let (result_tx, result_rx) = mpsc::channel(event_capacity);

    let mut resource_bag = RuntimeResources::new().with_resource(connection_manager.clone());
    if let Some(ai_service) = ai_service {
        resource_bag.insert(ai_service);
    }
    let shared_resources: SharedResources = Arc::new(resource_bag);

    // ---- 阶段 1：按拓扑序实例化节点 + on_deploy（ADR-0009）----
    //
    // 拓扑序保证上游节点先完成 on_deploy，让下游节点 on_deploy 时上游的资源
    // （连接、订阅）已就绪——为未来跨节点资源依赖打基础。任一节点失败时
    // `lifecycle_guards` 在函数返回前 drop，按 RAII 自动 cancel 已部署的后台任务。
    let shutdown_token = CancellationToken::new();
    let mut nodes_by_id: HashMap<String, Arc<dyn NodeTrait>> = HashMap::new();
    let mut lifecycle_guards = Vec::with_capacity(topology.deployment_order.len());

    for node_id in &topology.deployment_order {
        let node_definition = graph.nodes.get(node_id).ok_or_else(|| {
            EngineError::invalid_graph(format!("拓扑序中的节点 `{node_id}` 在图中不存在"))
        })?;
        let node = registry.create(node_definition, shared_resources.clone())?;

        // 触发器节点 emit 时直接广播给所有下游 sender，不按 port 路由——
        // 路由语义只对 transform 路径有效（ADR-0008 metadata 通道是另一回事）。
        let downstream_for_handle = topology
            .downstream
            .get(node_id)
            .into_iter()
            .flat_map(|edges| edges.iter())
            .filter_map(|edge| senders.get(&edge.to).cloned())
            .collect::<Vec<_>>();

        let handle = NodeHandle::new(
            node_id.clone(),
            Arc::clone(&store),
            downstream_for_handle,
            event_tx.clone(),
        );

        let ctx = NodeLifecycleContext {
            resources: shared_resources.clone(),
            handle,
            shutdown: shutdown_token.child_token(),
        };

        let guard = node.on_deploy(ctx).await?;
        lifecycle_guards.push((node_id.clone(), guard));
        nodes_by_id.insert(node_id.clone(), node);
    }

    // ---- 阶段 2：spawn run_node ----
    //
    // 不再调用 registry.create——节点实例已在阶段 1 创建并持有 lifecycle guard。
    // 阶段 2 失败仍需要保留已收集的 guards 让 RAII 清理 on_deploy 拉起的任务。
    for node_id in &topology.deployment_order {
        let Some(node) = nodes_by_id.remove(node_id) else {
            return Err(EngineError::invalid_graph(format!(
                "节点 `{node_id}` 在阶段 2 缺失"
            )));
        };
        let node_definition = graph
            .nodes
            .get(node_id)
            .ok_or_else(|| EngineError::invalid_graph("节点定义在阶段 2 缺失"))?;
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
            node_definition.timeout_ms().map(Duration::from_millis),
            input_rx,
            downstream_senders,
            result_tx.clone(),
            event_tx.clone(),
            Arc::clone(&store),
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

    tracing::info!(
        root_count = topology.root_nodes.len(),
        guard_count = lifecycle_guards.len(),
        "工作流 DAG 部署完成"
    );

    Ok(WorkflowDeployment {
        ingress: WorkflowIngress {
            root_nodes: topology.root_nodes,
            root_senders,
            store: Arc::clone(&store),
        },
        streams: WorkflowStreams {
            event_rx,
            result_rx,
            store,
        },
        lifecycle_guards,
        shutdown_token,
    })
}
