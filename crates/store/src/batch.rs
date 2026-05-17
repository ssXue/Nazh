//! 批量写入器：异步入队 + 后台定时/定量 flush。

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use tokio::sync::mpsc;
use tokio::task::JoinHandle;
use tokio::time::{Duration, MissedTickBehavior};

use crate::{Store, StoreError};

/// 批量写入器。
///
/// 调用方通过 [`enqueue`] 将记录推入通道即返回（即发即弃）。后台 Tokio task
/// 按容量阈值或时间阈值批量 flush 到 Store。
pub struct BatchWriter<O: Send + 'static> {
    tx: Option<mpsc::Sender<O>>,
    flush_guard: Option<JoinHandle<()>>,
    /// 累计丢弃数量（通道满时丢弃）。
    dropped_count: Arc<AtomicU64>,
}

impl<O: Send + 'static> BatchWriter<O> {
    /// 创建批量写入器。
    ///
    /// - `channel_capacity`：mpsc 通道容量，超出时丢弃。
    /// - `flush_capacity`：累积多少条后触发 flush。
    /// - `flush_interval`：定时 flush 间隔（毫秒）。
    /// - `store`：共享 Store 引用。
    /// - `flush_fn`：执行批量写入的闭包。
    pub fn new<F>(
        channel_capacity: usize,
        flush_capacity: usize,
        flush_interval_ms: u64,
        store: Arc<Store>,
        flush_fn: F,
    ) -> Self
    where
        F: Fn(&Store, Vec<O>) -> Result<(), StoreError> + Send + 'static,
    {
        let (tx, rx) = mpsc::channel(channel_capacity);
        let dropped_count = Arc::new(AtomicU64::new(0));
        let dropped_inner = Arc::clone(&dropped_count);

        let guard = tokio::spawn(background_flush_task(
            rx,
            store,
            flush_fn,
            flush_capacity,
            flush_interval_ms,
            dropped_inner,
        ));

        Self {
            tx: Some(tx),
            flush_guard: Some(guard),
            dropped_count,
        }
    }

    /// 入队一条记录。通道满时丢弃并增加丢弃计数。
    pub fn enqueue(&self, item: O) {
        let Some(tx) = &self.tx else {
            return;
        };
        if tx.try_send(item).is_err() {
            let prev = self.dropped_count.fetch_add(1, Ordering::Relaxed);
            tracing::debug!(dropped_total = prev + 1, "批量写入器通道已满，丢弃记录");
        }
    }

    /// 优雅关闭：关闭通道并等待后台 task flush 剩余记录。
    pub async fn shutdown(mut self) {
        // Drop Sender 关闭通道，后台 task 会在 drain 后退出
        self.tx = None;
        if let Some(guard) = self.flush_guard.take() {
            let _ = guard.await;
        }
    }

    /// 返回累计丢弃数量。
    pub fn dropped_count(&self) -> u64 {
        self.dropped_count.load(Ordering::Relaxed)
    }
}

impl<O: Send + 'static> Drop for BatchWriter<O> {
    fn drop(&mut self) {
        // Drop tx（置 None）关闭通道，后台 task drain 完成后退出。
        self.tx = None;
        // abort 防止 task 泄漏；drop guard 会自动 abort。
        if let Some(guard) = self.flush_guard.take() {
            guard.abort();
        }
    }
}

async fn background_flush_task<O, F>(
    mut rx: mpsc::Receiver<O>,
    store: Arc<Store>,
    flush_fn: F,
    flush_capacity: usize,
    flush_interval_ms: u64,
    dropped_count: Arc<AtomicU64>,
) where
    O: Send + 'static,
    F: Fn(&Store, Vec<O>) -> Result<(), StoreError> + Send + 'static,
{
    let mut interval = tokio::time::interval(Duration::from_millis(flush_interval_ms));
    interval.set_missed_tick_behavior(MissedTickBehavior::Delay);
    let _ = interval.tick().await; // 跳过首次立即触发

    let mut buffer = Vec::new();

    loop {
        tokio::select! {
            biased; // 优先检查通道关闭

            item = rx.recv() => {
                if let Some(item) = item {
                    buffer.push(item);
                    if buffer.len() >= flush_capacity {
                        do_flush(&store, &flush_fn, &mut buffer, &dropped_count);
                        let _ = interval.tick().await; // 重置定时器
                    }
                } else {
                    // 通道关闭 → flush 剩余并退出
                    if !buffer.is_empty() {
                        do_flush(&store, &flush_fn, &mut buffer, &dropped_count);
                    }
                    return;
                }
            }
            // 定时 flush
            _ = interval.tick() => {
                if !buffer.is_empty() {
                    do_flush(&store, &flush_fn, &mut buffer, &dropped_count);
                }
            }
        }
    }
}

fn do_flush<O, F>(
    store: &Arc<Store>,
    flush_fn: &F,
    buffer: &mut Vec<O>,
    dropped_count: &Arc<AtomicU64>,
) where
    F: Fn(&Store, Vec<O>) -> Result<(), StoreError>,
{
    let batch: Vec<O> = std::mem::take(buffer);
    let batch_len = batch.len();
    match flush_fn(store, batch) {
        Ok(()) => {}
        Err(error) => {
            let dropped = dropped_count.fetch_add(batch_len as u64, Ordering::Relaxed);
            tracing::warn!(
                ?error,
                batch_size = batch_len,
                dropped_total = dropped + batch_len as u64,
                "批量 flush 失败，丢弃本批记录"
            );
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use std::sync::Mutex as StdMutex;

    fn test_store() -> Arc<Store> {
        Arc::new(Store::open_in_memory().expect("内存 Store 应可打开"))
    }

    #[tokio::test]
    async fn 容量阈值触发_flush() {
        let store = test_store();
        let flushed: Arc<StdMutex<Vec<Vec<i32>>>> = Arc::new(StdMutex::new(Vec::new()));
        let flushed_inner = Arc::clone(&flushed);

        let writer = BatchWriter::new(
            1024,
            3, // flush_capacity = 3
            5000,
            store,
            move |_store, batch| {
                flushed_inner.lock().expect("lock").push(batch);
                Ok(())
            },
        );

        writer.enqueue(1);
        writer.enqueue(2);
        writer.enqueue(3); // 应触发 flush
        tokio::time::sleep(Duration::from_millis(50)).await;

        let batches = flushed.lock().expect("lock");
        assert_eq!(batches.len(), 1);
        assert_eq!(batches[0], vec![1, 2, 3]);
    }

    #[tokio::test]
    async fn 定时触发_flush() {
        let store = test_store();
        let flushed: Arc<StdMutex<Vec<Vec<i32>>>> = Arc::new(StdMutex::new(Vec::new()));
        let flushed_inner = Arc::clone(&flushed);

        let writer = BatchWriter::new(
            1024,
            100, // 高容量阈值，靠定时器触发
            50,  // 50ms 间隔
            store,
            move |_store, batch| {
                flushed_inner.lock().expect("lock").push(batch);
                Ok(())
            },
        );

        writer.enqueue(42);
        tokio::time::sleep(Duration::from_millis(120)).await;

        let batches = flushed.lock().expect("lock");
        assert_eq!(batches.len(), 1);
        assert_eq!(batches[0], vec![42]);
    }

    #[tokio::test]
    async fn shutdown_刷新剩余() {
        let store = test_store();
        let flushed: Arc<StdMutex<Vec<Vec<i32>>>> = Arc::new(StdMutex::new(Vec::new()));
        let flushed_inner = Arc::clone(&flushed);

        let writer = BatchWriter::new(
            1024,
            100,  // 高容量阈值
            5000, // 长间隔，靠 shutdown 触发
            store,
            move |_store, batch| {
                flushed_inner.lock().expect("lock").push(batch);
                Ok(())
            },
        );

        writer.enqueue(1);
        writer.enqueue(2);
        writer.shutdown().await;

        let batches = flushed.lock().expect("lock");
        assert_eq!(batches.len(), 1);
        assert_eq!(batches[0], vec![1, 2]);
    }

    #[tokio::test]
    async fn 满通道时丢弃并计数() {
        let store = test_store();
        let flushed: Arc<StdMutex<Vec<Vec<i32>>>> = Arc::new(StdMutex::new(Vec::new()));
        let flushed_inner = Arc::clone(&flushed);

        let writer = BatchWriter::new(
            2,   // 通道容量 2
            100, // 高 flush 阈值
            5000,
            store,
            move |_store, batch| {
                flushed_inner.lock().expect("lock").push(batch);
                Ok(())
            },
        );

        writer.enqueue(1);
        writer.enqueue(2);
        // 通道已满，后续入队应被丢弃
        writer.enqueue(3);
        writer.enqueue(4);

        assert_eq!(writer.dropped_count(), 2);
    }
}
