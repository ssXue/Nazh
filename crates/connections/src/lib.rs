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
use tokio::sync::{Mutex as AsyncMutex, RwLock};

use nazh_core::EngineError;

/// 全局连接池的线程安全句柄。
pub type SharedConnectionManager = Arc<ConnectionManager>;

mod guard;
mod health;
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

        Ok(ConnectionGuard::new(lease, entry))
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
#[cfg(test)]
mod tests;
