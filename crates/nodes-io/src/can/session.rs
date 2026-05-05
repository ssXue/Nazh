//! CAN 总线连接级共享会话。
//!
//! 同一 `connection_id` 的所有 CAN 节点共享一个总线实例，
//! 会话生命周期跟随工程部署/撤销，而非跟随单个节点。
//!
//! 真实 CAN/SLCAN 链路不能在每帧 transform 时反复打开串口、设置波特率、打开总线。
//! 本模块把建连成本移动到部署期或首帧，节点热路径只复用已建立的后端实例。

use std::sync::Arc;

use connections::{ConnectionLease, SharedConnectionManager};
use nazh_core::{EngineError, LifecycleGuard, NodeLifecycleContext};
use tokio::sync::{Mutex, MutexGuard};

use crate::can::{CanBus, CanBusConfig, CanError, create_can_bus};

/// 连接级共享 CAN 总线会话。
///
/// 内部用 `Mutex<Option<...>>` 包裹总线实例，支持多个节点并发访问，
/// 并在 `cleanup` 时安全取出总线执行关闭。
#[allow(dead_code)]
pub struct SharedCanBusSession {
    bus: Mutex<Option<Box<dyn CanBus>>>,
    connection_id: String,
    channel_info: String,
    simulated: bool,
    lease: Option<ConnectionLease>,
}

impl SharedCanBusSession {
    /// 获取总线引用。若会话已被清理则返回错误。
    pub fn bus(
        &self,
        node_id: &str,
    ) -> Result<MutexGuard<'_, Option<Box<dyn CanBus>>>, EngineError> {
        self.bus.try_lock().map_err(|_| {
            EngineError::node_config(node_id.to_owned(), "CAN 总线会话锁竞争".to_owned())
        })
    }

    pub fn simulated(&self) -> bool {
        self.simulated
    }

    pub fn channel_info(&self) -> &str {
        &self.channel_info
    }

    #[allow(dead_code)]
    pub fn connection_id(&self) -> &str {
        &self.connection_id
    }

    pub fn lease(&self) -> Option<&ConnectionLease> {
        self.lease.as_ref()
    }

    /// 关闭总线并释放硬件资源。
    ///
    /// 取出 `Option` 中的总线，触发其 `Drop`（SLCAN 会发送 `C\r` 并 join 读取线程）。
    pub fn cleanup(&self) {
        if let Ok(mut guard) = self.bus.try_lock()
            && let Some(bus) = guard.take()
            && let Err(error) = bus.shutdown()
        {
            tracing::warn!(?error, "CAN 总线会话清理失败");
        }
    }
}

/// CAN 总线运行期操作句柄。
///
/// 封装连接管理器和连接 ID，提供 `ensure_session` 接口
/// 获取或创建共享会话。轻量级，可按需创建。
pub struct CanBusRuntime {
    connection_manager: SharedConnectionManager,
    connection_id: String,
}

impl CanBusRuntime {
    pub fn new(connection_manager: SharedConnectionManager, connection_id: String) -> Self {
        Self {
            connection_manager,
            connection_id,
        }
    }

    /// 获取或创建当前连接的共享 CAN 会话。
    ///
    /// 多个节点共享同一 `connection_id` 时返回同一会话实例。
    /// 首次调用时执行建连，后续调用直接返回缓存。
    ///
    /// `configure` 在首次建连时有机会补充节点级配置（如连接级过滤器）。
    pub async fn ensure_session(
        &self,
        node_id: &str,
        configure: impl Fn(&mut CanBusConfig) -> Result<(), CanError>,
    ) -> Result<Arc<SharedCanBusSession>, EngineError> {
        let conn_id = self.connection_id.clone();
        let cm = self.connection_manager.clone();
        let node_id = node_id.to_owned();

        self.connection_manager
            .ensure_shared_session::<SharedCanBusSession>(&self.connection_id, async || {
                open_shared_session(&node_id, &conn_id, &cm, configure).await
            })
            .await
    }

    /// 关闭共享会话并从连接管理器移除。
    pub async fn shutdown(&self) {
        self.connection_manager
            .cleanup_and_remove_shared_session::<SharedCanBusSession>(
                &self.connection_id,
                SharedCanBusSession::cleanup,
            )
            .await;
    }

    #[allow(dead_code)]
    pub fn connection_id(&self) -> &str {
        &self.connection_id
    }
}

/// 构造随部署撤销自动关闭 CAN 共享会话的生命周期守卫。
pub fn lifecycle_guard(
    ctx: NodeLifecycleContext,
    connection_manager: SharedConnectionManager,
    connection_id: String,
) -> LifecycleGuard {
    let token = ctx.shutdown.clone();
    let join = tokio::spawn(async move {
        token.cancelled().await;
        connection_manager
            .cleanup_and_remove_shared_session::<SharedCanBusSession>(
                &connection_id,
                SharedCanBusSession::cleanup,
            )
            .await;
    });
    LifecycleGuard::from_task(ctx.shutdown, join)
}

/// 打开新的共享 CAN 总线会话。
async fn open_shared_session(
    node_id: &str,
    connection_id: &str,
    connection_manager: &SharedConnectionManager,
    configure: impl Fn(&mut CanBusConfig) -> Result<(), CanError>,
) -> Result<SharedCanBusSession, EngineError> {
    // 获取连接元数据（不使用 ConnectionGuard 的排他借用）
    let record = connection_manager.get(connection_id).await.ok_or_else(|| {
        EngineError::node_config(
            node_id.to_owned(),
            format!("CAN 连接 `{connection_id}` 不存在"),
        )
    })?;

    let mut config = if record.kind == "mock" || record.kind.is_empty() {
        mock_config()
    } else {
        CanBusConfig::from_metadata(&record.metadata)
            .map_err(|error| EngineError::node_config(node_id.to_owned(), error.to_string()))?
    };

    // 应用节点级配置
    configure(&mut config)
        .map_err(|error| EngineError::node_config(node_id.to_owned(), error.to_string()))?;

    // 创建总线后端
    let bus = match create_can_bus(&config).await {
        Ok(bus) => bus,
        Err(error) => {
            let reason = error.to_string();
            let _ = connection_manager
                .record_connect_failure(connection_id, &reason)
                .await;
            return Err(EngineError::node_config(
                node_id.to_owned(),
                format!("CAN 总线初始化失败: {reason}"),
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
        .record_connect_success(connection_id, "CAN 总线共享会话已建立", None)
        .await;

    tracing::info!(connection_id, channel_info, "CAN 总线共享会话已建立");

    Ok(SharedCanBusSession {
        bus: Mutex::new(Some(bus)),
        connection_id: connection_id.to_owned(),
        channel_info,
        simulated,
        lease: Some(lease),
    })
}

fn mock_config() -> CanBusConfig {
    CanBusConfig {
        interface: "mock".to_owned(),
        channel: "mock-can".to_owned(),
        baud_rate: 115_200,
        bitrate: 500_000,
        filters: Vec::new(),
        fd: false,
        receive_own_messages: false,
    }
}
