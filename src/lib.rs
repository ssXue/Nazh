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

pub mod graph;
mod registry;

pub use nazh_core::{
    ArenaDataStore, CompletedExecutionEvent, ContextRef, DataId, DataStore, DeployResponse,
    DispatchResponse, EngineError, ExecutionEvent, ListNodeTypesResponse, NodeDispatch,
    NodeExecution, NodeOutput, NodeRegistry, NodeTrait, NodeTypeEntry, Plugin, PluginHost,
    PluginManifest, RuntimeResources, SharedResources, UndeployResponse, WorkflowContext,
    WorkflowNodeDefinition, into_payload_map,
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
    BarkPushNode, BarkPushNodeConfig, DebugConsoleNode, DebugConsoleNodeConfig, HttpClientNode,
    HttpClientNodeConfig, IoPlugin, ModbusReadNode, ModbusReadNodeConfig, MqttClientNode,
    MqttClientNodeConfig, NativeNode, NativeNodeConfig, SerialTriggerNode, SerialTriggerNodeConfig,
    SqlWriterNode, SqlWriterNodeConfig, TimerNode, TimerNodeConfig,
};

pub use graph::{
    WorkflowDeployment, WorkflowGraph, WorkflowIngress, WorkflowStreams, deploy_workflow,
    deploy_workflow_with_ai,
};

/// 加载全部标准库插件，返回就绪的节点注册表。
pub fn standard_registry() -> NodeRegistry {
    let mut host = PluginHost::new();
    host.load(&FlowPlugin);
    host.load(&IoPlugin);
    host.into_registry()
}

#[cfg(test)]
mod export_bindings {
    //! ts-rs 类型导出入口，通过 `cargo test --workspace --lib export_bindings` 触发生成。

    use super::*;
    use crate::graph::WorkflowEdge;
    use ts_rs::TS;

    #[test]
    fn export_engine_types() {
        let _ =
            std::fs::create_dir_all(std::env::var("OUT_DIR").unwrap_or_else(|_| "/tmp".to_owned()));
        let _ = WorkflowEdge::export();
        let _ = WorkflowGraph::export();
    }
}
