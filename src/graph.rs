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

mod deploy;
mod pin_validator;
mod runner;
mod topology;
pub(crate) mod types;

pub use deploy::{deploy_workflow, deploy_workflow_with_ai};
pub use types::{
    WorkflowDeployment, WorkflowDeploymentParts, WorkflowEdge, WorkflowGraph, WorkflowIngress,
    WorkflowStreams,
};
