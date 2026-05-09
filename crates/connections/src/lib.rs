//! 全局连接资源池。
//!
//! 节点绝不直接访问硬件。所有协议连接（Modbus、MQTT、HTTP 等）
//! 均注册在 [`ConnectionManager`] 中，通过共享的 `Arc<ConnectionManager>`
//! 统一治理连接的建连、重连、心跳、超时、限流、熔断与状态诊断。

use std::{
    any::Any,
    collections::{HashMap, HashSet, VecDeque},
    sync::{Arc, Mutex},
};

use chrono::{DateTime, Utc};
use serde_json::Value;
use tokio::sync::{Mutex as AsyncMutex, RwLock};
use url::Url;

use nazh_core::EngineError;

/// 全局连接池的线程安全句柄。
pub type SharedConnectionManager = Arc<ConnectionManager>;

const SUPPORTED_CONNECTION_TYPES: &[&str] = &[
    "serial", "modbus", "mqtt", "http", "bark", "can", "ethercat",
];

mod types;

pub use types::{
    ConnectionDefinition, ConnectionHealthSnapshot, ConnectionHealthState, ConnectionLease,
    ConnectionRecord, connection_metadata,
};

#[derive(Debug, Clone, PartialEq)]
struct ConnectionGovernancePolicy {
    connect_timeout_ms: u64,
    operation_timeout_ms: u64,
    heartbeat_interval_ms: u64,
    heartbeat_timeout_ms: u64,
    rate_limit_max_attempts: u32,
    rate_limit_window_ms: u64,
    rate_limit_cooldown_ms: u64,
    circuit_failure_threshold: u32,
    circuit_open_ms: u64,
    reconnect_base_ms: u64,
    reconnect_max_ms: u64,
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
    fn from_metadata(metadata: &Value) -> Self {
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

    fn reconnect_delay_ms(&self, attempt: u32) -> u64 {
        let exponent = attempt.saturating_sub(1).min(6);
        let multiplier = 1_u64 << exponent;
        self.reconnect_base_ms
            .saturating_mul(multiplier)
            .min(self.reconnect_max_ms)
    }
}

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

/// 连接借出的 RAII 守卫。
///
/// `Drop` 实现在任何退出路径（正常返回、错误返回、panic 展开）
/// 都会自动释放连接，消除手动归还遗漏的可能性。
///
/// 默认假定操作失败（未显式调用 [`mark_success`](Self::mark_success)
/// 即视为异常退出）。
pub struct ConnectionGuard {
    lease: ConnectionLease,
    record: Arc<Mutex<ConnectionRecord>>,
    outcome: ConnectionOutcome,
}

/// Guard 退出时的结果标记。
enum ConnectionOutcome {
    /// 未明确标记（默认），视为异常退出。
    Pending,
    /// 操作成功。
    Success,
    /// 操作失败，附带原因。
    Failure(String),
}

impl ConnectionGuard {
    /// 返回借出的连接租约信息。
    pub fn lease(&self) -> &ConnectionLease {
        &self.lease
    }

    /// 连接 ID。
    pub fn id(&self) -> &str {
        &self.lease.id
    }

    /// 连接元数据。
    pub fn metadata(&self) -> &Value {
        &self.lease.metadata
    }

    /// 标记本次操作成功。Drop 时会更新连接为 Healthy 状态。
    pub fn mark_success(&mut self) {
        self.outcome = ConnectionOutcome::Success;
    }

    /// 标记本次操作失败。Drop 时会更新连接为 Degraded 状态。
    pub fn mark_failure(&mut self, reason: &str) {
        self.outcome = ConnectionOutcome::Failure(reason.to_owned());
    }
}

impl Drop for ConnectionGuard {
    fn drop(&mut self) {
        let mut record = self
            .record
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        finalize_release(&mut record, self.lease.borrowed_at, &self.outcome);
    }
}

/// 管理具名连接资源池，采用 RAII 排他借出语义。
///
/// 连接通过 [`acquire`](Self::acquire) 借出，返回 [`ConnectionGuard`]，
/// Guard 的 Drop 实现保证在任何退出路径（包括 panic）都自动释放连接。
///
/// 内部使用 `std::sync::Mutex` 以支持同步 Drop 释放。
#[derive(Debug, Default)]
pub struct ConnectionManager {
    connections: RwLock<HashMap<String, Arc<Mutex<ConnectionRecord>>>>,
    attempt_windows: Mutex<HashMap<String, VecDeque<DateTime<Utc>>>>,
    /// 连接级共享会话缓存。key 为 `connection_id`，value 为类型擦除的会话实例。
    shared_sessions: RwLock<HashMap<String, Arc<dyn Any + Send + Sync>>>,
    /// 按连接 ID 合流首次建连，避免并发任务重复打开同一硬件会话。
    shared_session_initializers: Mutex<HashMap<String, Arc<AsyncMutex<()>>>>,
}

/// 创建一个空的 [`ConnectionManager`]，包装在 `Arc<...>` 中。
pub fn shared_connection_manager() -> SharedConnectionManager {
    Arc::new(ConnectionManager::default())
}

impl ConnectionManager {
    /// 注册新连接。若 ID 已存在则返回错误。
    ///
    /// # Errors
    ///
    /// 连接 ID 已存在时返回 [`EngineError::ConnectionAlreadyExists`]。
    pub async fn register_connection(
        &self,
        definition: ConnectionDefinition,
    ) -> Result<(), EngineError> {
        let mut connections = self.connections.write().await;
        if connections.contains_key(&definition.id) {
            return Err(EngineError::ConnectionAlreadyExists(definition.id));
        }

        if let Err(reason) = validate_connection_definition(&definition.kind, &definition.metadata)
        {
            return Err(EngineError::ConnectionInvalidConfiguration {
                connection_id: definition.id,
                reason,
            });
        }

        let id = definition.id.clone();
        connections.insert(id.clone(), Arc::new(Mutex::new(build_record(definition))));
        drop(connections);

        self.reset_attempt_window(&id);
        Ok(())
    }

    /// 插入或替换连接定义（幂等操作）。
    pub async fn upsert_connection(&self, definition: ConnectionDefinition) {
        let id = definition.id.clone();
        if self.has_shared_session(&id).await {
            return;
        }

        let record = build_record(definition);
        let mut connections = self.connections.write().await;
        if connections.get(&id).is_some_and(connection_record_in_use) {
            return;
        }

        connections.insert(id.clone(), Arc::new(Mutex::new(record)));
        drop(connections);

        self.reset_attempt_window(&id);
    }

    /// 批量插入或替换连接定义。
    pub async fn upsert_connections(
        &self,
        definitions: impl IntoIterator<Item = ConnectionDefinition>,
    ) {
        let next_definitions = definitions.into_iter().collect::<Vec<_>>();
        let next_ids = next_definitions
            .iter()
            .map(|definition| definition.id.clone())
            .collect::<Vec<_>>();
        let active_session_ids = self.shared_session_ids().await;

        let mut connections = self.connections.write().await;
        for definition in next_definitions {
            if connections
                .get(&definition.id)
                .is_some_and(connection_record_in_use)
                || active_session_ids.contains(&definition.id)
            {
                continue;
            }

            connections.insert(
                definition.id.clone(),
                Arc::new(Mutex::new(build_record(definition))),
            );
        }
        drop(connections);

        self.reset_attempt_windows(next_ids);
    }

    /// 用给定定义整体替换连接资源池。
    pub async fn replace_connections(
        &self,
        definitions: impl IntoIterator<Item = ConnectionDefinition>,
    ) {
        if self.has_any_shared_session().await {
            return;
        }

        let next_definitions = definitions.into_iter().collect::<Vec<_>>();
        let next_ids = next_definitions
            .iter()
            .map(|definition| definition.id.clone())
            .collect::<Vec<_>>();

        let mut next_connections = HashMap::new();
        for definition in next_definitions {
            next_connections.insert(
                definition.id.clone(),
                Arc::new(Mutex::new(build_record(definition))),
            );
        }

        let mut connections = self.connections.write().await;
        if connections.values().any(connection_record_in_use) {
            return;
        }

        *connections = next_connections;
        drop(connections);

        self.reset_attempt_windows(next_ids);
    }

    /// 按 ID 定位连接的内层 `Arc`，释放外层读锁后返回。
    async fn entry(
        &self,
        connection_id: &str,
    ) -> Result<Arc<Mutex<ConnectionRecord>>, EngineError> {
        let connections = self.connections.read().await;
        connections
            .get(connection_id)
            .cloned()
            .ok_or_else(|| EngineError::ConnectionNotFound(connection_id.to_owned()))
    }

    fn reset_attempt_window(&self, connection_id: &str) {
        let mut attempt_windows = self
            .attempt_windows
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        attempt_windows.insert(connection_id.to_owned(), VecDeque::new());
    }

    fn reset_attempt_windows(&self, connection_ids: Vec<String>) {
        let mut attempt_windows = self
            .attempt_windows
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        *attempt_windows = connection_ids
            .into_iter()
            .map(|connection_id| (connection_id, VecDeque::new()))
            .collect();
    }

    async fn has_shared_session(&self, connection_id: &str) -> bool {
        let sessions = self.shared_sessions.read().await;
        sessions.contains_key(connection_id)
    }

    async fn has_any_shared_session(&self) -> bool {
        let sessions = self.shared_sessions.read().await;
        !sessions.is_empty()
    }

    async fn shared_session_ids(&self) -> HashSet<String> {
        let sessions = self.shared_sessions.read().await;
        sessions.keys().cloned().collect()
    }

    fn initializer_lock(&self, connection_id: &str) -> Arc<AsyncMutex<()>> {
        let mut initializers = self
            .shared_session_initializers
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        initializers
            .entry(connection_id.to_owned())
            .or_insert_with(|| Arc::new(AsyncMutex::new(())))
            .clone()
    }

    fn remove_initializer_lock(&self, connection_id: &str) {
        let mut initializers = self
            .shared_session_initializers
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        initializers.remove(connection_id);
    }

    fn register_attempt(
        &self,
        connection_id: &str,
        policy: &ConnectionGovernancePolicy,
        now: DateTime<Utc>,
    ) -> Result<(), u64> {
        let mut attempt_windows = self
            .attempt_windows
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let attempts = attempt_windows.entry(connection_id.to_owned()).or_default();
        let cutoff = now - duration_ms(policy.rate_limit_window_ms);

        while attempts
            .front()
            .is_some_and(|attempted_at| *attempted_at < cutoff)
        {
            let _ = attempts.pop_front();
        }

        if attempts.len() >= policy.rate_limit_max_attempts as usize {
            return Err(policy.rate_limit_cooldown_ms);
        }

        attempts.push_back(now);
        Ok(())
    }

    /// RAII 方式借出连接，返回 [`ConnectionGuard`]。
    ///
    /// Guard 的 [`Drop`] 实现保证在任何退出路径（包括 panic）自动释放连接，
    /// 根据 [`mark_success`](ConnectionGuard::mark_success) 或
    /// [`mark_failure`](ConnectionGuard::mark_failure) 的调用情况更新连接健康状态。
    ///
    /// # Errors
    ///
    /// 连接不存在、已被借出、配置无效、限流或熔断时返回错误。
    pub async fn acquire(&self, connection_id: &str) -> Result<ConnectionGuard, EngineError> {
        let entry = self.entry(connection_id).await?;
        let lease = {
            let mut record = entry
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            let now = Utc::now();
            let policy = ConnectionGovernancePolicy::from_metadata(&record.metadata);

            reconcile_timed_state(&mut record, &policy, now);

            if record.in_use {
                return Err(EngineError::ConnectionBusy(connection_id.to_owned()));
            }

            if let Err(reason) = validate_connection_definition(&record.kind, &record.metadata) {
                mark_invalid(&mut record, reason.clone(), now);
                return Err(EngineError::ConnectionInvalidConfiguration {
                    connection_id: connection_id.to_owned(),
                    reason,
                });
            }

            if let Some(circuit_open_until) = record.health.circuit_open_until
                && circuit_open_until > now
            {
                return Err(EngineError::ConnectionCircuitOpen {
                    connection_id: connection_id.to_owned(),
                    retry_after_ms: remaining_ms(circuit_open_until, now),
                    reason: record
                        .health
                        .last_failure_reason
                        .clone()
                        .unwrap_or_else(|| "连接仍处于熔断冷却期".to_owned()),
                });
            }

            if let Some(rate_limited_until) = record.health.rate_limited_until
                && rate_limited_until > now
            {
                return Err(EngineError::ConnectionRateLimited {
                    connection_id: connection_id.to_owned(),
                    retry_after_ms: remaining_ms(rate_limited_until, now),
                });
            }

            if let Err(retry_after_ms) = self.register_attempt(connection_id, &policy, now) {
                record.health.rate_limit_hits = record.health.rate_limit_hits.saturating_add(1);
                record.health.rate_limited_until = Some(now + duration_ms(retry_after_ms));
                record.health.phase = ConnectionHealthState::RateLimited;
                record.health.last_state_changed_at = Some(now);
                record.health.last_checked_at = Some(now);
                record.health.diagnosis = Some("短时间内建连次数过多，已进入限流保护".to_owned());
                record.health.recommended_action =
                    Some("等待冷却结束后重试，或降低节点触发频率".to_owned());

                return Err(EngineError::ConnectionRateLimited {
                    connection_id: connection_id.to_owned(),
                    retry_after_ms,
                });
            }

            let phase = if record.health.consecutive_failures > 0 {
                ConnectionHealthState::Reconnecting
            } else {
                ConnectionHealthState::Connecting
            };

            record.in_use = true;
            record.last_borrowed_at = Some(now);
            record.health.phase = phase;
            record.health.last_state_changed_at = Some(now);
            record.health.last_checked_at = Some(now);
            record.health.diagnosis = Some(match phase {
                ConnectionHealthState::Reconnecting => "正在重建连接会话".to_owned(),
                _ => "正在建立连接会话".to_owned(),
            });
            record.health.recommended_action =
                Some("连接已被运行态占用，完成后会自动释放".to_owned());

            ConnectionLease {
                id: record.id.clone(),
                kind: record.kind.clone(),
                metadata: record.metadata.clone(),
                borrowed_at: now,
            }
        };

        Ok(ConnectionGuard {
            lease,
            record: entry,
            outcome: ConnectionOutcome::Pending,
        })
    }

    /// 记录一次真实建连成功。
    ///
    /// # Errors
    ///
    /// 当连接 ID 不存在时返回 `EngineError`。
    pub async fn record_connect_success(
        &self,
        connection_id: &str,
        diagnosis: impl Into<String>,
        latency_ms: Option<u64>,
    ) -> Result<(), EngineError> {
        let entry = self.entry(connection_id).await?;
        let mut record = entry
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let now = Utc::now();

        record.health.phase = ConnectionHealthState::Healthy;
        record.health.last_state_changed_at = Some(now);
        record.health.last_connected_at = Some(now);
        record.health.last_heartbeat_at = Some(now);
        record.health.last_checked_at = Some(now);
        record.health.diagnosis = Some(diagnosis.into());
        record.health.recommended_action = Some("连接运行中，正在等待下一次数据或调度".to_owned());
        record.health.consecutive_failures = 0;
        record.health.reconnect_attempts = 0;
        record.health.next_retry_at = None;
        record.health.circuit_open_until = None;
        record.health.rate_limited_until = None;
        if let Some(latency_ms) = latency_ms {
            record.health.last_latency_ms = Some(latency_ms);
        }

        Ok(())
    }

    /// 记录一次心跳。
    ///
    /// # Errors
    ///
    /// 当连接 ID 不存在时返回 `EngineError`。
    pub async fn record_heartbeat(
        &self,
        connection_id: &str,
        diagnosis: impl Into<String>,
    ) -> Result<(), EngineError> {
        let entry = self.entry(connection_id).await?;
        let mut record = entry
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let now = Utc::now();

        record.health.last_heartbeat_at = Some(now);
        record.health.last_checked_at = Some(now);
        record.health.diagnosis = Some(diagnosis.into());
        if !matches!(
            record.health.phase,
            ConnectionHealthState::Invalid
                | ConnectionHealthState::CircuitOpen
                | ConnectionHealthState::RateLimited
        ) {
            record.health.phase = if record.in_use {
                ConnectionHealthState::Healthy
            } else {
                ConnectionHealthState::Degraded
            };
            record.health.last_state_changed_at = Some(now);
        }

        Ok(())
    }

    /// 记录一次连接失败，并返回建议的重连等待时长。
    ///
    /// # Errors
    ///
    /// 当连接 ID 不存在时返回 `EngineError`。
    pub async fn record_connect_failure(
        &self,
        connection_id: &str,
        reason: &str,
    ) -> Result<u64, EngineError> {
        let entry = self.entry(connection_id).await?;
        let mut record = entry
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let now = Utc::now();
        let policy = ConnectionGovernancePolicy::from_metadata(&record.metadata);
        Ok(apply_runtime_failure(
            &mut record,
            &policy,
            now,
            reason,
            false,
        ))
    }

    /// 记录一次心跳或运行链路超时，并返回建议的重连等待时长。
    ///
    /// # Errors
    ///
    /// 当连接 ID 不存在时返回 `EngineError`。
    pub async fn record_timeout(
        &self,
        connection_id: &str,
        reason: &str,
    ) -> Result<u64, EngineError> {
        let entry = self.entry(connection_id).await?;
        let mut record = entry
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let now = Utc::now();
        let policy = ConnectionGovernancePolicy::from_metadata(&record.metadata);
        Ok(apply_runtime_failure(
            &mut record,
            &policy,
            now,
            reason,
            true,
        ))
    }

    /// 标记连接配置本身无效。
    ///
    /// # Errors
    ///
    /// 当连接 ID 不存在时返回 `EngineError`。
    pub async fn mark_invalid_configuration(
        &self,
        connection_id: &str,
        reason: &str,
    ) -> Result<(), EngineError> {
        let entry = self.entry(connection_id).await?;
        let mut record = entry
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let now = Utc::now();
        mark_invalid(&mut record, reason.to_string(), now);
        Ok(())
    }

    /// 标记连接已断开。
    ///
    /// # Errors
    ///
    /// 当连接 ID 不存在时返回 `EngineError`。
    pub async fn mark_disconnected(
        &self,
        connection_id: &str,
        diagnosis: &str,
    ) -> Result<(), EngineError> {
        let entry = self.entry(connection_id).await?;
        let mut record = entry
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let now = Utc::now();

        record.in_use = false;
        record.health.phase = ConnectionHealthState::Disconnected;
        record.health.last_state_changed_at = Some(now);
        record.health.last_checked_at = Some(now);
        record.health.last_released_at = Some(now);
        record.health.diagnosis = Some(diagnosis.to_string());
        record.health.recommended_action =
            Some("如需恢复，请检查设备在线状态并等待重连".to_owned());

        Ok(())
    }

    /// 将全部连接切回空闲态。
    pub async fn mark_all_idle(&self, diagnosis: impl Into<String>) {
        let diagnosis = diagnosis.into();
        let now = Utc::now();
        let connections = self.connections.read().await;
        let entries = connections.values().cloned().collect::<Vec<_>>();
        drop(connections);

        for entry in entries {
            let mut record = entry
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            record.in_use = false;
            record.health.last_checked_at = Some(now);
            record.health.last_released_at = Some(now);
            record.health.next_retry_at = None;

            if matches!(record.health.phase, ConnectionHealthState::Invalid) {
                record.health.last_state_changed_at = Some(now);
                record.health.diagnosis = Some("连接配置仍无效，运行停止后保留当前诊断".to_owned());
                record.health.recommended_action =
                    Some("请先修正连接配置，再重新部署或测试".to_owned());
                continue;
            }

            if record.health.last_failure_reason.is_some()
                && matches!(
                    record.health.phase,
                    ConnectionHealthState::CircuitOpen
                        | ConnectionHealthState::Disconnected
                        | ConnectionHealthState::RateLimited
                        | ConnectionHealthState::Reconnecting
                        | ConnectionHealthState::Timeout
                )
            {
                record.health.phase = ConnectionHealthState::Degraded;
                record.health.last_state_changed_at = Some(now);
                record.health.diagnosis = Some("运行已停止，已保留最近一次故障诊断".to_owned());
                record.health.recommended_action =
                    Some("可查看失败原因后重新部署，或先执行手动测试连接".to_owned());
                continue;
            }

            record.health.phase = ConnectionHealthState::Idle;
            record.health.last_state_changed_at = Some(now);
            record.health.diagnosis = Some(diagnosis.clone());
            record.health.recommended_action = Some("等待下一次部署或手动测试连接".to_owned());
        }
    }

    /// 返回单个连接记录的快照。
    pub async fn get(&self, connection_id: &str) -> Option<ConnectionRecord> {
        let entry = self.entry(connection_id).await.ok()?;
        let mut record = entry
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let now = Utc::now();
        let policy = ConnectionGovernancePolicy::from_metadata(&record.metadata);
        reconcile_timed_state(&mut record, &policy, now);
        Some(record.clone())
    }

    /// 返回所有已注册连接的快照列表。
    pub async fn list(&self) -> Vec<ConnectionRecord> {
        let connections = self.connections.read().await;
        let entries = connections.values().cloned().collect::<Vec<_>>();
        drop(connections);

        let mut result = Vec::with_capacity(entries.len());
        for entry in entries {
            let mut record = entry
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            let now = Utc::now();
            let policy = ConnectionGovernancePolicy::from_metadata(&record.metadata);
            reconcile_timed_state(&mut record, &policy, now);
            result.push(record.clone());
        }
        result
    }

    /// 获取或创建连接级共享会话。
    ///
    /// 同一 `connection_id` 的多个调用者共享同一会话实例。
    /// 首次调用时执行 `factory` 创建会话，后续调用直接返回缓存。
    ///
    /// `factory` 内部应自行通过 `record_connect_success` / `record_connect_failure`
    /// 报告建连健康状态，不依赖 `ConnectionGuard`（共享会话不使用排他借用）。
    ///
    /// # Errors
    ///
    /// - factory 创建失败时传播底层错误
    pub async fn ensure_shared_session<T: Send + Sync + 'static>(
        &self,
        connection_id: &str,
        factory: impl AsyncFnOnce() -> Result<T, EngineError>,
    ) -> Result<Arc<T>, EngineError> {
        // 快速路径：读锁检查缓存
        {
            let sessions = self.shared_sessions.read().await;
            if let Some(existing) = sessions.get(connection_id) {
                return existing.clone().downcast::<T>().map_err(|_| {
                    EngineError::node_config(
                        connection_id.to_owned(),
                        "共享会话类型不匹配".to_owned(),
                    )
                });
            }
        }

        let initializer = self.initializer_lock(connection_id);
        let _initializer_guard = initializer.lock().await;

        {
            let sessions = self.shared_sessions.read().await;
            if let Some(existing) = sessions.get(connection_id) {
                return existing.clone().downcast::<T>().map_err(|_| {
                    EngineError::node_config(
                        connection_id.to_owned(),
                        "共享会话类型不匹配".to_owned(),
                    )
                });
            }
        }

        // 慢路径：按连接 ID 串行执行 factory，但不占用缓存写锁。
        let session = factory().await?;

        let mut sessions = self.shared_sessions.write().await;
        // double-check：factory 执行期间可能已被其他任务插入
        if let Some(existing) = sessions.get(connection_id) {
            return existing.clone().downcast::<T>().map_err(|_| {
                EngineError::node_config(connection_id.to_owned(), "共享会话类型不匹配".to_owned())
            });
        }

        let arc: Arc<dyn Any + Send + Sync> = Arc::new(session);
        let result = arc.clone().downcast::<T>().map_err(|_| {
            EngineError::node_config(connection_id.to_owned(), "共享会话类型不匹配".to_owned())
        })?;
        sessions.insert(connection_id.to_owned(), arc);
        drop(sessions);

        self.remove_initializer_lock(connection_id);
        Ok(result)
    }

    /// 释放连接级共享会话。
    ///
    /// 从缓存移除会话。会话的 `Drop` 实现负责关闭底层总线和释放硬件资源。
    /// 调用方（节点生命周期守卫）负责在移除前/后更新连接健康状态。
    pub async fn remove_shared_session(&self, connection_id: &str) {
        let mut sessions = self.shared_sessions.write().await;
        sessions.remove(connection_id);
    }

    /// 清理并移除连接级共享会话。
    ///
    /// 从缓存取出会话，调用 `cleanup` 执行协议级关闭，然后从缓存移除。
    /// `cleanup` 闭包接收 downcast 后的会话引用，负责关闭底层总线。
    pub async fn cleanup_and_remove_shared_session<T: Send + Sync + 'static>(
        &self,
        connection_id: &str,
        cleanup: impl FnOnce(&T),
    ) {
        let mut sessions = self.shared_sessions.write().await;
        if let Some(session) = sessions.remove(connection_id)
            && let Ok(concrete) = session.downcast::<T>()
        {
            cleanup(&concrete);
        }
    }
}

/// 释放连接时的共享状态机：超时检测 → 成功/失败/异常退出处理。
///
/// 由 [`ConnectionGuard::drop`] 统一调用。
#[allow(clippy::cast_sign_loss)]
fn finalize_release(
    record: &mut ConnectionRecord,
    borrowed_at: DateTime<Utc>,
    outcome: &ConnectionOutcome,
) {
    let now = Utc::now();
    let policy = ConnectionGovernancePolicy::from_metadata(&record.metadata);
    let elapsed_ms = (now - borrowed_at).num_milliseconds().max(0) as u64;

    record.in_use = false;
    record.health.last_released_at = Some(now);
    record.health.last_checked_at = Some(now);
    record.health.last_latency_ms = Some(elapsed_ms);

    if elapsed_ms > policy.operation_timeout_ms {
        let timeout_reason = format!(
            "连接占用 {elapsed_ms} ms，超过治理超时 {} ms",
            policy.operation_timeout_ms
        );
        let _ = apply_runtime_failure(record, &policy, now, &timeout_reason, true);
        return;
    }

    match outcome {
        ConnectionOutcome::Success => {
            record.health.phase = ConnectionHealthState::Healthy;
            record.health.last_state_changed_at = Some(now);
            record.health.last_connected_at = Some(now);
            record.health.last_heartbeat_at = Some(now);
            record.health.diagnosis = Some(format!("最近一次连接操作完成，用时 {elapsed_ms} ms"));
            record.health.recommended_action = Some("连接空闲，可继续被节点复用".to_owned());
            record.health.consecutive_failures = 0;
            record.health.reconnect_attempts = 0;
            record.health.next_retry_at = None;
            record.health.rate_limited_until = None;
            record.health.circuit_open_until = None;
        }
        ConnectionOutcome::Failure(reason) => {
            let failure_reason = if reason.is_empty() {
                "连接操作失败，节点未提供具体原因"
            } else {
                reason
            };
            if !connection_failure_recorded_during_lease(record, borrowed_at) {
                let _ = apply_runtime_failure(record, &policy, now, failure_reason, false);
            }
        }
        ConnectionOutcome::Pending => {
            record.health.phase = ConnectionHealthState::Degraded;
            record.health.last_state_changed_at = Some(now);
            record.health.diagnosis =
                Some("连接 Guard 未标记结果即被丢弃（可能为 panic 退出）".to_owned());
            record.health.recommended_action =
                Some("检查节点执行路径是否在所有分支都调用了 mark_success/mark_failure".to_owned());
        }
    }
}

fn connection_failure_recorded_during_lease(
    record: &ConnectionRecord,
    borrowed_at: DateTime<Utc>,
) -> bool {
    matches!(
        record.health.phase,
        ConnectionHealthState::Reconnecting
            | ConnectionHealthState::CircuitOpen
            | ConnectionHealthState::Timeout
    ) && record
        .health
        .last_failure_at
        .is_some_and(|last_failure_at| last_failure_at >= borrowed_at)
}

fn build_record(definition: ConnectionDefinition) -> ConnectionRecord {
    let mut record = ConnectionRecord {
        id: definition.id,
        kind: definition.kind,
        metadata: definition.metadata,
        in_use: false,
        last_borrowed_at: None,
        health: ConnectionHealthSnapshot::default(),
    };

    refresh_definition_diagnosis(&mut record);
    record
}

fn connection_record_in_use(entry: &Arc<Mutex<ConnectionRecord>>) -> bool {
    let record = entry
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    record.in_use
}

fn refresh_definition_diagnosis(record: &mut ConnectionRecord) {
    match validate_connection_definition(&record.kind, &record.metadata) {
        Ok(()) => {
            record.health.phase = ConnectionHealthState::Idle;
            record.health.diagnosis = Some("连接配置已就绪，等待建连".to_owned());
            record.health.recommended_action =
                Some("可部署工作流、触发节点或执行测试连接".to_owned());
        }
        Err(reason) => {
            record.health.phase = ConnectionHealthState::Invalid;
            record.health.diagnosis = Some("连接配置缺失或格式无效".to_owned());
            record.health.recommended_action =
                Some("请在连接设置中补齐必填参数后再运行".to_owned());
            record.health.last_failure_reason = Some(reason);
        }
    }
}

#[allow(clippy::cast_sign_loss)]
fn reconcile_timed_state(
    record: &mut ConnectionRecord,
    policy: &ConnectionGovernancePolicy,
    now: DateTime<Utc>,
) {
    if let Some(rate_limited_until) = record.health.rate_limited_until
        && rate_limited_until <= now
    {
        record.health.rate_limited_until = None;
        if matches!(record.health.phase, ConnectionHealthState::RateLimited) {
            record.health.phase = ConnectionHealthState::Idle;
            record.health.last_state_changed_at = Some(now);
            record.health.diagnosis = Some("限流窗口已结束，可再次尝试建连".to_owned());
            record.health.recommended_action =
                Some("如仍然频繁触发，请调整节点节流策略".to_owned());
        }
    }

    if let Some(circuit_open_until) = record.health.circuit_open_until
        && circuit_open_until <= now
    {
        record.health.circuit_open_until = None;
        if matches!(record.health.phase, ConnectionHealthState::CircuitOpen) {
            record.health.phase = ConnectionHealthState::Idle;
            record.health.last_state_changed_at = Some(now);
            record.health.diagnosis = Some("熔断冷却结束，可重新尝试建连".to_owned());
            record.health.recommended_action =
                Some("建议优先检查最近一次失败原因后再重试".to_owned());
        }
    }

    if let Some(next_retry_at) = record.health.next_retry_at
        && next_retry_at <= now
        && matches!(
            record.health.phase,
            ConnectionHealthState::Reconnecting | ConnectionHealthState::Timeout
        )
        && !record.in_use
    {
        record.health.next_retry_at = None;
        record.health.phase = ConnectionHealthState::Idle;
        record.health.last_state_changed_at = Some(now);
        record.health.diagnosis = Some("已结束退避等待，可再次尝试建连".to_owned());
        record.health.recommended_action = Some("若仍失败，请检查目标端是否可达".to_owned());
    }

    if !record.in_use {
        if matches!(record.health.phase, ConnectionHealthState::Connecting) {
            record.health.phase = ConnectionHealthState::Idle;
            record.health.last_state_changed_at = Some(now);
        }

        if let Some(last_heartbeat_at) = record.health.last_heartbeat_at {
            let heartbeat_age_ms = (now - last_heartbeat_at).num_milliseconds().max(0) as u64;
            if heartbeat_age_ms > policy.heartbeat_timeout_ms
                && matches!(record.health.phase, ConnectionHealthState::Healthy)
            {
                record.health.phase = ConnectionHealthState::Degraded;
                record.health.last_state_changed_at = Some(now);
                record.health.diagnosis = Some(format!(
                    "连接心跳已静默 {} ms，超过治理阈值 {} ms",
                    heartbeat_age_ms, policy.heartbeat_timeout_ms
                ));
                record.health.recommended_action =
                    Some("可尝试重连或检查对端设备在线状态".to_owned());
            }
        }
    }
}

fn mark_invalid(record: &mut ConnectionRecord, reason: String, now: DateTime<Utc>) {
    record.in_use = false;
    record.health.phase = ConnectionHealthState::Invalid;
    record.health.last_state_changed_at = Some(now);
    record.health.last_checked_at = Some(now);
    record.health.last_failure_at = Some(now);
    record.health.last_failure_reason = Some(reason);
    record.health.diagnosis = Some("连接配置无效，已拒绝本次建连".to_owned());
    record.health.recommended_action = Some("请先修正连接资源配置再重新部署".to_owned());
}

fn apply_runtime_failure(
    record: &mut ConnectionRecord,
    policy: &ConnectionGovernancePolicy,
    now: DateTime<Utc>,
    reason: &str,
    is_timeout: bool,
) -> u64 {
    record.health.total_failures = record.health.total_failures.saturating_add(1);
    record.health.consecutive_failures = record.health.consecutive_failures.saturating_add(1);
    record.health.reconnect_attempts = record.health.reconnect_attempts.saturating_add(1);
    if is_timeout {
        record.health.timeout_count = record.health.timeout_count.saturating_add(1);
    }

    record.health.last_failure_at = Some(now);
    record.health.last_checked_at = Some(now);
    record.health.last_state_changed_at = Some(now);
    record.health.last_failure_reason = Some(reason.to_string());

    if record.health.consecutive_failures >= policy.circuit_failure_threshold {
        let open_until = now + duration_ms(policy.circuit_open_ms);
        record.health.phase = ConnectionHealthState::CircuitOpen;
        record.health.circuit_open_until = Some(open_until);
        record.health.next_retry_at = Some(open_until);
        record.health.diagnosis = Some("连接连续失败，已进入熔断保护".to_owned());
        record.health.recommended_action =
            Some("请检查目标端可达性、串口占用或参数配置，冷却结束后会再次允许建连".to_owned());
        return policy.circuit_open_ms;
    }

    let retry_after_ms = policy.reconnect_delay_ms(record.health.reconnect_attempts);
    record.health.phase = if is_timeout {
        ConnectionHealthState::Timeout
    } else {
        ConnectionHealthState::Reconnecting
    };
    record.health.next_retry_at = Some(now + duration_ms(retry_after_ms));
    record.health.circuit_open_until = None;
    record.health.diagnosis = Some(if is_timeout {
        "连接心跳或操作超时，准备重连".to_owned()
    } else {
        "连接失败，准备按退避策略重连".to_owned()
    });
    record.health.recommended_action =
        Some("如持续失败，请检查设备在线状态、网络链路或端口占用".to_owned());

    retry_after_ms
}

#[allow(clippy::too_many_lines)]
fn validate_connection_definition(kind: &str, metadata: &Value) -> Result<(), String> {
    let normalized_kind = normalize_connection_kind(kind);
    let metadata = metadata_object(metadata);

    match normalized_kind.as_str() {
        "serial" | "serialport" | "serial_port" | "uart" | "rs232" | "rs485" => {
            let port_path = metadata
                .and_then(|value| value.get("port_path"))
                .and_then(Value::as_str)
                .map(str::trim)
                .unwrap_or_default();
            if port_path.is_empty() {
                return Err("串口连接需要配置 port_path".to_owned());
            }

            let baud_rate = metadata
                .and_then(|value| value.get("baud_rate"))
                .and_then(Value::as_u64)
                .unwrap_or(0);
            if baud_rate == 0 {
                return Err("串口连接需要配置有效的 baud_rate".to_owned());
            }
        }
        "modbus" | "modbus_tcp" => {
            let host = metadata
                .and_then(|value| value.get("host"))
                .and_then(Value::as_str)
                .map(str::trim)
                .unwrap_or_default();
            if host.is_empty() {
                return Err("Modbus 连接需要配置 host".to_owned());
            }

            let port = metadata
                .and_then(|value| value.get("port"))
                .and_then(Value::as_u64)
                .unwrap_or(0);
            if port == 0 || port > u64::from(u16::MAX) {
                return Err("Modbus 连接需要配置 1-65535 之间的 port".to_owned());
            }
        }
        "mqtt" => {
            let host = metadata
                .and_then(|value| value.get("host"))
                .and_then(Value::as_str)
                .map(str::trim)
                .unwrap_or_default();
            if host.is_empty() {
                return Err("MQTT 连接需要配置 host".to_owned());
            }

            let topic = metadata
                .and_then(|value| value.get("topic"))
                .and_then(Value::as_str)
                .map(str::trim)
                .unwrap_or_default();
            if topic.is_empty() {
                return Err("MQTT 连接需要配置 topic".to_owned());
            }
        }
        "http" | "http_sink" => {
            let url = metadata
                .and_then(|value| value.get("url"))
                .and_then(Value::as_str)
                .map(str::trim)
                .unwrap_or_default();
            if url.is_empty() {
                return Err("HTTP 连接需要配置 URL".to_owned());
            }

            Url::parse(url).map_err(|error| format!("HTTP URL 无效: {error}"))?;
        }
        "bark" | "bark_push" => {
            let device_key = metadata
                .and_then(|value| value.get("device_key"))
                .and_then(Value::as_str)
                .map(str::trim)
                .unwrap_or_default();
            if device_key.is_empty() {
                return Err("Bark 连接需要配置 device_key 或完整推送 URL".to_owned());
            }

            let server_url = metadata
                .and_then(|value| value.get("server_url"))
                .and_then(Value::as_str)
                .map(str::trim)
                .unwrap_or_default();
            if !server_url.is_empty() {
                Url::parse(server_url).map_err(|error| format!("Bark server_url 无效: {error}"))?;
            }
        }
        "can" | "can-slcan" | "slcan" => {
            let interface = metadata
                .and_then(|value| value.get("interface"))
                .and_then(Value::as_str)
                .map(str::trim)
                .unwrap_or_default();
            if interface.is_empty() {
                return Err("CAN-SLCAN 连接需要配置 interface（slcan/mock/virtual）".to_owned());
            }
            if !matches!(interface, "slcan" | "mock" | "virtual") {
                return Err(format!("CAN 连接 interface 不支持: {interface}"));
            }

            let channel = metadata
                .and_then(|value| value.get("channel"))
                .and_then(Value::as_str)
                .map(str::trim)
                .unwrap_or_default();
            if channel.is_empty() {
                return Err("CAN-SLCAN 连接需要配置 channel（串口设备路径）".to_owned());
            }

            let baud_rate = metadata
                .and_then(|value| value.get("baud_rate"))
                .and_then(Value::as_u64)
                .unwrap_or(0);
            if baud_rate == 0 {
                return Err("CAN-SLCAN 连接需要配置有效的 baud_rate".to_owned());
            }

            let bitrate = metadata
                .and_then(|value| value.get("bitrate"))
                .and_then(Value::as_u64)
                .unwrap_or(0);
            if bitrate == 0 {
                return Err("CAN-SLCAN 连接需要配置 CAN 总线 bitrate".to_owned());
            }
            if !matches!(
                bitrate,
                10_000
                    | 20_000
                    | 50_000
                    | 100_000
                    | 125_000
                    | 250_000
                    | 500_000
                    | 800_000
                    | 1_000_000
            ) {
                return Err(format!("CAN-SLCAN 连接不支持 bitrate: {bitrate}"));
            }
        }
        "ethercat" | "ethercat-soem" | "ecat" => {
            let backend = metadata
                .and_then(|value| value.get("backend"))
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(str::to_ascii_lowercase)
                .unwrap_or_default();
            if backend.is_empty() {
                return Err("EtherCAT 连接需要配置 backend（ethercrab/mock）".to_owned());
            }
            if !matches!(backend.as_str(), "ethercrab" | "mock") {
                return Err(format!("EtherCAT 连接不支持 backend: {backend}"));
            }

            let interface = metadata
                .and_then(|value| value.get("interface"))
                .and_then(Value::as_str)
                .map(str::trim)
                .unwrap_or_default();
            if interface.is_empty() {
                return Err("EtherCAT 连接需要配置 interface（网络接口名）".to_owned());
            }

            let cycle_time_ms = metadata
                .and_then(|value| value.get("cycle_time_ms"))
                .and_then(Value::as_u64)
                .unwrap_or(0);
            if cycle_time_ms == 0 {
                return Err("EtherCAT 连接 cycle_time_ms 必须大于 0".to_owned());
            }

            let op_timeout_ms = metadata
                .and_then(|value| value.get("op_timeout_ms"))
                .and_then(Value::as_u64)
                .unwrap_or(0);
            if op_timeout_ms == 0 {
                return Err("EtherCAT 连接 op_timeout_ms 必须大于 0".to_owned());
            }
        }
        _ => {
            return Err(format!(
                "不支持的连接类型 `{kind}`；支持类型: {}",
                SUPPORTED_CONNECTION_TYPES.join(", ")
            ));
        }
    }

    Ok(())
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

fn normalize_connection_kind(value: &str) -> String {
    value.trim().to_ascii_lowercase()
}

#[allow(clippy::cast_possible_wrap)]
fn duration_ms(value: u64) -> chrono::Duration {
    chrono::Duration::milliseconds(value.min(i64::MAX as u64) as i64)
}

#[allow(clippy::cast_sign_loss)]
fn remaining_ms(target: DateTime<Utc>, now: DateTime<Utc>) -> u64 {
    (target - now).num_milliseconds().max(0) as u64
}

#[cfg(test)]
mod tests;
