//! 异步执行守卫：统一的 panic 隔离与超时保护。
//!
//! DAG 节点运行循环和线性流水线阶段均通过 [`guarded_execute`] 执行，
//! 保证单个任务的 panic 或超时不会导致整个运行时崩溃。

use std::{future::Future, panic::AssertUnwindSafe, time::Duration};

use futures_util::FutureExt;
use uuid::Uuid;

use crate::EngineError;

/// 在 panic 隔离和可选超时保护下执行异步任务。
///
/// - 通过 [`AssertUnwindSafe`] + [`catch_unwind`](FutureExt::catch_unwind) 捕获 panic
/// - 可选的 [`tokio::time::timeout`] 保护
/// - panic 转换为 [`EngineError::StagePanicked`]
/// - 超时转换为 [`EngineError::StageTimeout`]
pub(crate) async fn guarded_execute<T, Fut>(
    stage: &str,
    trace_id: Uuid,
    timeout: Option<Duration>,
    fut: Fut,
) -> Result<T, EngineError>
where
    Fut: Future<Output = Result<T, EngineError>> + Send,
{
    let guarded = AssertUnwindSafe(fut).catch_unwind();

    if let Some(duration) = timeout {
        match tokio::time::timeout(duration, guarded).await {
            Ok(Ok(result)) => result,
            Ok(Err(_)) => Err(EngineError::StagePanicked {
                stage: stage.to_owned(),
                trace_id,
            }),
            Err(_) => Err(EngineError::StageTimeout {
                stage: stage.to_owned(),
                trace_id,
                timeout_ms: duration.as_millis(),
            }),
        }
    } else {
        guarded.await.unwrap_or_else(|_| {
            Err(EngineError::StagePanicked {
                stage: stage.to_owned(),
                trace_id,
            })
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn 正常执行返回结果() {
        let trace_id = Uuid::new_v4();
        let result: Result<i32, EngineError> =
            guarded_execute("test", trace_id, None, async { Ok(42) }).await;
        assert!(matches!(result, Ok(42)));
    }

    #[tokio::test]
    async fn 内部错误正常传播() {
        let trace_id = Uuid::new_v4();
        let result: Result<i32, EngineError> = guarded_execute("test", trace_id, None, async {
            Err(EngineError::invalid_graph("测试错误"))
        })
        .await;
        assert!(matches!(
            result,
            Err(EngineError::InvalidGraph(ref msg)) if msg.contains("测试错误")
        ));
    }

    #[tokio::test]
    async fn panic_被捕获转为阶段异常() {
        let trace_id = Uuid::new_v4();
        let result: Result<i32, EngineError> =
            guarded_execute("panicky", trace_id, None, async { panic!("boom") }).await;
        assert!(matches!(
            result,
            Err(EngineError::StagePanicked { ref stage, .. }) if stage == "panicky"
        ));
    }

    #[tokio::test]
    async fn 超时返回阶段超时错误() {
        let trace_id = Uuid::new_v4();
        let timeout = Some(Duration::from_millis(10));
        let result: Result<i32, EngineError> = guarded_execute("slow", trace_id, timeout, async {
            tokio::time::sleep(Duration::from_secs(10)).await;
            Ok(0)
        })
        .await;
        assert!(matches!(
            result,
            Err(EngineError::StageTimeout { ref stage, .. }) if stage == "slow"
        ));
    }
}
