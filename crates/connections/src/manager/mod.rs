use std::{
    any::Any,
    collections::{HashMap, VecDeque},
    sync::{Arc, Mutex},
};

use chrono::{DateTime, Utc};
use tokio::sync::{Mutex as AsyncMutex, RwLock};

use nazh_core::EngineError;

use crate::{
    ConnectionDefinition, ConnectionGovernancePolicy, ConnectionGuard, ConnectionHealthSnapshot,
    ConnectionHealthState, ConnectionLease, ConnectionRecord, SharedConnectionManager, duration_ms,
    mark_invalid, reconcile_timed_state, refresh_definition_diagnosis, remaining_ms,
    validate_connection_definition,
};

mod health_ops;
mod session;

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
