//! 全局连接资源池。
//!
//! 节点绝不直接访问硬件。所有协议连接（Modbus、MQTT、HTTP 等）
//! 均注册在 [`ConnectionManager`] 中，通过共享的 `Arc<ConnectionManager>`
//! 统一治理连接的建连、重连、心跳、超时、限流、熔断与状态诊断。

use std::sync::Arc;

/// 全局连接池的线程安全句柄。
pub type SharedConnectionManager = Arc<ConnectionManager>;

mod guard;
mod health;
mod manager;
mod policy;
mod types;
mod validation;

use health::{
    apply_runtime_failure, duration_ms, mark_invalid, reconcile_timed_state,
    refresh_definition_diagnosis, remaining_ms,
};
use policy::ConnectionGovernancePolicy;
use validation::validate_connection_definition;

pub use guard::ConnectionGuard;
pub use manager::{ConnectionManager, shared_connection_manager};
pub use types::{
    ConnectionDefinition, ConnectionHealthSnapshot, ConnectionHealthState, ConnectionLease,
    ConnectionRecord, connection_metadata,
};

/// ts-rs 类型导出入口。仅在 `ts-export` feature 启用时编译。
#[cfg(feature = "ts-export")]
pub mod export_bindings {
    use super::{
        ConnectionDefinition, ConnectionHealthSnapshot, ConnectionHealthState, ConnectionRecord,
    };
    use ts_rs::{Config, ExportError, TS};

    pub fn export_all() -> Result<(), ExportError> {
        let cfg = Config::from_env();

        ConnectionDefinition::export(&cfg)?;
        ConnectionHealthSnapshot::export(&cfg)?;
        ConnectionHealthState::export(&cfg)?;
        ConnectionRecord::export(&cfg)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests;
