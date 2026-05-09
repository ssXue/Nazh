//! 连接治理策略。
//!
//! 该模块只负责从连接 metadata 中读取治理参数，并提供退避窗口计算；
//! 连接状态机仍由 `lib.rs` 中的 `ConnectionManager` 编排。

use serde_json::Value;

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct ConnectionGovernancePolicy {
    pub(crate) connect_timeout_ms: u64,
    pub(crate) operation_timeout_ms: u64,
    pub(crate) heartbeat_interval_ms: u64,
    pub(crate) heartbeat_timeout_ms: u64,
    pub(crate) rate_limit_max_attempts: u32,
    pub(crate) rate_limit_window_ms: u64,
    pub(crate) rate_limit_cooldown_ms: u64,
    pub(crate) circuit_failure_threshold: u32,
    pub(crate) circuit_open_ms: u64,
    pub(crate) reconnect_base_ms: u64,
    pub(crate) reconnect_max_ms: u64,
}

impl Default for ConnectionGovernancePolicy {
    fn default() -> Self {
        Self {
            connect_timeout_ms: 3_000,
            operation_timeout_ms: 5_000,
            heartbeat_interval_ms: 3_000,
            heartbeat_timeout_ms: 12_000,
            rate_limit_max_attempts: 8,
            rate_limit_window_ms: 10_000,
            rate_limit_cooldown_ms: 4_000,
            circuit_failure_threshold: 3,
            circuit_open_ms: 15_000,
            reconnect_base_ms: 800,
            reconnect_max_ms: 8_000,
        }
    }
}

impl ConnectionGovernancePolicy {
    pub(crate) fn from_metadata(metadata: &Value) -> Self {
        let defaults = Self::default();
        Self {
            connect_timeout_ms: governance_u64(
                metadata,
                "connect_timeout_ms",
                defaults.connect_timeout_ms,
            )
            .max(200),
            operation_timeout_ms: governance_u64(
                metadata,
                "operation_timeout_ms",
                defaults.operation_timeout_ms,
            )
            .max(200),
            heartbeat_interval_ms: governance_u64(
                metadata,
                "heartbeat_interval_ms",
                defaults.heartbeat_interval_ms,
            )
            .max(250),
            heartbeat_timeout_ms: governance_u64(
                metadata,
                "heartbeat_timeout_ms",
                defaults.heartbeat_timeout_ms,
            )
            .max(500),
            rate_limit_max_attempts: governance_u32(
                metadata,
                "rate_limit_max_attempts",
                defaults.rate_limit_max_attempts,
            )
            .max(1),
            rate_limit_window_ms: governance_u64(
                metadata,
                "rate_limit_window_ms",
                defaults.rate_limit_window_ms,
            )
            .max(500),
            rate_limit_cooldown_ms: governance_u64(
                metadata,
                "rate_limit_cooldown_ms",
                defaults.rate_limit_cooldown_ms,
            )
            .max(500),
            circuit_failure_threshold: governance_u32(
                metadata,
                "circuit_failure_threshold",
                defaults.circuit_failure_threshold,
            )
            .max(1),
            circuit_open_ms: governance_u64(metadata, "circuit_open_ms", defaults.circuit_open_ms)
                .max(1_000),
            reconnect_base_ms: governance_u64(
                metadata,
                "reconnect_base_ms",
                defaults.reconnect_base_ms,
            )
            .max(200),
            reconnect_max_ms: governance_u64(
                metadata,
                "reconnect_max_ms",
                defaults.reconnect_max_ms,
            )
            .max(500),
        }
    }

    pub(crate) fn reconnect_delay_ms(&self, attempt: u32) -> u64 {
        let exponent = attempt.saturating_sub(1).min(6);
        let multiplier = 1_u64 << exponent;
        self.reconnect_base_ms
            .saturating_mul(multiplier)
            .min(self.reconnect_max_ms)
    }
}

fn governance_u64(metadata: &Value, key: &str, fallback: u64) -> u64 {
    governance_value(metadata, key)
        .and_then(Value::as_u64)
        .unwrap_or(fallback)
}

fn governance_u32(metadata: &Value, key: &str, fallback: u32) -> u32 {
    governance_value(metadata, key)
        .and_then(Value::as_u64)
        .and_then(|value| u32::try_from(value).ok())
        .unwrap_or(fallback)
}

fn governance_value<'a>(metadata: &'a Value, key: &str) -> Option<&'a Value> {
    metadata_object(metadata)
        .and_then(|value| value.get("governance"))
        .and_then(metadata_object)
        .and_then(|governance| governance.get(key))
}

fn metadata_object(value: &Value) -> Option<&serde_json::Map<String, Value>> {
    value.as_object()
}
