use chrono::{DateTime, Utc};

use crate::{
    ConnectionGovernancePolicy, ConnectionHealthState, ConnectionRecord,
    validate_connection_definition,
};

/// Guard 退出时的结果标记。
pub(crate) enum ConnectionOutcome {
    /// 未明确标记（默认），视为异常退出。
    Pending,
    /// 操作成功。
    Success,
    /// 操作失败，附带原因。
    Failure(String),
}

/// 释放连接时的共享状态机：超时检测 -> 成功/失败/异常退出处理。
#[allow(clippy::cast_sign_loss)]
pub(crate) fn finalize_release(
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

pub(crate) fn refresh_definition_diagnosis(record: &mut ConnectionRecord) {
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
pub(crate) fn reconcile_timed_state(
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

pub(crate) fn mark_invalid(record: &mut ConnectionRecord, reason: String, now: DateTime<Utc>) {
    record.in_use = false;
    record.health.phase = ConnectionHealthState::Invalid;
    record.health.last_state_changed_at = Some(now);
    record.health.last_checked_at = Some(now);
    record.health.last_failure_at = Some(now);
    record.health.last_failure_reason = Some(reason);
    record.health.diagnosis = Some("连接配置无效，已拒绝本次建连".to_owned());
    record.health.recommended_action = Some("请先修正连接资源配置再重新部署".to_owned());
}

pub(crate) fn apply_runtime_failure(
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

#[allow(clippy::cast_possible_wrap)]
pub(crate) fn duration_ms(value: u64) -> chrono::Duration {
    chrono::Duration::milliseconds(value.min(i64::MAX as u64) as i64)
}

#[allow(clippy::cast_sign_loss)]
pub(crate) fn remaining_ms(target: DateTime<Utc>, now: DateTime<Utc>) -> u64 {
    (target - now).num_milliseconds().max(0) as u64
}
