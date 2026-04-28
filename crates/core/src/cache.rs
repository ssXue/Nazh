//! ADR-0014 引脚二分：节点输出缓存槽。
//!
//! 每个声明 [`PinKind::Data`](crate::PinKind::Data) 输出引脚的节点持有一份
//! [`OutputCache`]，每个 Data 输出引脚对应一个 [`ArcSwap`] 槽位。Runner 在
//! 节点 transform 完成后写槽位；下游通过 Data 边消费时（Phase 2 起）读槽位。
//!
//! **Phase 1 范围**：仅完成"写"——下游消费在 Phase 2/3 接入。

use std::sync::Arc;

use arc_swap::ArcSwap;
use chrono::{DateTime, Utc};
use dashmap::DashMap;
use serde_json::Value;
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

/// 单节点持有的输出缓存——一个 Data 输出引脚对应一个槽位。
///
/// 槽位用 [`ArcSwap`] 包裹 `Option<CachedOutput>`：
/// - 写：[`store`](ArcSwap::store)，无锁
/// - 读：[`load_full`](ArcSwap::load_full)，返回快照副本
///
/// `slots` 由 [`prepare_slot`](Self::prepare_slot) 在部署期初始化（仅声明
/// Data 输出 pin 的节点会有非空 `slots`），运行期对未预分配的 pin 写入是
/// 静默 noop（属于实现 bug，Runner 不应触发）。
#[derive(Debug, Default)]
pub struct OutputCache {
    slots: DashMap<String, Arc<ArcSwap<Option<CachedOutput>>>>,
}

impl OutputCache {
    pub fn new() -> Self {
        Self::default()
    }

    /// 部署期为指定 pin id 预分配槽位；同一 pin 多次预分配是幂等的。
    pub fn prepare_slot(&self, pin_id: &str) {
        if !self.slots.contains_key(pin_id) {
            self.slots
                .insert(pin_id.to_owned(), Arc::new(ArcSwap::from_pointee(None)));
        }
    }

    /// 写入指定 pin 的最新值。
    /// pin 未预分配时静默忽略——上层 Runner 在调用前应确保 [`prepare_slot`](Self::prepare_slot) 已 cover。
    pub fn write(&self, pin_id: &str, output: CachedOutput) {
        if let Some(slot) = self.slots.get(pin_id) {
            slot.store(Arc::new(Some(output)));
        }
    }

    /// 读取指定 pin 的最新缓存值。pin 未预分配或槽空时返回 `None`。
    pub fn read(&self, pin_id: &str) -> Option<CachedOutput> {
        let slot = self.slots.get(pin_id)?;
        let snapshot = slot.load_full();
        (*snapshot).clone()
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
        assert!(cache.read("ghost").is_none());
    }

    #[test]
    fn 预分配后未写入读返回_none() {
        let cache = OutputCache::new();
        cache.prepare_slot("latest");
        assert!(cache.read("latest").is_none());
    }

    #[test]
    fn 写后读返回最新值() {
        let cache = OutputCache::new();
        cache.prepare_slot("latest");
        cache.write("latest", sample_output(42));
        let got = cache.read("latest").unwrap();
        assert_eq!(got.value, Value::from(42));
    }

    #[test]
    fn 多次写入只保留最新() {
        let cache = OutputCache::new();
        cache.prepare_slot("latest");
        cache.write("latest", sample_output(1));
        cache.write("latest", sample_output(2));
        cache.write("latest", sample_output(3));
        let got = cache.read("latest").unwrap();
        assert_eq!(got.value, Value::from(3));
    }

    #[test]
    fn 写未预分配的_pin_是_noop() {
        let cache = OutputCache::new();
        cache.write("missing", sample_output(99));
        assert!(cache.read("missing").is_none());
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
        assert_eq!(cache.read("alpha").unwrap().value, Value::from(1));
        assert_eq!(cache.read("beta").unwrap().value, Value::from(2));
    }
}
