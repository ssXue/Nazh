use chrono::Utc;

use nazh_core::EngineError;

use super::ConnectionManager;
use crate::{
    ConnectionGovernancePolicy, ConnectionHealthState, apply_runtime_failure, mark_invalid,
};

impl ConnectionManager {
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
}
