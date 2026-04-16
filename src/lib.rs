//! # Nazh Engine
//!
//! 面向工业边缘场景的 DAG 工作流编排运行时。
//!
//! 引擎接收前端导出的 JSON 工作流图（AST），校验其为有向无环图后，
//! 将每个节点部署为独立的 Tokio 异步任务，节点间通过 MPSC 通道通信。
//!
//! ## 分层架构
//!
//! | 层级 | Crate | 职责 |
//! |------|-------|------|
//! | Ring 0 | `nazh-core` | 内核原语：`NodeTrait`、`DataStore`、`ContextRef` 等 |
//! | Ring 1 | `nazh-pipeline` | 线性流水线抽象 |
//! | Ring 1 | `nazh-connections` | 全局连接资源池 |
//! | Ring 1 | `nazh-scripting` | Rhai 脚本引擎基座 |
//! | Ring 1 | `nazh-nodes-flow` | 流程控制节点（if/switch/loop/tryCatch/rhai） |
//! | Ring 1 | `nazh-nodes-io` | I/O 节点（native/timer/serial/modbus/http/sql/debug） |
//! | Facade | `nazh-engine`（本 crate） | 组装 Ring 0 + Ring 1，DAG 部署编排 |

pub mod graph;
pub mod registry;

pub use nazh_core::{
    into_payload_map, ArenaDataStore, ContextRef, DataId, DataStore, DeployResponse,
    DispatchResponse, EngineError, ExecutionEvent, ListNodeTypesResponse, NodeDispatch,
    NodeExecution, NodeOutput, NodeTrait, NodeTypeEntry, UndeployResponse, WorkflowContext,
};

pub use connections::{
    shared_connection_manager, ConnectionDefinition, ConnectionGuard, ConnectionLease,
    ConnectionManager, ConnectionRecord, SharedConnectionManager,
};

pub use pipeline::{build_linear_pipeline, PipelineHandle, PipelineStage, StageFuture};

pub use nodes_flow::{
    IfNode, IfNodeConfig, LoopNode, LoopNodeConfig, RhaiNode, RhaiNodeConfig, SwitchBranchConfig,
    SwitchNode, SwitchNodeConfig, TryCatchNode, TryCatchNodeConfig,
};

pub use nodes_io::{
    DebugConsoleNode, DebugConsoleNodeConfig, HttpClientNode, HttpClientNodeConfig, ModbusReadNode,
    ModbusReadNodeConfig, NativeNode, NativeNodeConfig, SerialTriggerNode,
    SerialTriggerNodeConfig, SqlWriterNode, SqlWriterNodeConfig, TimerNode, TimerNodeConfig,
};

pub use graph::{
    deploy_workflow, WorkflowDeployment, WorkflowGraph, WorkflowIngress, WorkflowNodeDefinition,
    WorkflowStreams,
};
pub use registry::NodeRegistry;
