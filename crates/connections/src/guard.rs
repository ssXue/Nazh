use std::sync::{Arc, Mutex};

use serde_json::Value;

use crate::{
    ConnectionLease, ConnectionRecord,
    health::{ConnectionOutcome, finalize_release},
};

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

impl ConnectionGuard {
    pub(crate) fn new(lease: ConnectionLease, record: Arc<Mutex<ConnectionRecord>>) -> Self {
        Self {
            lease,
            record,
            outcome: ConnectionOutcome::Pending,
        }
    }

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
