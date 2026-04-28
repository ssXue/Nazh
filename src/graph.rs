//! 基于 DAG 的工作流图：解析、校验与异步部署。
//!
//! 本模块将前端画布导出的 JSON AST 解析为 [`WorkflowGraph`]，
//! 校验其为有向无环图后，通过 [`deploy_workflow`] / [`deploy_workflow_with_ai`]
//! 将每个节点实例化为
//! Tokio 任务，并通过 MPSC 通道连接。
//!
//! | 子模块 | 职责 |
//! |--------|------|
//! | `types` | 所有数据结构定义与句柄方法 |
//! | `topology` | DAG 校验、拓扑排序（Kahn 算法） |
//! | `deploy` | 工作流部署编排 |
//! | `runner` | 单节点异步执行循环与事件发射 |
//! | `variables_init` | 部署期变量声明 → `Arc<WorkflowVariables>` 初始化器（ADR-0012）|

mod deploy;
mod pin_validator;
mod runner;
mod topology;
pub(crate) mod types;
mod variables_init;

/// 默认输入引脚 id——节点单输入约定（[`PinDefinition::default_input`](nazh_core::PinDefinition::default_input)）。
/// `WorkflowEdge.target_port_id == None` 时回落到此值。
pub(crate) const DEFAULT_INPUT_PIN_ID: &str = "in";
/// 默认输出引脚 id——节点单输出约定（[`PinDefinition::default_output`](nazh_core::PinDefinition::default_output)）。
/// `WorkflowEdge.source_port_id == None` 时回落到此值。
pub(crate) const DEFAULT_OUTPUT_PIN_ID: &str = "out";

pub use deploy::{deploy_workflow, deploy_workflow_with_ai};
pub use types::{
    WorkflowDeployment, WorkflowDeploymentParts, WorkflowEdge, WorkflowGraph, WorkflowIngress,
    WorkflowStreams,
};
pub use variables_init::build_workflow_variables;
