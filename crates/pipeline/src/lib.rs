//! 线性流水线抽象，用于顺序阶段执行。
//!
//! [`build_linear_pipeline`] 将一系列 [`PipelineStage`] 串联为 Tokio 驱动的流水线，
//! 每个阶段具备独立的超时保护和基于 `catch_unwind` 的 panic 隔离。
//!
//! | 子模块 | 职责 |
//! |--------|------|
//! | [`types`] | 阶段、事件、句柄定义与流水线构建 |
//! | [`runner`] | 单阶段异步执行循环与事件发射 |

mod runner;
mod types;

pub use types::{build_linear_pipeline, PipelineHandle, PipelineStage, StageFuture};
