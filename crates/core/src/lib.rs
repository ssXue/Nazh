//! # Nazh Core
//!
//! Nazh 引擎的 Ring 0 内核，定义工作流运行时的基础类型与原语。
//!
//! 本 crate 不包含任何具体节点实现、脚本引擎或协议驱动，
//! 仅提供引擎运行所需的最小类型集合。
//!
//! Tauri IPC 命令的请求/响应类型已迁出至 `tauri-bindings` crate，
//! `ts-rs` 由 `ts-export` feature 按需启用。详见 ADR-0017。

pub mod context;
pub mod data;
pub mod error;
pub mod event;
pub mod guard;
pub mod node;
pub mod plugin;

pub use context::{ContextRef, WorkflowContext};
pub use data::{ArenaDataStore, DataId, DataStore};
pub use error::EngineError;
pub use event::CompletedExecutionEvent;
pub use event::ExecutionEvent;
pub use node::{
    NodeCapabilities, NodeDispatch, NodeExecution, NodeOutput, NodeTrait, into_payload_map,
};
pub use plugin::{
    NodeRegistry, Plugin, PluginHost, PluginManifest, RuntimeResources, SharedResources,
    WorkflowNodeDefinition,
};
pub use uuid::Uuid;

/// ts-rs 类型导出入口。仅在 `ts-export` feature 启用时编译。
///
/// CI 通过 `tauri_bindings::export_all()` 间接调用本模块的 `export_all()`。
#[cfg(feature = "ts-export")]
pub mod export_bindings {
    use super::{CompletedExecutionEvent, ExecutionEvent, WorkflowContext, WorkflowNodeDefinition};
    use ts_rs::{ExportError, TS};

    /// 导出本 crate 的所有 ts-rs 类型到 `web/src/generated/`。
    pub fn export_all() -> Result<(), ExportError> {
        CompletedExecutionEvent::export()?;
        ExecutionEvent::export()?;
        WorkflowContext::export()?;
        WorkflowNodeDefinition::export()?;
        Ok(())
    }
}
