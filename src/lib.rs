//! # Nazh Engine
//!
//! 面向工业边缘场景的 DAG 工作流编排运行时。
//!
//! 引擎接收前端导出的 JSON 工作流图（AST），校验其为有向无环图后，
//! 将每个节点部署为独立的 Tokio 异步任务，节点间通过 MPSC 通道通信。
//! 支持两类节点：
//!
//! - **原生节点（Native Node）** — 纯 Rust 实现的协议 I/O 与数据注入逻辑。
//! - **脚本节点（Rhai Node）** — 沙箱化的用户脚本，带执行步数上限。
//!
//! 所有硬件访问通过全局 [`ConnectionManager`] 中介，
//! 每次节点执行均受超时保护与 panic 隔离，保证运行时绝不崩溃。

pub mod connection;
pub mod context;
pub mod error;
pub mod graph;
pub mod nodes;
pub mod pipeline;

pub use connection::{
    shared_connection_manager, ConnectionDefinition, ConnectionLease, ConnectionManager,
    ConnectionRecord, SharedConnectionManager,
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
