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
//! | Ring 0 | `nazh-core` | 内核原语：`NodeTrait`、`DataStore`、`Plugin` 等 |
//! | Ring 1 | `pipeline` | 线性流水线抽象 |
//! | Ring 1 | `connections` | 全局连接资源池 |
//! | Ring 1 | `scripting` | Rhai 脚本引擎基座 |
//! | Ring 1 | `nodes-flow` | 流程控制节点（if/switch/loop/tryCatch/code） |
//! | Ring 1 | `nodes-io` | I/O 节点（native/timer/serial/modbus/http/bark/sql/debug） |
//! | Facade | `nazh-engine`（本 crate） | 组装 Ring 0 + Ring 1，DAG 部署编排 |
//! | IPC | `tauri-bindings` | Tauri 命令请求/响应类型 + ts-rs 导出汇总 |

pub mod graph;
mod registry;

pub use nazh_core::{
    AiCompletionRequest, AiCompletionResponse, AiError, AiGenerationParams, AiMessage,
    AiMessageRole, AiReasoningEffort, AiService, AiThinkingConfig, AiThinkingMode, AiTokenUsage,
    ArenaDataStore, CachedOutput, CancellationToken, CompletedExecutionEvent, ContextRef, DataId,
    DataStore, EngineError, ExecutionEvent, LifecycleGuard, NodeCapabilities, NodeDispatch,
    NodeExecution, NodeHandle, NodeLifecycleContext, NodeOutput, NodeRegistry, NodeTrait,
    OutputCache, PinDefinition, PinDirection, PinKind, PinType, Plugin, PluginHost, PluginManifest,
    RuntimeResources, SharedResources, StreamChunk, VariableDeclaration, WorkflowContext,
    WorkflowNodeDefinition, WorkflowVariables, into_payload_map,
};

pub use connections::{
    ConnectionDefinition, ConnectionGuard, ConnectionLease, ConnectionManager, ConnectionRecord,
    SharedConnectionManager, shared_connection_manager,
};

pub use pipeline::{PipelineHandle, PipelineStage, StageFuture, build_linear_pipeline};

pub use nodes_flow::{
    CodeNode, CodeNodeAiConfig, CodeNodeConfig, FlowPlugin, IfNode, IfNodeConfig, LoopNode,
    LoopNodeConfig, SwitchBranchConfig, SwitchNode, SwitchNodeConfig, TryCatchNode,
    TryCatchNodeConfig,
};

pub use nodes_io::{
    DebugConsoleNode, DebugConsoleNodeConfig, IoPlugin, NativeNode, NativeNodeConfig, TimerNode,
    TimerNodeConfig,
};

#[cfg(feature = "io-notify")]
pub use nodes_io::{BarkPushNode, BarkPushNodeConfig};
#[cfg(feature = "io-http")]
pub use nodes_io::{HttpClientNode, HttpClientNodeConfig};
#[cfg(feature = "io-modbus")]
pub use nodes_io::{ModbusReadNode, ModbusReadNodeConfig};
#[cfg(feature = "io-mqtt")]
pub use nodes_io::{MqttClientNode, MqttClientNodeConfig, MqttMode};
#[cfg(feature = "io-serial")]
pub use nodes_io::{SerialTriggerNode, SerialTriggerNodeConfig};
#[cfg(feature = "io-sql")]
pub use nodes_io::{SqlWriterNode, SqlWriterNodeConfig};

pub use graph::{
    WorkflowDeployment, WorkflowDeploymentParts, WorkflowGraph, WorkflowIngress, WorkflowStreams,
    deploy_workflow, deploy_workflow_with_ai,
};

/// 加载全部标准库插件，返回就绪的节点注册表。
pub fn standard_registry() -> NodeRegistry {
    let mut host = PluginHost::new();
    host.load(&FlowPlugin);
    host.load(&IoPlugin);
    host.into_registry()
}

/// ts-rs 类型导出入口。仅在 `ts-export` feature 启用时编译。
#[cfg(feature = "ts-export")]
pub mod export_bindings {
    use crate::graph::{WorkflowEdge, WorkflowGraph};
    use ts_rs::{ExportError, TS};

    pub fn export_all() -> Result<(), ExportError> {
        WorkflowEdge::export()?;
        WorkflowGraph::export()?;
        Ok(())
    }
}
