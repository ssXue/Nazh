//! 节点生命周期钩子（ADR-0009）：长连接/触发器节点的部署 RAII 抽象。
//!
//! ## 关键类型
//!
//! - [`NodeLifecycleContext`]：传给 [`NodeTrait::on_deploy`](crate::NodeTrait::on_deploy)
//!   的受限上下文，含资源包、向 DAG 推数据的 [`NodeHandle`] 与
//!   `tokio_util::sync::CancellationToken` 取消信号。
//! - [`NodeHandle`]：触发器节点（MQTT 订阅 / Timer / Serial 监听）把外部消息
//!   推进 DAG 数据通道的句柄。`emit` 内部封装"写 `DataStore` + 广播 `ContextRef`
//!   + 发 Started/Completed 事件"，与 Runner 的 `apply_output` 路径保持单一实现。
//! - [`LifecycleGuard`]：RAII 句柄。Drop 时取消 token；
//!   [`shutdown`](LifecycleGuard::shutdown) 提供显式异步等待（默认 5s 超时）。
//!
//! ## 单一 emit 路径承诺
//!
//! `NodeHandle::emit` 与 Runner 的 `transform → apply_output` 路径**逻辑等价**：
//! 同一份 payload + metadata 在两条路径上产生的事件序列必须一致——这是 ADR-0009
//! 风险章节明确要求的不变量，避免触发器节点与变换节点在元数据语义上分裂。

use std::sync::Arc;

use serde_json::{Map, Value};
use tokio::sync::mpsc;
use tokio::task::JoinHandle;
use tokio::time::Duration;
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

use crate::{
    CompletedExecutionEvent, ContextRef, DataStore, EngineError, ExecutionEvent, SharedResources,
};

/// 节点部署钩子可用的受限上下文。
///
/// 由 Runner 在调用 [`NodeTrait::on_deploy`](crate::NodeTrait::on_deploy)
/// 前为每个节点构造一次。`shutdown` 是从工作流根 token 派生的子 token——撤销
/// 整图时根 token 取消会沿派生链广播到所有节点。
pub struct NodeLifecycleContext {
    /// 与节点工厂同款的资源包（含 `SharedConnectionManager`、`Arc<dyn AiService>` 等）。
    pub resources: SharedResources,
    /// 向 DAG 数据通道推消息的句柄；触发器节点用，纯变换节点忽略即可。
    pub handle: NodeHandle,
    /// 撤销信号。后台任务必须在 `tokio::select!` 第一分支监听 `cancelled().await`。
    pub shutdown: CancellationToken,
}

/// 触发器节点向 DAG 推消息的句柄。
///
/// 由 Runner 在节点 spawn 之前为每个节点构造。每次 [`emit`](NodeHandle::emit)：
/// 1. 发 [`ExecutionEvent::Started`]（新 `trace_id`）
/// 2. 写 `DataStore` 得到 `DataId`
/// 3. 构造 [`ContextRef`]（`source_node = Some(node_id)`）广播到所有下游 sender
/// 4. 发 [`ExecutionEvent::Completed`]（携带传入的 metadata）
///
/// 触发器节点不发 [`ExecutionEvent::Output`]——`Output` 是叶节点专用。
#[derive(Clone)]
pub struct NodeHandle {
    inner: Arc<NodeHandleInner>,
}

struct NodeHandleInner {
    node_id: String,
    store: Arc<dyn DataStore>,
    downstream: Vec<mpsc::Sender<ContextRef>>,
    event_tx: mpsc::Sender<ExecutionEvent>,
}

impl NodeHandle {
    /// Runner 构造接口。Ring 1 节点不应直接调用——它们通过
    /// [`NodeLifecycleContext::handle`] 获取实例。
    #[must_use]
    pub fn new(
        node_id: impl Into<String>,
        store: Arc<dyn DataStore>,
        downstream: Vec<mpsc::Sender<ContextRef>>,
        event_tx: mpsc::Sender<ExecutionEvent>,
    ) -> Self {
        Self {
            inner: Arc::new(NodeHandleInner {
                node_id: node_id.into(),
                store,
                downstream,
                event_tx,
            }),
        }
    }

    /// 该 handle 所属的节点 ID。
    #[must_use]
    pub fn node_id(&self) -> &str {
        &self.inner.node_id
    }

    /// 触发器节点向下游推一条数据。
    ///
    /// `metadata` 通过 [`ExecutionEvent::Completed`] 事件通道传递，**不会**
    /// 进入 payload（与 `transform` 路径保持一致——ADR-0008 不变量）。空
    /// metadata 时事件中的 `metadata` 字段为 `None`。
    ///
    /// # Errors
    ///
    /// `DataStore::write` 失败（容量上限）时返回 [`EngineError::DataStoreCapacityExceeded`]。
    /// 下游 channel 已关闭不视为错误——当作"无消费者，丢弃即可"，仅 `tracing::debug!` 记录。
    pub async fn emit(
        &self,
        payload: Value,
        metadata: Map<String, Value>,
    ) -> Result<(), EngineError> {
        let inner = &self.inner;
        let trace_id = Uuid::new_v4();

        // 1. 写 DataStore（先做，因为失败要立即返回；事件未发出避免半成品状态）
        let consumers = inner.downstream.len().max(1);
        let data_id = inner.store.write(payload, consumers)?;

        // 2. 发 Started 事件（事件通道满 / 关闭都不阻塞 emit；事件丢失只影响可观测性）
        let _ = inner
            .event_tx
            .send(ExecutionEvent::Started {
                stage: inner.node_id.clone(),
                trace_id,
            })
            .await;

        // 3. 广播 ContextRef 到下游
        let ctx_ref = ContextRef::new(trace_id, data_id, Some(inner.node_id.clone()));
        for sender in &inner.downstream {
            if sender.send(ctx_ref.clone()).await.is_err() {
                tracing::debug!(
                    node_id = %inner.node_id,
                    "下游 channel 已关闭，触发数据丢弃"
                );
            }
        }

        // 4. 发 Completed 事件（携带元数据；空 metadata 转 None 与 transform 路径一致）
        let metadata_field = if metadata.is_empty() {
            None
        } else {
            Some(metadata)
        };
        let _ = inner
            .event_tx
            .send(ExecutionEvent::Completed(CompletedExecutionEvent {
                stage: inner.node_id.clone(),
                trace_id,
                metadata: metadata_field,
            }))
            .await;

        Ok(())
    }
}

/// 节点生命周期 RAII 句柄。
///
/// `Drop` 立即取消内部 [`CancellationToken`]——**不**等待后台任务结束（Drop
/// 不能 await）。如需同步等待清理完成，请显式调用 [`shutdown`](Self::shutdown)。
///
/// 默认 shutdown 超时 5s，可通过 [`with_shutdown_timeout`](Self::with_shutdown_timeout)
/// 调整（如 MQTT 订阅者完成 broker 优雅断开可能需要更久）。
pub struct LifecycleGuard {
    inner: Option<LifecycleGuardInner>,
}

struct LifecycleGuardInner {
    token: CancellationToken,
    join: Option<JoinHandle<()>>,
    shutdown_timeout: Duration,
}

impl LifecycleGuard {
    /// 纯变换节点用：drop / shutdown 都不做任何事。
    ///
    /// `NodeTrait::on_deploy` 的默认实现返回此 guard。
    #[must_use]
    pub fn noop() -> Self {
        Self { inner: None }
    }

    /// 从已 spawn 的 Tokio 任务构造 guard。
    ///
    /// `token` 必须是 spawn 任务时持有的同一个 [`CancellationToken`]——
    /// guard drop / shutdown 时通过它通知任务退出。
    #[must_use]
    pub fn from_task(token: CancellationToken, join: JoinHandle<()>) -> Self {
        Self {
            inner: Some(LifecycleGuardInner {
                token,
                join: Some(join),
                shutdown_timeout: Duration::from_secs(5),
            }),
        }
    }

    /// 自定义 shutdown 超时（默认 5 秒）。
    #[must_use]
    pub fn with_shutdown_timeout(mut self, timeout: Duration) -> Self {
        if let Some(inner) = self.inner.as_mut() {
            inner.shutdown_timeout = timeout;
        }
        self
    }

    /// 显式取消 + 等待任务结束（受 [`shutdown_timeout`](Self::with_shutdown_timeout) 保护）。
    ///
    /// 超时则放弃等待并 `tracing::warn!`——任务可能仍在运行，由 Tokio 自然回收。
    /// 调用方拿不到任务的最终错误（如有）；这是有意设计：撤销路径关心"是否
    /// 清理完成"，不关心业务错误。
    pub async fn shutdown(mut self) {
        let Some(mut inner) = self.inner.take() else {
            return;
        };
        inner.token.cancel();
        let Some(join) = inner.join.take() else {
            return;
        };
        match tokio::time::timeout(inner.shutdown_timeout, join).await {
            Ok(Ok(())) => {}
            Ok(Err(error)) => {
                tracing::warn!(?error, "节点生命周期任务退出错误");
            }
            Err(_) => {
                tracing::warn!(
                    timeout_ms = inner.shutdown_timeout.as_millis(),
                    "节点生命周期任务 shutdown 超时，已取消但未等到退出"
                );
            }
        }
    }
}

impl Drop for LifecycleGuard {
    fn drop(&mut self) {
        if let Some(inner) = self.inner.take() {
            inner.token.cancel();
            // 不在 Drop 中 await JoinHandle（异步上下文不可用）。
            // Tokio 会在任务感知到 token cancel 后自然退出并被 runtime 回收。
            // 调用方需要确定性等待清理时请用 shutdown().await。
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use std::sync::atomic::{AtomicBool, Ordering};

    use super::*;
    use crate::ArenaDataStore;

    #[tokio::test]
    async fn noop_guard_drop_不_panic() {
        let guard = LifecycleGuard::noop();
        drop(guard);
    }

    #[tokio::test]
    async fn noop_guard_shutdown_立即返回() {
        let guard = LifecycleGuard::noop();
        guard.shutdown().await;
    }

    #[tokio::test]
    async fn guard_drop_触发_token_cancel() {
        let token = CancellationToken::new();
        let observed = Arc::new(AtomicBool::new(false));
        let observed_clone = Arc::clone(&observed);
        let task_token = token.clone();
        let join = tokio::spawn(async move {
            task_token.cancelled().await;
            observed_clone.store(true, Ordering::SeqCst);
        });

        let guard = LifecycleGuard::from_task(token, join);
        drop(guard);
        // 给任务一个 yield 机会观察到 cancel
        tokio::time::sleep(Duration::from_millis(50)).await;
        assert!(observed.load(Ordering::SeqCst), "drop 应触发 cancel 信号");
    }

    #[tokio::test]
    async fn shutdown_等待任务正常退出() {
        let token = CancellationToken::new();
        let task_token = token.clone();
        let join = tokio::spawn(async move {
            task_token.cancelled().await;
            // 模拟一点清理工作
            tokio::time::sleep(Duration::from_millis(10)).await;
        });

        let guard = LifecycleGuard::from_task(token, join);
        let shutdown_started = std::time::Instant::now();
        guard.shutdown().await;
        let elapsed = shutdown_started.elapsed();
        assert!(
            elapsed < Duration::from_secs(1),
            "shutdown 应等到任务退出，但花了 {elapsed:?}"
        );
    }

    #[tokio::test]
    async fn shutdown_超时则放弃等待() {
        let token = CancellationToken::new();
        // 任务故意忽略 cancel，模拟"卡住"的清理
        let join = tokio::spawn(async {
            tokio::time::sleep(Duration::from_secs(60)).await;
        });

        let guard =
            LifecycleGuard::from_task(token, join).with_shutdown_timeout(Duration::from_millis(50));
        let started = std::time::Instant::now();
        guard.shutdown().await;
        let elapsed = started.elapsed();
        assert!(
            elapsed < Duration::from_millis(500),
            "shutdown 超时应在 ~50ms 后返回，实际 {elapsed:?}"
        );
    }

    #[tokio::test]
    async fn node_handle_emit_发出_started_completed_事件() {
        let store: Arc<dyn DataStore> = Arc::new(ArenaDataStore::new());
        let (event_tx, mut event_rx) = mpsc::channel(8);
        let (downstream_tx, mut downstream_rx) = mpsc::channel(4);
        let handle = NodeHandle::new("trigger-1", store, vec![downstream_tx], event_tx);

        handle
            .emit(
                serde_json::json!({"value": 42}),
                Map::new(),
            )
            .await
            .unwrap();

        let started = event_rx.recv().await.unwrap();
        assert!(matches!(started, ExecutionEvent::Started { .. }));
        let completed = event_rx.recv().await.unwrap();
        match completed {
            ExecutionEvent::Completed(event) => {
                assert_eq!(event.stage, "trigger-1");
                assert!(event.metadata.is_none(), "空 metadata 应转为 None");
            }
            other => panic!("expected Completed, got {other:?}"),
        }

        let ctx_ref = downstream_rx.recv().await.unwrap();
        assert_eq!(ctx_ref.source_node.as_deref(), Some("trigger-1"));
    }

    #[tokio::test]
    async fn node_handle_emit_metadata_非空时进入_completed() {
        let store: Arc<dyn DataStore> = Arc::new(ArenaDataStore::new());
        let (event_tx, mut event_rx) = mpsc::channel(8);
        let handle = NodeHandle::new("trigger-2", store, vec![], event_tx);

        let mut metadata = Map::new();
        metadata.insert("timer".to_owned(), serde_json::json!({"interval_ms": 1000}));
        handle
            .emit(serde_json::Value::Null, metadata.clone())
            .await
            .unwrap();

        // 跳过 Started，校验 Completed 中的 metadata
        let _ = event_rx.recv().await;
        match event_rx.recv().await.unwrap() {
            ExecutionEvent::Completed(event) => {
                assert_eq!(event.metadata.as_ref(), Some(&metadata));
            }
            other => panic!("expected Completed, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn node_handle_emit_下游关闭不报错() {
        let store: Arc<dyn DataStore> = Arc::new(ArenaDataStore::new());
        let (event_tx, _event_rx) = mpsc::channel(8);
        let (downstream_tx, downstream_rx) = mpsc::channel::<ContextRef>(1);
        drop(downstream_rx); // 立即关闭下游
        let handle = NodeHandle::new("trigger-3", store, vec![downstream_tx], event_tx);

        // 不应返回 Err；只是 tracing::debug! 记录
        handle
            .emit(serde_json::Value::Null, Map::new())
            .await
            .unwrap();
    }
}
