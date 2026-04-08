use std::{
    collections::{HashMap, VecDeque},
    panic::AssertUnwindSafe,
    sync::Arc,
    time::Duration,
};

use futures_util::FutureExt;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio::sync::mpsc;
use uuid::Uuid;

use crate::{
    ConnectionDefinition, DebugConsoleNode, DebugConsoleNodeConfig, EngineError, HttpClientNode,
    HttpClientNodeConfig, IfNode, IfNodeConfig, LoopNode, LoopNodeConfig, ModbusReadNode,
    ModbusReadNodeConfig, NativeNode, NativeNodeConfig, NodeDispatch, NodeTrait, RhaiNode,
    RhaiNodeConfig, SharedConnectionManager, SqlWriterNode, SqlWriterNodeConfig, SwitchNode,
    SwitchNodeConfig, TimerNode, TimerNodeConfig, TryCatchNode, TryCatchNodeConfig,
    WorkflowContext,
};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowGraph {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub connections: Vec<ConnectionDefinition>,
    #[serde(default)]
    pub nodes: HashMap<String, WorkflowNodeDefinition>,
    #[serde(default)]
    pub edges: Vec<WorkflowEdge>,
}

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

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum WorkflowEvent {
    NodeStarted { node_id: String, trace_id: Uuid },
    NodeCompleted { node_id: String, trace_id: Uuid },
    NodeFailed {
        node_id: String,
        trace_id: Uuid,
        error: String,
    },
    WorkflowOutput { node_id: String, trace_id: Uuid },
}

#[derive(Clone)]
pub struct WorkflowIngress {
    root_nodes: Vec<String>,
    root_senders: HashMap<String, mpsc::Sender<WorkflowContext>>,
}

pub struct WorkflowStreams {
    event_rx: mpsc::Receiver<WorkflowEvent>,
    result_rx: mpsc::Receiver<WorkflowContext>,
}

pub struct WorkflowDeployment {
    ingress: WorkflowIngress,
    streams: WorkflowStreams,
}

struct WorkflowTopology {
    root_nodes: Vec<String>,
    downstream: HashMap<String, Vec<WorkflowEdge>>,
}

#[derive(Clone)]
struct DownstreamTarget {
    source_port_id: Option<String>,
    sender: mpsc::Sender<WorkflowContext>,
}

fn default_node_buffer() -> usize {
    32
}

impl WorkflowGraph {
    pub fn from_json(ast: &str) -> Result<Self, EngineError> {
        let mut graph: WorkflowGraph = serde_json::from_str(ast)
            .map_err(|error| EngineError::graph_deserialization(error.to_string()))?;

        for (node_id, node_definition) in &mut graph.nodes {
            if node_definition.id.is_empty() {
                node_definition.id = node_id.clone();
            }

            if node_definition.connection_id.is_none() {
                node_definition.connection_id = node_definition
                    .config
                    .get("connection_id")
                    .and_then(Value::as_str)
                    .map(ToOwned::to_owned);
            }
        }

        graph.validate()?;
        Ok(graph)
    }

    pub fn validate(&self) -> Result<(), EngineError> {
        let topology = self.topology()?;
        if topology.root_nodes.is_empty() {
            return Err(EngineError::invalid_graph(
                "the graph must contain at least one root node",
            ));
        }
        Ok(())
    }

    fn topology(&self) -> Result<WorkflowTopology, EngineError> {
        let mut incoming: HashMap<String, usize> = self
            .nodes
            .keys()
            .map(|node_id| (node_id.clone(), 0_usize))
            .collect();
        let mut downstream: HashMap<String, Vec<WorkflowEdge>> = self
            .nodes
            .keys()
            .map(|node_id| (node_id.clone(), Vec::new()))
            .collect();

        for edge in &self.edges {
            if !self.nodes.contains_key(&edge.from) {
                return Err(EngineError::invalid_graph(format!(
                    "edge source `{}` does not exist",
                    edge.from
                )));
            }

            if !self.nodes.contains_key(&edge.to) {
                return Err(EngineError::invalid_graph(format!(
                    "edge target `{}` does not exist",
                    edge.to
                )));
            }

            downstream
                .entry(edge.from.clone())
                .or_default()
                .push(edge.clone());

            if let Some(count) = incoming.get_mut(&edge.to) {
                *count += 1;
            }
        }

        let root_nodes = incoming
            .iter()
            .filter(|(_, count)| **count == 0)
            .map(|(node_id, _)| node_id.clone())
            .collect::<Vec<_>>();

        let mut queue = VecDeque::from(root_nodes.clone());
        let mut remaining_incoming = incoming.clone();
        let mut processed = 0_usize;

        while let Some(node_id) = queue.pop_front() {
            processed += 1;
            if let Some(neighbors) = downstream.get(&node_id) {
                for neighbor in neighbors {
                    if let Some(count) = remaining_incoming.get_mut(&neighbor.to) {
                        *count -= 1;
                        if *count == 0 {
                            queue.push_back(neighbor.to.clone());
                        }
                    }
                }
            }
        }

        if processed != self.nodes.len() {
            return Err(EngineError::invalid_graph(
                "the workflow graph must be a DAG without cycles",
            ));
        }

        Ok(WorkflowTopology {
            root_nodes,
            downstream,
        })
    }
}

impl WorkflowIngress {
    pub async fn submit(&self, ctx: WorkflowContext) -> Result<(), EngineError> {
        if self.root_senders.is_empty() {
            return Err(EngineError::invalid_graph(
                "the deployed workflow does not have any root node senders",
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

    pub async fn submit_to(
        &self,
        node_id: &str,
        ctx: WorkflowContext,
    ) -> Result<(), EngineError> {
        let sender = self.root_senders.get(node_id).ok_or_else(|| {
            EngineError::invalid_graph(format!(
                "root node sender `{node_id}` is not available in the deployed workflow"
            ))
        })?;

        sender.send(ctx).await.map_err(|_| EngineError::ChannelClosed {
            stage: "workflow-ingress".to_owned(),
        })
    }

    pub fn root_nodes(&self) -> &[String] {
        &self.root_nodes
    }
}

impl WorkflowStreams {
    pub async fn next_event(&mut self) -> Option<WorkflowEvent> {
        self.event_rx.recv().await
    }

    pub async fn next_result(&mut self) -> Option<WorkflowContext> {
        self.result_rx.recv().await
    }

    pub fn into_receivers(
        self,
    ) -> (
        mpsc::Receiver<WorkflowEvent>,
        mpsc::Receiver<WorkflowContext>,
    ) {
        (self.event_rx, self.result_rx)
    }
}

impl WorkflowDeployment {
    pub async fn submit(&self, ctx: WorkflowContext) -> Result<(), EngineError> {
        self.ingress.submit(ctx).await
    }

    pub async fn next_event(&mut self) -> Option<WorkflowEvent> {
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

pub async fn deploy_workflow(
    graph: WorkflowGraph,
    connection_manager: SharedConnectionManager,
) -> Result<WorkflowDeployment, EngineError> {
    let topology = graph.topology()?;
    let runtime = tokio::runtime::Handle::try_current().map_err(|_| {
        EngineError::invalid_graph("deploy_workflow must run inside a Tokio runtime")
    })?;

    if !graph.connections.is_empty() {
        let mut manager = connection_manager.write().await;
        manager.upsert_connections(graph.connections.clone());
    }

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
        let node = instantiate_node(node_definition, connection_manager.clone())?;
        let input_rx = receivers
            .remove(node_id)
            .ok_or_else(|| EngineError::invalid_graph("missing node receiver"))?;

        let downstream_senders = topology
            .downstream
            .get(node_id)
            .into_iter()
            .flat_map(|edges| edges.iter())
            .filter_map(|edge| {
                senders.get(&edge.to).cloned().map(|sender| DownstreamTarget {
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
        .filter_map(|node_id| senders.get(node_id).cloned().map(|sender| (node_id.clone(), sender)))
        .collect::<HashMap<_, _>>();

    drop(result_tx);
    drop(event_tx);

    Ok(WorkflowDeployment {
        ingress: WorkflowIngress {
            root_nodes: topology.root_nodes,
            root_senders,
        },
        streams: WorkflowStreams { event_rx, result_rx },
    })
}

fn instantiate_node(
    definition: &WorkflowNodeDefinition,
    connection_manager: SharedConnectionManager,
) -> Result<Arc<dyn NodeTrait>, EngineError> {
    match definition.node_type.as_str() {
        "native" | "native/log" | "log" => {
            let mut config: NativeNodeConfig = serde_json::from_value(definition.config.clone())
                .map_err(|error| EngineError::node_config(definition.id.clone(), error.to_string()))?;

            if config.connection_id.is_none() {
                config.connection_id = definition.connection_id.clone();
            }

            let description = definition.ai_description.clone().unwrap_or_else(|| {
                "Print payload metadata and optionally attach connection context".to_owned()
            });

            Ok(Arc::new(NativeNode::new(
                definition.id.clone(),
                config,
                description,
                connection_manager,
            )))
        }
        "rhai" | "code" | "code/rhai" => {
            let config: RhaiNodeConfig = serde_json::from_value(definition.config.clone())
                .map_err(|error| EngineError::node_config(definition.id.clone(), error.to_string()))?;
            let description = definition.ai_description.clone().unwrap_or_else(|| {
                "Execute business logic with a bounded Rhai script".to_owned()
            });
            Ok(Arc::new(RhaiNode::new(
                definition.id.clone(),
                config,
                description,
            )?))
        }
        "timer" => {
            let config: TimerNodeConfig = serde_json::from_value(definition.config.clone())
                .map_err(|error| EngineError::node_config(definition.id.clone(), error.to_string()))?;
            let description = definition.ai_description.clone().unwrap_or_else(|| {
                "Trigger the workflow on a fixed interval and inject timer metadata".to_owned()
            });
            Ok(Arc::new(TimerNode::new(
                definition.id.clone(),
                config,
                description,
            )))
        }
        "modbusRead" | "modbus/read" => {
            let mut config: ModbusReadNodeConfig =
                serde_json::from_value(definition.config.clone()).map_err(|error| {
                    EngineError::node_config(definition.id.clone(), error.to_string())
                })?;

            if config.connection_id.is_none() {
                config.connection_id = definition.connection_id.clone();
            }

            let description = definition.ai_description.clone().unwrap_or_else(|| {
                "Read simulated Modbus registers and enrich the payload with telemetry".to_owned()
            });
            Ok(Arc::new(ModbusReadNode::new(
                definition.id.clone(),
                config,
                description,
                connection_manager,
            )))
        }
        "if" => {
            let config: IfNodeConfig = serde_json::from_value(definition.config.clone())
                .map_err(|error| EngineError::node_config(definition.id.clone(), error.to_string()))?;
            let description = definition.ai_description.clone().unwrap_or_else(|| {
                "Evaluate a boolean script and dispatch to true or false".to_owned()
            });
            Ok(Arc::new(IfNode::new(
                definition.id.clone(),
                config,
                description,
            )?))
        }
        "switch" => {
            let config: SwitchNodeConfig = serde_json::from_value(definition.config.clone())
                .map_err(|error| EngineError::node_config(definition.id.clone(), error.to_string()))?;
            let description = definition.ai_description.clone().unwrap_or_else(|| {
                "Evaluate a route script and dispatch to the matched branch".to_owned()
            });
            Ok(Arc::new(SwitchNode::new(
                definition.id.clone(),
                config,
                description,
            )?))
        }
        "tryCatch" => {
            let config: TryCatchNodeConfig = serde_json::from_value(definition.config.clone())
                .map_err(|error| EngineError::node_config(definition.id.clone(), error.to_string()))?;
            let description = definition.ai_description.clone().unwrap_or_else(|| {
                "Execute a guarded script and dispatch to try or catch".to_owned()
            });
            Ok(Arc::new(TryCatchNode::new(
                definition.id.clone(),
                config,
                description,
            )?))
        }
        "loop" => {
            let config: LoopNodeConfig = serde_json::from_value(definition.config.clone())
                .map_err(|error| EngineError::node_config(definition.id.clone(), error.to_string()))?;
            let description = definition.ai_description.clone().unwrap_or_else(|| {
                "Evaluate an iterable script and dispatch each item through body before done".to_owned()
            });
            Ok(Arc::new(LoopNode::new(
                definition.id.clone(),
                config,
                description,
            )?))
        }
        "httpClient" | "http/client" => {
            let config: HttpClientNodeConfig = serde_json::from_value(definition.config.clone())
                .map_err(|error| EngineError::node_config(definition.id.clone(), error.to_string()))?;
            let description = definition.ai_description.clone().unwrap_or_else(|| {
                "Send the payload to an HTTP endpoint such as DingTalk robot alarms".to_owned()
            });
            Ok(Arc::new(HttpClientNode::new(
                definition.id.clone(),
                config,
                description,
            )))
        }
        "sqlWriter" | "sql/writer" => {
            let config: SqlWriterNodeConfig = serde_json::from_value(definition.config.clone())
                .map_err(|error| EngineError::node_config(definition.id.clone(), error.to_string()))?;
            let description = definition.ai_description.clone().unwrap_or_else(|| {
                "Persist the current payload into a local SQLite table".to_owned()
            });
            Ok(Arc::new(SqlWriterNode::new(
                definition.id.clone(),
                config,
                description,
            )))
        }
        "debugConsole" | "debug/console" => {
            let config: DebugConsoleNodeConfig = serde_json::from_value(definition.config.clone())
                .map_err(|error| EngineError::node_config(definition.id.clone(), error.to_string()))?;
            let description = definition.ai_description.clone().unwrap_or_else(|| {
                "Print the payload to the debug console for inspection".to_owned()
            });
            Ok(Arc::new(DebugConsoleNode::new(
                definition.id.clone(),
                config,
                description,
            )))
        }
        other => Err(EngineError::unsupported_node_type(other)),
    }
}

async fn run_node(
    node: Arc<dyn NodeTrait>,
    timeout: Option<Duration>,
    mut input_rx: mpsc::Receiver<WorkflowContext>,
    downstream_senders: Vec<DownstreamTarget>,
    result_tx: mpsc::Sender<WorkflowContext>,
    event_tx: mpsc::Sender<WorkflowEvent>,
) {
    while let Some(ctx) = input_rx.recv().await {
        let trace_id = ctx.trace_id;
        let node_id = node.id().to_owned();

        emit_workflow_event(
            &event_tx,
            WorkflowEvent::NodeStarted {
                node_id: node_id.clone(),
                trace_id,
            },
        )
        .await;

        let execution = AssertUnwindSafe(node.execute(ctx)).catch_unwind();
        let result = if let Some(timeout) = timeout {
            match tokio::time::timeout(timeout, execution).await {
                Ok(Ok(outcome)) => outcome,
                Ok(Err(_)) => Err(EngineError::StagePanicked {
                    stage: node_id.clone(),
                    trace_id,
                }),
                Err(_) => Err(EngineError::StageTimeout {
                    stage: node_id.clone(),
                    trace_id,
                    timeout_ms: timeout.as_millis(),
                }),
            }
        } else {
            match execution.await {
                Ok(outcome) => outcome,
                Err(_) => Err(EngineError::StagePanicked {
                    stage: node_id.clone(),
                    trace_id,
                }),
            }
        };

        match result {
            Ok(output) => {
                let mut send_error = None;

                for node_output in output.outputs {
                    let matching_targets = match &node_output.dispatch {
                        NodeDispatch::Broadcast => downstream_senders.iter().collect::<Vec<_>>(),
                        NodeDispatch::Route(port_ids) => downstream_senders
                            .iter()
                            .filter(|target| {
                                target
                                    .source_port_id
                                    .as_ref()
                                    .map(|port_id| port_ids.iter().any(|candidate| candidate == port_id))
                                    .unwrap_or(false)
                            })
                            .collect::<Vec<_>>(),
                    };

                    let write_result = if matching_targets.is_empty() {
                        result_tx
                            .send(node_output.ctx)
                            .await
                            .map_err(|_| EngineError::ChannelClosed {
                                stage: node_id.clone(),
                            })
                    } else {
                        let mut downstream_error = None;
                        for target in &matching_targets {
                            if let Err(_) = target.sender.send(node_output.ctx.clone()).await {
                                downstream_error = Some(EngineError::ChannelClosed {
                                    stage: node_id.clone(),
                                });
                                break;
                            }
                        }

                        if let Some(error) = downstream_error {
                            Err(error)
                        } else {
                            Ok(())
                        }
                    };

                    match write_result {
                        Ok(()) => {
                            if matching_targets.is_empty() {
                                emit_workflow_event(
                                    &event_tx,
                                    WorkflowEvent::WorkflowOutput {
                                        node_id: node_id.clone(),
                                        trace_id,
                                    },
                                )
                                .await;
                            }
                        }
                        Err(error) => {
                            send_error = Some(error);
                            break;
                        }
                    }
                }

                if let Some(error) = send_error {
                    emit_workflow_failure(&event_tx, &node_id, trace_id, &error).await;
                    break;
                }

                emit_workflow_event(
                    &event_tx,
                    WorkflowEvent::NodeCompleted {
                        node_id: node_id.clone(),
                        trace_id,
                    },
                )
                .await;
            }
            Err(error) => {
                emit_workflow_failure(&event_tx, &node_id, trace_id, &error).await;
            }
        }
    }
}

async fn emit_workflow_failure(
    event_tx: &mpsc::Sender<WorkflowEvent>,
    node_id: &str,
    trace_id: Uuid,
    error: &EngineError,
) {
    emit_workflow_event(
        event_tx,
        WorkflowEvent::NodeFailed {
            node_id: node_id.to_owned(),
            trace_id,
            error: error.to_string(),
        },
    )
    .await;
}

async fn emit_workflow_event(event_tx: &mpsc::Sender<WorkflowEvent>, event: WorkflowEvent) {
    let _ = event_tx.send(event).await;
}
