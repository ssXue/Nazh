//! # Nazh Core
//!
//! Nazh 引擎的 Ring 0 内核，定义工作流运行时的基础类型与原语。
//!
//! 本 crate 不包含任何具体节点实现、脚本引擎或协议驱动，
//! 仅提供引擎运行所需的最小类型集合。

pub mod context;
pub mod data;
pub mod error;
pub mod event;
pub mod guard;
pub mod ipc;
pub mod node;
pub mod plugin;

pub use context::{ContextRef, WorkflowContext};
pub use data::{ArenaDataStore, DataId, DataStore};
pub use error::EngineError;
pub use event::ExecutionEvent;
pub use event::CompletedExecutionEvent;
pub use ipc::{
    DeployResponse, DispatchResponse, ListNodeTypesResponse, NodeTypeEntry, UndeployResponse,
};
pub use node::{NodeDispatch, NodeExecution, NodeOutput, NodeTrait, into_payload_map};
pub use plugin::{
    NodeRegistry, Plugin, PluginHost, PluginManifest, RuntimeResources, SharedResources,
    WorkflowNodeDefinition,
};
pub use uuid::Uuid;

#[cfg(test)]
mod export_bindings {
    //! ts-rs 类型导出入口，通过 `cargo test --workspace --lib export_bindings` 触发生成。

    use super::*;
    use ts_rs::TS;

    #[test]
    fn export_core_types() {
        let _ =
            std::fs::create_dir_all(std::env::var("OUT_DIR").unwrap_or_else(|_| "/tmp".to_owned()));
        let _ = CompletedExecutionEvent::export();
        let _ = DeployResponse::export();
        let _ = DispatchResponse::export();
        let _ = ExecutionEvent::export();
        let _ = ListNodeTypesResponse::export();
        let _ = NodeTypeEntry::export();
        let _ = UndeployResponse::export();
        let _ = WorkflowContext::export();
        let _ = WorkflowNodeDefinition::export();
    }
}
