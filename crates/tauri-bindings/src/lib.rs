//! Nazh Tauri 命令的请求/响应类型集中地。
//!
//! 这些类型只服务于 Tauri 桌面壳层与前端的 IPC 契约，不属于引擎运行时；
//! 因此从 Ring 0（`nazh-core`）迁出，独立成一个 crate。详见 ADR-0017。
//!
//! `ts-rs` 通过 `ts-export` feature 启用，CI 用
//! `cargo test -p tauri-bindings --features ts-export export_bindings`
//! 触发本 crate 与所有依赖 crate 的 TypeScript 类型导出。

mod copilot;
mod deployment_session;
mod export;
mod observability;
mod runtime;
mod serial;
mod variables;
mod workflow;

pub use copilot::*;
pub use deployment_session::*;
#[cfg(feature = "ts-export")]
pub use export::export_all;
pub use observability::*;
pub use runtime::*;
pub use serial::*;
pub use variables::*;
pub use workflow::*;

#[cfg(test)]
#[allow(clippy::unwrap_used)]
#[path = "tests.rs"]
mod tests;
