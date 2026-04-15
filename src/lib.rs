//! # Nazh Engine
//!
//! 面向工业边缘场景的 DAG 工作流编排运行时。
//!
//! 引擎接收前端导出的 JSON 工作流图（AST），校验其为有向无环图后，
//! 将每个节点部署为独立的 Tokio 异步任务，节点间通过 MPSC 通道通信。
//!
//! ## 模块结构
//!
//! | 模块 | 职责 |
//! |------|------|
//! | [`nodes`] | 节点 Trait 与全部节点实现（每种节点一个文件） |
//! | [`graph`] | DAG 解析、校验、部署、节点工厂、运行循环 |
//! | [`pipeline`] | 线性流水线抽象（顺序阶段执行） |
//! | [`connection`] | 全局连接资源池（借出/归还语义） |
//! | [`context`] | 在节点间流转的数据信封 |
//! | [`event`] | 统一执行生命周期事件 |
//! | [`ipc`] | Tauri IPC 响应类型（ts-rs 自动导出至前端） |
//! | [`error`] | 引擎统一错误类型 |
//!
//! 所有硬件访问通过全局 [`ConnectionManager`] 中介，
//! 每次节点执行均受超时保护与 panic 隔离，保证运行时绝不崩溃。

pub mod connection;
pub mod context;
pub mod error;
pub mod event;
pub mod graph;
mod guard;
pub mod ipc;
pub mod nodes;
pub mod pipeline;
pub mod registry;

pub use connection::{
    shared_connection_manager, ConnectionDefinition, ConnectionLease, ConnectionManager,
    ConnectionRecord, SharedConnectionManager,
};
pub use context::WorkflowContext;
pub use error::EngineError;
pub use event::ExecutionEvent;
pub use graph::{
    deploy_workflow, WorkflowDeployment, WorkflowGraph, WorkflowIngress, WorkflowNodeDefinition,
    WorkflowStreams,
};
pub use ipc::{DeployResponse, DispatchResponse, UndeployResponse};
pub use nodes::{
    DebugConsoleNode, DebugConsoleNodeConfig, HttpClientNode, HttpClientNodeConfig, IfNode,
    IfNodeConfig, LoopNode, LoopNodeConfig, ModbusReadNode, ModbusReadNodeConfig, NativeNode,
    NativeNodeConfig, NodeDispatch, NodeExecution, NodeTrait, RhaiNode, RhaiNodeConfig,
    SerialTriggerNode, SerialTriggerNodeConfig, SqlWriterNode, SqlWriterNodeConfig,
    SwitchBranchConfig, SwitchNode, SwitchNodeConfig, TimerNode, TimerNodeConfig, TryCatchNode,
    TryCatchNodeConfig,
};
pub use pipeline::{build_linear_pipeline, PipelineHandle, PipelineStage, StageFuture};
pub use registry::NodeRegistry;
