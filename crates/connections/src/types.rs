use chrono::{DateTime, Utc};
use nazh_core::EngineError;
use serde::{Deserialize, Deserializer, Serialize};
use serde_json::Value;
#[cfg(feature = "ts-export")]
use ts_rs::TS;

/// 连接资源的声明式定义（用于全局连接资源库）。
#[derive(Debug, Clone, Serialize, PartialEq)]
#[cfg_attr(feature = "ts-export", derive(TS), ts(export))]
pub struct ConnectionDefinition {
    pub id: String,
    #[serde(rename = "type")]
    pub kind: String,
    #[serde(default)]
    pub metadata: Value,
}

impl<'de> Deserialize<'de> for ConnectionDefinition {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        struct ConnectionDefinitionInput {
            id: String,
            #[serde(rename = "type", alias = "kind")]
            kind: String,
            #[serde(default)]
            metadata: Value,
        }

        let input = ConnectionDefinitionInput::deserialize(deserializer)?;
        Ok(Self {
            id: input.id,
            kind: input.kind,
            metadata: input.metadata,
        })
    }
}

/// 由 [`ConnectionManager::borrow`] 返回的临时借出连接句柄。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ConnectionLease {
    pub id: String,
    pub kind: String,
    pub metadata: Value,
    pub borrowed_at: DateTime<Utc>,
}

/// 将连接租约序列化为元数据键值对 `("connection", Value)`。
pub fn connection_metadata(
    node_id: &str,
    lease: &ConnectionLease,
) -> Result<(String, serde_json::Value), EngineError> {
    let value = serde_json::to_value(lease)
        .map_err(|error| EngineError::payload_conversion(node_id.to_owned(), error.to_string()))?;
    Ok(("connection".to_owned(), value))
}

/// 连接健康阶段。
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "ts-export", derive(TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub enum ConnectionHealthState {
    Idle,
    Connecting,
    Healthy,
    Degraded,
    Invalid,
    Reconnecting,
    RateLimited,
    CircuitOpen,
    Timeout,
    Disconnected,
}

/// 连接治理运行时快照。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[cfg_attr(feature = "ts-export", derive(TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct ConnectionHealthSnapshot {
    pub phase: ConnectionHealthState,
    #[cfg_attr(feature = "ts-export", ts(optional))]
    pub diagnosis: Option<String>,
    #[cfg_attr(feature = "ts-export", ts(optional))]
    pub recommended_action: Option<String>,
    #[cfg_attr(feature = "ts-export", ts(optional))]
    pub last_state_changed_at: Option<DateTime<Utc>>,
    #[cfg_attr(feature = "ts-export", ts(optional))]
    pub last_connected_at: Option<DateTime<Utc>>,
    #[cfg_attr(feature = "ts-export", ts(optional))]
    pub last_heartbeat_at: Option<DateTime<Utc>>,
    #[cfg_attr(feature = "ts-export", ts(optional))]
    pub last_checked_at: Option<DateTime<Utc>>,
    #[cfg_attr(feature = "ts-export", ts(optional))]
    pub last_released_at: Option<DateTime<Utc>>,
    #[cfg_attr(feature = "ts-export", ts(optional))]
    pub last_failure_at: Option<DateTime<Utc>>,
    #[cfg_attr(feature = "ts-export", ts(optional))]
    pub last_failure_reason: Option<String>,
    #[cfg_attr(feature = "ts-export", ts(optional))]
    pub next_retry_at: Option<DateTime<Utc>>,
    #[cfg_attr(feature = "ts-export", ts(optional))]
    pub circuit_open_until: Option<DateTime<Utc>>,
    #[cfg_attr(feature = "ts-export", ts(optional))]
    pub rate_limited_until: Option<DateTime<Utc>>,
    pub consecutive_failures: u32,
    pub total_failures: u32,
    pub timeout_count: u32,
    pub rate_limit_hits: u32,
    pub reconnect_attempts: u32,
    #[cfg_attr(feature = "ts-export", ts(optional))]
    pub last_latency_ms: Option<u64>,
}

impl Default for ConnectionHealthSnapshot {
    fn default() -> Self {
        Self {
            phase: ConnectionHealthState::Idle,
            diagnosis: Some("连接配置已加载，等待建连".to_owned()),
            recommended_action: Some("可部署工作流或触发测试连接以建立会话".to_owned()),
            last_state_changed_at: None,
            last_connected_at: None,
            last_heartbeat_at: None,
            last_checked_at: None,
            last_released_at: None,
            last_failure_at: None,
            last_failure_reason: None,
            next_retry_at: None,
            circuit_open_until: None,
            rate_limited_until: None,
            consecutive_failures: 0,
            total_failures: 0,
            timeout_count: 0,
            rate_limit_hits: 0,
            reconnect_attempts: 0,
            last_latency_ms: None,
        }
    }
}

/// 已注册连接的内部记录，追踪其借出状态与治理健康信息。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[cfg_attr(feature = "ts-export", derive(TS), ts(export))]
pub struct ConnectionRecord {
    pub id: String,
    pub kind: String,
    pub metadata: Value,
    pub in_use: bool,
    #[cfg_attr(feature = "ts-export", ts(optional))]
    pub last_borrowed_at: Option<DateTime<Utc>>,
    #[serde(default)]
    pub health: ConnectionHealthSnapshot,
}
