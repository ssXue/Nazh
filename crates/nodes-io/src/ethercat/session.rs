//! EtherCAT 主站连接级共享会话。
//!
//! 同一 `connection_id` 的所有 EtherCAT 节点共享一个主站实例，
//! 会话生命周期跟随工程部署/撤销，而非跟随单个节点。

use std::sync::Arc;

use connections::{ConnectionLease, SharedConnectionManager};
use nazh_core::{EngineError, LifecycleGuard, NodeLifecycleContext};
use tokio::sync::{Mutex, MutexGuard};

use crate::ethercat::{EthercatBus, EthercatConfig};

use super::backends::create_ethercat_bus;

/// 连接级共享 EtherCAT 主站会话。
///
/// 内部用 `Mutex<Option<...>>` 包裹总线实例，支持多个节点并发访问，
/// 并在 `cleanup` 时安全取出总线执行关闭。
#[allow(dead_code)]
pub struct SharedEthercatSession {
    bus: Mutex<Option<Box<dyn EthercatBus>>>,
    connection_id: String,
    channel_info: String,
    simulated: bool,
    lease: Option<ConnectionLease>,
}

impl SharedEthercatSession {
    /// 获取总线引用。若已有节点正在访问同一主站，则排队等待。
    pub async fn bus(
        &self,
        node_id: &str,
    ) -> Result<MutexGuard<'_, Option<Box<dyn EthercatBus>>>, EngineError> {
        let guard = self.bus.lock().await;
        if guard.is_some() {
            Ok(guard)
        } else {
            Err(EngineError::node_config(
                node_id.to_owned(),
                "EtherCAT 总线会话已释放".to_owned(),
            ))
        }
    }

    #[allow(dead_code)]
    pub fn simulated(&self) -> bool {
        self.simulated
    }

    #[allow(dead_code)]
    pub fn channel_info(&self) -> &str {
        &self.channel_info
    }

    #[allow(dead_code)]
    pub fn connection_id(&self) -> &str {
        &self.connection_id
    }

    #[allow(dead_code)]
    pub fn lease(&self) -> Option<&ConnectionLease> {
        self.lease.as_ref()
    }

    /// 关闭主站并释放硬件资源。
    pub fn cleanup(&self) {
        if let Ok(mut guard) = self.bus.try_lock()
            && let Some(bus) = guard.take()
            && let Err(error) = bus.shutdown()
        {
            tracing::warn!(?error, "EtherCAT 主站会话清理失败");
        }
    }
}

/// EtherCAT 总线运行期操作句柄。
///
/// 封装连接管理器和连接 ID，提供 `ensure_session` 接口
/// 获取或创建共享会话。轻量级，可按需创建。
pub struct EthercatRuntime {
    connection_manager: SharedConnectionManager,
    connection_id: String,
}

impl EthercatRuntime {
    pub fn new(connection_manager: SharedConnectionManager, connection_id: String) -> Self {
        Self {
            connection_manager,
            connection_id,
        }
    }

    /// 获取或创建当前连接的共享 EtherCAT 会话。
    ///
    /// 多个节点共享同一 `connection_id` 时返回同一会话实例。
    /// 首次调用时执行建连，后续调用直接返回缓存。
    pub async fn ensure_session(
        &self,
        node_id: &str,
    ) -> Result<Arc<SharedEthercatSession>, EngineError> {
        let conn_id = self.connection_id.clone();
        let cm = self.connection_manager.clone();
        let node_id = node_id.to_owned();

        self.connection_manager
            .ensure_shared_session::<SharedEthercatSession>(&self.connection_id, async || {
                open_shared_session(&node_id, &conn_id, &cm).await
            })
            .await
    }

    /// 关闭共享会话并从连接管理器移除。
    #[allow(dead_code)]
    pub async fn shutdown(&self) {
        self.connection_manager
            .cleanup_and_remove_shared_session::<SharedEthercatSession>(
                &self.connection_id,
                SharedEthercatSession::cleanup,
            )
            .await;
    }

    #[allow(dead_code)]
    pub fn connection_id(&self) -> &str {
        &self.connection_id
    }
}

/// 构造随部署撤销自动关闭 EtherCAT 共享会话的生命周期守卫。
pub fn lifecycle_guard(
    ctx: NodeLifecycleContext,
    connection_manager: SharedConnectionManager,
    connection_id: String,
) -> LifecycleGuard {
    let token = ctx.shutdown.clone();
    let join = tokio::spawn(async move {
        token.cancelled().await;
        connection_manager
            .cleanup_and_remove_shared_session::<SharedEthercatSession>(
                &connection_id,
                SharedEthercatSession::cleanup,
            )
            .await;
    });
    LifecycleGuard::from_task(ctx.shutdown, join)
}

/// 打开新的共享 EtherCAT 主站会话。
async fn open_shared_session(
    node_id: &str,
    connection_id: &str,
    connection_manager: &SharedConnectionManager,
) -> Result<SharedEthercatSession, EngineError> {
    // 获取连接元数据（不使用 ConnectionGuard 的排他借用）
    let record = connection_manager.get(connection_id).await.ok_or_else(|| {
        EngineError::node_config(
            node_id.to_owned(),
            format!("EtherCAT 连接 `{connection_id}` 不存在"),
        )
    })?;

    let config = if record.kind == "mock" || record.kind.is_empty() {
        mock_config()
    } else {
        EthercatConfig::from_metadata(&record.metadata)
            .map_err(|error| EngineError::node_config(node_id.to_owned(), error.to_string()))?
    };

    // 创建总线后端
    let bus = match create_ethercat_bus(&config).await {
        Ok(bus) => bus,
        Err(error) => {
            let reason = error.to_string();
            let _ = connection_manager
                .record_connect_failure(connection_id, &reason)
                .await;
            return Err(EngineError::node_config(
                node_id.to_owned(),
                format!("EtherCAT 主站初始化失败: {reason}"),
            ));
        }
    };

    let channel_info = bus.channel_info();
    let simulated = record.kind.is_empty();
    let lease = ConnectionLease {
        id: record.id.clone(),
        kind: record.kind.clone(),
        metadata: record.metadata.clone(),
        borrowed_at: chrono::Utc::now(),
    };

    let _ = connection_manager
        .record_connect_success(connection_id, "EtherCAT 主站共享会话已建立", None)
        .await;

    tracing::info!(connection_id, channel_info, "EtherCAT 主站共享会话已建立");

    Ok(SharedEthercatSession {
        bus: Mutex::new(Some(bus)),
        connection_id: connection_id.to_owned(),
        channel_info,
        simulated,
        lease: Some(lease),
    })
}

fn mock_config() -> EthercatConfig {
    EthercatConfig {
        backend: "mock".to_owned(),
        interface: "mock-eth0".to_owned(),
        cycle_time_ms: 5,
        op_timeout_ms: 15_000,
    }
}
