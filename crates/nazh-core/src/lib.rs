//! # Nazh Core
//!
//! Nazh 引擎的 Ring 0 内核，定义工作流运行时的基础类型与原语。
//!
//! 本 crate 不包含任何具体节点实现、脚本引擎或协议驱动，
//! 仅提供引擎运行所需的最小类型集合。

pub mod context;
pub mod error;
pub mod event;
pub mod guard;
pub mod ipc;

pub use context::WorkflowContext;
pub use error::EngineError;
pub use event::ExecutionEvent;
pub use ipc::{
    DeployResponse, DispatchResponse, ListNodeTypesResponse, NodeTypeEntry, UndeployResponse,
};
