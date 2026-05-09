use std::{any::Any, collections::HashSet, sync::Arc};

use tokio::sync::Mutex as AsyncMutex;

use nazh_core::EngineError;

use super::ConnectionManager;

impl ConnectionManager {
    pub(super) async fn has_shared_session(&self, connection_id: &str) -> bool {
        let sessions = self.shared_sessions.read().await;
        sessions.contains_key(connection_id)
    }

    pub(super) async fn has_any_shared_session(&self) -> bool {
        let sessions = self.shared_sessions.read().await;
        !sessions.is_empty()
    }

    pub(super) async fn shared_session_ids(&self) -> HashSet<String> {
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
