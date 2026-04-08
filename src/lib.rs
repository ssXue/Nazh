pub mod connection;
pub mod context;
pub mod error;
pub mod graph;
pub mod nodes;
pub mod pipeline;

pub use connection::{
    shared_connection_manager, ConnectionDefinition, ConnectionLease, ConnectionManager,
    ConnectionRecord,
    SharedConnectionManager,
};
pub use context::WorkflowContext;
pub use error::EngineError;
pub use graph::{
    deploy_workflow, WorkflowDeployment, WorkflowEvent, WorkflowGraph, WorkflowIngress,
    WorkflowNodeDefinition, WorkflowStreams,
};
pub use nodes::{
    DebugConsoleNode, DebugConsoleNodeConfig, HttpClientNode, HttpClientNodeConfig, IfNode,
    IfNodeConfig, LoopNode, LoopNodeConfig, ModbusReadNode, ModbusReadNodeConfig, NativeNode,
    NativeNodeConfig, NodeDispatch, NodeExecution, NodeTrait, RhaiNode, RhaiNodeConfig,
    SqlWriterNode, SqlWriterNodeConfig, SwitchBranchConfig, SwitchNode, SwitchNodeConfig,
    TimerNode, TimerNodeConfig, TryCatchNode, TryCatchNodeConfig,
};
pub use pipeline::{
    build_linear_pipeline, PipelineEvent, PipelineHandle, PipelineStage, StageFuture,
};
