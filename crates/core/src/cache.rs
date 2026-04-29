//! ADR-0014 引脚二分：节点输出缓存槽。
//!
//! 每个声明 [`PinKind::Data`](crate::PinKind::Data) 输出引脚的节点持有一份
//! [`OutputCache`]，每个 Data 输出引脚对应一个 [`tokio::sync::watch`] 槽位。
//! Runner 在节点 transform 完成后写槽位；下游通过 Data 边消费时（Phase 2 起）
//! 读槽位。
//!
//! Phase 5 重构：`ArcSwap` + `Notify` 替换为 `watch` channel，单一原语同时
//! 提供值存储和变更通知。`read` 加 `ttl_ms` 过期检查不变。

use std::sync::Arc;

use chrono::{DateTime, Utc};
use dashmap::DashMap;
use serde_json::Value;
use tokio::sync::watch;
use uuid::Uuid;

/// 单个 Data 输出引脚的缓存值快照。
///
/// `trace_id` 携带产生此值时的上游 trace。下游消费时记录到自己的事件中——
/// 让"一次 transform 关联多个 trace"在观测层显式可见（设计见 ADR-0014 风险 3）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CachedOutput {
    pub value: Value,
    pub produced_at: DateTime<Utc>,
    pub trace_id: Uuid,
}

#[derive(Debug)]
struct Slot {
    tx: watch::Sender<Option<CachedOutput>>,
    rx: watch::Receiver<Option<CachedOutput>>,
}

/// 单节点持有的输出缓存——一个 Data 输出引脚对应一个槽位。
///
/// Phase 5 重构：每个槽位是一个 `watch` channel，写入时自动通知所有
/// [`subscribe`](Self::subscribe) 持有的 Receiver。
#[derive(Debug, Default)]
pub struct OutputCache {
    slots: DashMap<String, Arc<Slot>>,
}

impl OutputCache {
    pub fn new() -> Self {
        Self::default()
    }

    /// 部署期为指定 pin id 预分配槽位；同一 pin 多次预分配是幂等的。
    pub fn prepare_slot(&self, pin_id: &str) {
        if !self.slots.contains_key(pin_id) {
            let (tx, rx) = watch::channel(None);
            self.slots
                .insert(pin_id.to_owned(), Arc::new(Slot { tx, rx }));
        }
    }

    /// 写入指定 pin 的最新值并自动通知所有 Receiver。
    /// 返回 `true` 表示值与旧值不同（新写入或值变化）；`false` 表示值未变（覆盖写）。
    pub fn write(&self, pin_id: &str, output: CachedOutput) -> bool {
        if let Some(slot) = self.slots.get(pin_id) {
            let changed = slot
                .rx
                .borrow()
                .as_ref()
                .is_none_or(|old| old.value != output.value);
            let _ = slot.tx.send(Some(output));
            changed
        } else {
            false
        }
    }

    /// 便利方法：用当前时间戳构造 [`CachedOutput`] 并写入指定 pin。
    /// 返回值语义同 [`write`](Self::write)。
    pub fn write_now(&self, pin_id: &str, value: Value, trace_id: Uuid) -> bool {
        self.write(
            pin_id,
            CachedOutput {
                value,
                produced_at: Utc::now(),
                trace_id,
            },
        )
    }

    /// 读取指定 pin 的最新缓存值。`ttl_ms` 给出且值已过期时返回 `None`。
    pub fn read(&self, pin_id: &str, ttl_ms: Option<u64>) -> Option<CachedOutput> {
        let slot = self.slots.get(pin_id)?;
        let cached = slot.rx.borrow().clone()?;
        if let Some(ttl) = ttl_ms {
            let age = Utc::now()
                .signed_duration_since(cached.produced_at)
                .num_milliseconds();
            if age.unsigned_abs() > ttl {
                return None;
            }
        }
        Some(cached)
    }

    /// 拿到 slot 的 watch Receiver clone——pull collector 在 `BlockUntilReady` 下
    /// `changed().await` 等新值。
    pub fn subscribe(&self, pin_id: &str) -> Option<watch::Receiver<Option<CachedOutput>>> {
        self.slots.get(pin_id).map(|slot| slot.rx.clone())
    }

    /// 已分配槽位的 pin id 列表，主要供测试 / 调试。
    pub fn slot_ids(&self) -> Vec<String> {
        self.slots.iter().map(|entry| entry.key().clone()).collect()
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use std::time::Duration;
    use tokio::time::timeout;

    fn sample_output(value: i64) -> CachedOutput {
        CachedOutput {
            value: Value::from(value),
            produced_at: Utc::now(),
            trace_id: Uuid::nil(),
        }
    }

    #[test]
    fn 未预分配的_pin_读返回_none() {
        let cache = OutputCache::new();
        assert!(cache.read("ghost", None).is_none());
    }

    #[test]
    fn 预分配后未写入读返回_none() {
        let cache = OutputCache::new();
        cache.prepare_slot("latest");
        assert!(cache.read("latest", None).is_none());
    }

    #[test]
    fn 写后读返回最新值() {
        let cache = OutputCache::new();
        cache.prepare_slot("latest");
        cache.write("latest", sample_output(42));
        let got = cache.read("latest", None).unwrap();
        assert_eq!(got.value, Value::from(42));
    }

    #[test]
    fn 多次写入只保留最新() {
        let cache = OutputCache::new();
        cache.prepare_slot("latest");
        cache.write("latest", sample_output(1));
        cache.write("latest", sample_output(2));
        cache.write("latest", sample_output(3));
        let got = cache.read("latest", None).unwrap();
        assert_eq!(got.value, Value::from(3));
    }

    #[test]
    fn 写未预分配的_pin_是_noop() {
        let cache = OutputCache::new();
        cache.write("missing", sample_output(99));
        assert!(cache.read("missing", None).is_none());
        assert!(cache.slot_ids().is_empty());
    }

    #[test]
    fn prepare_slot_是幂等的() {
        let cache = OutputCache::new();
        cache.prepare_slot("a");
        cache.prepare_slot("a");
        cache.prepare_slot("a");
        assert_eq!(cache.slot_ids().len(), 1);
    }

    #[test]
    fn 多个_pin_独立存储() {
        let cache = OutputCache::new();
        cache.prepare_slot("alpha");
        cache.prepare_slot("beta");
        cache.write("alpha", sample_output(1));
        cache.write("beta", sample_output(2));
        assert_eq!(cache.read("alpha", None).unwrap().value, Value::from(1));
        assert_eq!(cache.read("beta", None).unwrap().value, Value::from(2));
    }

    #[tokio::test]
    async fn ttl_过期值视为空() {
        let cache = OutputCache::new();
        cache.prepare_slot("latest");
        cache.write(
            "latest",
            CachedOutput {
                value: Value::from(1),
                produced_at: Utc::now() - chrono::Duration::milliseconds(200),
                trace_id: Uuid::nil(),
            },
        );
        // ttl=100ms 已过期
        assert!(cache.read("latest", Some(100)).is_none());
        // ttl=300ms 未过期
        assert!(cache.read("latest", Some(300)).is_some());
        // 无 ttl 永远有效
        assert!(cache.read("latest", None).is_some());
    }

    #[tokio::test]
    async fn write_唤醒等待者() {
        let cache = Arc::new(OutputCache::new());
        cache.prepare_slot("latest");
        let mut rx = cache.subscribe("latest").unwrap();

        let cache2 = Arc::clone(&cache);
        let waiter = tokio::spawn(async move {
            rx.changed().await.unwrap();
            cache2.read("latest", None)
        });

        tokio::time::sleep(Duration::from_millis(50)).await;
        cache.write_now("latest", Value::from(42), Uuid::nil());

        let got = timeout(Duration::from_secs(1), waiter)
            .await
            .unwrap()
            .unwrap();
        assert!(got.is_some());
        assert_eq!(got.unwrap().value, Value::from(42));
    }
}
