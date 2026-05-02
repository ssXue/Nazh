//! Human-in-the-Loop 审批节点模块。

pub mod config;
pub mod form;
pub mod node;
pub mod registry;
pub mod workflow_id;

pub use config::HumanLoopNodeConfig;
pub use node::HumanLoopNode;
pub use registry::ApprovalRegistry;
#[allow(unused_imports)] // facade + tauri shell 使用
pub use registry::{HumanLoopResponse, PendingApprovalSummary, ResponseAction};
#[allow(unused_imports)] // facade + tauri shell 使用
pub use workflow_id::WorkflowId;
