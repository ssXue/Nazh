//! PURE 纯函数节点的 trace 内 memo 缓存（ADR-0014 Phase 4）。

use std::sync::Arc;

use dashmap::DashMap;
use nazh_core::Uuid;
use serde_json::Value;

/// PURE 纯函数节点在同一 trace 内的输入哈希记忆缓存（ADR-0014 Phase 4）。
///
/// Key = `(node_id, trace_id, input_hash)`；Value = transform 产出的 payload。
/// fan-out 场景下同一 pure 节点被多个下游重复拉取时，第二次起直接命中缓存。
///
/// Trace 完成后由 Runner 调用 [`clear_trace`](Self::clear_trace) 清理对应条目，
/// 防止内存随 trace 数无限增长。
#[derive(Debug, Default)]
pub(crate) struct PureMemo {
    inner: Arc<DashMap<(String, Uuid, u64), Value>>,
}

impl PureMemo {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn get(&self, node_id: &str, trace_id: Uuid, input_hash: u64) -> Option<Value> {
        self.inner
            .get(&(node_id.to_owned(), trace_id, input_hash))
            .map(|v| v.value().clone())
    }

    pub fn insert(&self, node_id: &str, trace_id: Uuid, input_hash: u64, payload: Value) {
        self.inner
            .insert((node_id.to_owned(), trace_id, input_hash), payload);
    }

    /// 清理指定 trace 的所有 memo 条目。
    /// 由 Runner 在 Exec 节点完成一个 trace 后调用。
    /// 幂等——不存在的 key 被 `DashMap` 静默跳过。
    pub fn clear_trace(&self, trace_id: Uuid) {
        self.inner.retain(|key, _| key.1 != trace_id);
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn pure_memo_insert_and_get() {
        let memo = PureMemo::new();
        let trace = Uuid::nil();

        assert!(memo.get("node", trace, 123).is_none());

        memo.insert("node", trace, 123, serde_json::json!({"out": 99}));
        let hit = memo.get("node", trace, 123).unwrap();
        assert_eq!(hit, serde_json::json!({"out": 99}));

        // 不同 trace 不命中
        let other_trace = Uuid::new_v4();
        assert!(memo.get("node", other_trace, 123).is_none());
    }

    #[test]
    fn clear_trace_只清目标_trace() {
        let memo = PureMemo::new();
        let t1 = Uuid::new_v4();
        let t2 = Uuid::new_v4();

        memo.insert("node", t1, 1, serde_json::json!(1));
        memo.insert("node", t2, 2, serde_json::json!(2));
        memo.insert("other", t1, 3, serde_json::json!(3));

        memo.clear_trace(t1);

        // t1 的条目被清
        assert!(memo.get("node", t1, 1).is_none());
        assert!(memo.get("other", t1, 3).is_none());
        // t2 的条目保留
        assert_eq!(memo.get("node", t2, 2).unwrap(), serde_json::json!(2));
    }
}
