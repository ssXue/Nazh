//! 工作流部署编排：校验、实例化并将 DAG 部署为并发 Tokio 任务。
//!
//! 每个节点获得独立的任务，通过 MPSC 通道连接。通道传递 [`ContextRef`]（~64 字节），
//! 实际数据存储在共享的 [`ArenaDataStore`] 中，实现零拷贝扇出。
//!
//! ## 部署阶段
//!
//! 部署分四个阶段：
//! 0. **阶段 0 — 工作流变量构造**：调 [`build_workflow_variables`](super::variables_init::build_workflow_variables)
//!    把 `WorkflowGraph.variables` 声明转成 `Arc<WorkflowVariables>`，注入
//!    `RuntimeResources` 与 `NodeLifecycleContext`。声明初值类型不匹配立即整图
//!    拒绝——节点尚未实例化、无 RAII 资源在手，无需回滚（ADR-0012）。
//! 1. **阶段 0.5 — Pin 类型校验**：节点按拓扑序实例化（仅调 `registry.create`，
//!    无副作用），再调 [`pin_validator::validate_pin_compatibility`] 校验所有
//!    边的两端 pin 类型兼容、`required` 输入有上游、无重复 pin id。失败直接返
//!    回错误——节点尚未 `on_deploy`，无 RAII 资源需要回滚（详见 ADR-0010）。
//! 2. **`on_deploy` 阶段**：按拓扑序为每个节点构造 [`NodeLifecycleContext`] 并调用
//!    [`NodeTrait::on_deploy`]，收集返回的 [`LifecycleGuard`]。任一节点失败则按
//!    逆序释放已收集的 guards（RAII Drop 自动 cancel 内部任务），整图回滚。
//! 3. **spawn 阶段**：为每个节点 spawn `run_node` 循环。该阶段仅消费阶段 0.5 已
//!    实例化的节点，不再调用 `registry.create`。
//!
//! 触发器节点在 `on_deploy` 中 spawn 的后台任务可能早于 spawn 阶段就调用
//! [`NodeHandle::emit`](nazh_core::NodeHandle::emit)——通过 channel buffer
//! 自然缓冲，待 spawn 阶段完成后下游 `run_node` 自然消费。
//!
//! 设计决策见 ADR-0009 / ADR-0010。

use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
    time::Duration,
};

use nazh_core::ai::AiService;
use tokio::sync::mpsc;

use super::pin_validator;
use super::runner::run_node;
use super::topology::{classify_edges, detect_data_edge_cycle};
use super::types::{
    DownstreamTarget, WorkflowDeployment, WorkflowGraph, WorkflowIngress, WorkflowStreams,
};
use super::variables_init::build_workflow_variables;
use crate::SharedConnectionManager;
use nazh_core::{
    ArenaDataStore, CancellationToken, ContextRef, DataStore, EngineError, NodeHandle,
    NodeLifecycleContext, NodeRegistry, NodeTrait, OutputCache, PinKind, RuntimeResources,
    SharedResources,
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
    deploy_workflow_with_ai(graph, connection_manager, None, registry, None).await
}

/// 校验、实例化并将工作流图部署为并发 Tokio 任务，并可选注入 AI 服务。
///
/// `workflow_id` 由调用方传入，用于 [`ExecutionEvent::VariableChanged`] 事件的
/// `workflow_id` 字段。Tauri shell 经 `derive_workflow_id` 派生后传入，保证与
/// `DesktopWorkflow.workflow_id` 对齐；引擎内部调用 / 测试可传 `None`，
/// 此时按 `graph.name > "anonymous"` 顺序 fallback。
///
/// # Errors
///
/// DAG 校验失败、节点实例化失败、节点 `on_deploy` 失败或不在 Tokio 运行时
/// 中调用时返回错误。
// 函数为四阶段部署的线性主流程（阶段 0 变量构造、阶段 0.5 实例化 + Pin 校验、
// 阶段 1 on_deploy、阶段 2 spawn run_node），拆 helper 会切碎时序的关键不变量
// （每阶段全部完成才能进下一阶段），损可读性。
#[allow(clippy::too_many_lines)]
pub async fn deploy_workflow_with_ai(
    graph: WorkflowGraph,
    connection_manager: SharedConnectionManager,
    ai_service: Option<Arc<dyn AiService>>,
    registry: &NodeRegistry,
    workflow_id: Option<String>,
) -> Result<WorkflowDeployment, EngineError> {
    tracing::info!(
        node_count = graph.nodes.len(),
        edge_count = graph.edges.len(),
        "开始部署工作流 DAG"
    );
    let topology = graph.topology()?;

    // ---- 阶段 0：构造工作流变量（早于 connection 装配、Pin 校验）----
    //
    // 声明的初值类型若与声明类型不匹配立即整图失败——节点尚未实例化、
    // 无 RAII 资源在手，无需回滚（ADR-0012 早失败原则）。
    let workflow_variables = build_workflow_variables(graph.variables.as_ref())?;

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

    // ADR-0012 Phase 2：注入事件通道，让 set/CAS 在值变化时通过 ExecutionEvent::VariableChanged
    // 流向 Tauri shell drain loop（Task 4 转发到 workflow://variable-changed）。
    //
    // 必须在 resource_bag 装配和节点 on_deploy 启动之前完成注入——
    // 节点 on_deploy 期间的写入若 event_sink 尚未就绪将静默丢失事件。
    //
    // workflow_id 由调用方传入（src-tauri 经 derive_workflow_id 派生），engine 不再自己
    // 从 graph.name 派生——避免 src-tauri 用显式 workflow_id 或 project_id 派生时与
    // engine 内部 fallback 产生 diverge，导致 Task 4 前端事件按 workflow_id 过滤失败。
    // 优先级：调用方显式传入 > graph.name > "anonymous" fallback。
    let workflow_id_for_events = workflow_id
        .or_else(|| graph.name.clone())
        .unwrap_or_else(|| "anonymous".to_owned());
    workflow_variables.set_event_sender(workflow_id_for_events, event_tx.clone());

    let mut resource_bag = RuntimeResources::new()
        .with_resource(connection_manager.clone())
        .with_resource(Arc::clone(&workflow_variables));
    if let Some(ai_service) = ai_service {
        resource_bag.insert(ai_service);
    }
    let shared_resources: SharedResources = Arc::new(resource_bag);

    // ---- 阶段 0.5：按拓扑序实例化节点 + Pin 类型校验 ----
    //
    // 实例化（registry.create）不应有副作用——节点构造函数只读 config + 资源
    // 句柄克隆。这一阶段把"trait 元数据查询"与"on_deploy 副作用"清晰分离：
    // 任何边类型不兼容 / pin id 不存在 / 重复 pin / 缺失 required input 都在
    // 进入 on_deploy 之前失败，无需 RAII 回滚（无 LifecycleGuard 在手）。
    //
    // 设计决策见 ADR-0010。
    let mut nodes_by_id: HashMap<String, Arc<dyn NodeTrait>> = HashMap::new();
    for node_id in &topology.deployment_order {
        let node_definition = graph.nodes.get(node_id).ok_or_else(|| {
            EngineError::invalid_graph(format!("拓扑序中的节点 `{node_id}` 在图中不存在"))
        })?;
        let node = registry.create(node_definition, shared_resources.clone())?;
        nodes_by_id.insert(node_id.clone(), node);
    }
    pin_validator::validate_pin_compatibility(&nodes_by_id, &graph.edges)?;

    // ADR-0014 Phase 1：边按上游 source pin 的 PinKind 分类，Data 子图独立环检测
    let classified = classify_edges(&graph.edges, &nodes_by_id)?;
    detect_data_edge_cycle(&classified.data_edges)?;

    // 单次遍历给每节点同时构造 OutputCache（slots 预分配）与 data_output_pin_ids 集合
    let mut output_caches: HashMap<String, Arc<OutputCache>> =
        HashMap::with_capacity(nodes_by_id.len());
    let mut data_output_pin_ids_by_node: HashMap<String, HashSet<String>> =
        HashMap::with_capacity(nodes_by_id.len());
    for (id, node) in &nodes_by_id {
        let cache = OutputCache::new();
        let mut pin_ids = HashSet::new();
        for pin in node.output_pins() {
            if pin.kind == PinKind::Data {
                cache.prepare_slot(&pin.id);
                pin_ids.insert(pin.id);
            }
        }
        output_caches.insert(id.clone(), Arc::new(cache));
        data_output_pin_ids_by_node.insert(id.clone(), pin_ids);
    }

    // ---- 阶段 1：on_deploy ----
    //
    // 拓扑序保证上游节点先完成 on_deploy，让下游节点 on_deploy 时上游的资源
    // （连接、订阅）已就绪——为未来跨节点资源依赖打基础。任一节点失败时
    // `lifecycle_guards` 在函数返回前 drop，按 RAII 自动 cancel 已部署的后台任务。
    let shutdown_token = CancellationToken::new();
    let mut lifecycle_guards = Vec::with_capacity(topology.deployment_order.len());

    for node_id in &topology.deployment_order {
        let node = nodes_by_id.get(node_id).ok_or_else(|| {
            EngineError::invalid_graph(format!("阶段 1：节点 `{node_id}` 在阶段 0.5 缺失"))
        })?;

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
            variables: Arc::clone(&workflow_variables),
        };

        let guard = node.on_deploy(ctx).await?;
        lifecycle_guards.push((node_id.clone(), guard));
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

        let data_output_pin_ids = data_output_pin_ids_by_node
            .get(node_id)
            .cloned()
            .unwrap_or_default();
        let output_cache = Arc::clone(
            output_caches
                .get(node_id)
                .ok_or_else(|| EngineError::invalid_graph("阶段 2：output_cache 缺失"))?,
        );

        runtime.spawn(run_node(
            node,
            node_definition.timeout_ms().map(Duration::from_millis),
            input_rx,
            downstream_senders,
            result_tx.clone(),
            event_tx.clone(),
            Arc::clone(&store),
            output_cache,
            data_output_pin_ids,
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
        shared_resources: shared_resources.clone(),
        output_caches,
    })
}
