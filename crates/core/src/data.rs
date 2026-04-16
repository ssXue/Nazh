//! 数据面：DataStore 控制面/数据面分离。
//!
//! 工作流 DAG 中的数据不再随通道传递完整 payload，而是存储在共享的
//! [`DataStore`] 中，通道只传递 ~64 字节的 [`ContextRef`](crate::ContextRef)。
//!
//! ## 三种访问模式
//!
//! - **只读**（if/switch/debugConsole）：`store.read(id)` 返回 `Arc<Value>`，零拷贝。
//! - **变换**（Rhai 脚本、数据注入）：`store.read_mut(id)` 真正 clone → 修改 → `store.write()`。
//! - **扇出**（1 输出 → N 下游）：`store.write(payload, N)` 设置 N 个消费者，
//!   N 个下游各自 `read()` 获取 Arc 共享引用，`release()` 触发引用计数归零后释放。

use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};

use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

use crate::error::EngineError;

/// 数据面中一份数据的唯一标识。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct DataId(Uuid);

impl DataId {
    /// 生成新的唯一数据标识。
    #[must_use]
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

impl Default for DataId {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Display for DataId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// 数据存储契约：控制面/数据面分离的核心抽象。
///
/// 实现必须满足 `Send + Sync`，因为 `DataStore` 在多个 Tokio 任务间共享。
pub trait DataStore: Send + Sync {
    /// 写入一份数据，声明有多少消费者需要读取。
    ///
    /// # Errors
    ///
    /// 内存容量超限时返回错误。
    fn write(&self, payload: Value, consumers: usize) -> Result<DataId, EngineError>;

    /// 零拷贝读取（返回共享引用）。
    ///
    /// # Errors
    ///
    /// 数据标识不存在时返回 [`EngineError::DataNotFound`]。
    fn read(&self, id: &DataId) -> Result<Arc<Value>, EngineError>;

    /// 读取可修改副本（Copy-on-Write：真正 clone payload）。
    ///
    /// # Errors
    ///
    /// 数据标识不存在时返回 [`EngineError::DataNotFound`]。
    fn read_mut(&self, id: &DataId) -> Result<Value, EngineError>;

    /// 消费完成，引用计数 -1。归零时自动释放。
    fn release(&self, id: &DataId);
}

/// `DataStore` 中的单条数据条目。
struct DataEntry {
    /// 数据本体，通过 Arc 实现零拷贝共享。
    payload: Arc<Value>,
    /// 剩余消费者计数，归零时从 map 中移除。
    remaining: AtomicUsize,
}

/// 基于 `DashMap` 的默认内存 `DataStore` 实现。
///
/// 数据以 `Arc<Value>` 存储在并发安全的 `DashMap` 中，
/// 通过原子引用计数管理生命周期。
///
/// ## 设计取舍
///
/// | 决策 | 选择 | 理由 |
/// |------|------|------|
/// | 后端 | 内存（DashMap + Arc） | 比 SQLite 快 ~2000 倍；在途数据丢失可接受 |
/// | 内存管理 | 引用计数 + 容量上限 | 引用计数处理正常路径；上限防 OOM |
/// | 扇出 | 同一 DataId 多消费者 | N 下游共享同一份 Arc<Value>，零拷贝 |
pub struct ArenaDataStore {
    entries: dashmap::DashMap<DataId, DataEntry>,
    /// 当前条目数上限（0 = 无限制）。
    capacity: usize,
}

impl ArenaDataStore {
    /// 创建无容量上限的 `DataStore`。
    #[must_use]
    pub fn new() -> Self {
        Self {
            entries: dashmap::DashMap::new(),
            capacity: 0,
        }
    }

    /// 创建有条目数上限的 `DataStore`。
    #[must_use]
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            entries: dashmap::DashMap::new(),
            capacity,
        }
    }

    /// 当前存储的条目数量。
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// 是否为空。
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

impl Default for ArenaDataStore {
    fn default() -> Self {
        Self::new()
    }
}

impl DataStore for ArenaDataStore {
    fn write(&self, payload: Value, consumers: usize) -> Result<DataId, EngineError> {
        if self.capacity > 0 && self.entries.len() >= self.capacity {
            return Err(EngineError::DataStoreCapacityExceeded {
                capacity: self.capacity,
            });
        }

        let id = DataId::new();
        let consumers = consumers.max(1);
        self.entries.insert(
            id,
            DataEntry {
                payload: Arc::new(payload),
                remaining: AtomicUsize::new(consumers),
            },
        );
        Ok(id)
    }

    fn read(&self, id: &DataId) -> Result<Arc<Value>, EngineError> {
        self.entries
            .get(id)
            .map(|entry| Arc::clone(&entry.payload))
            .ok_or(EngineError::DataNotFound(*id))
    }

    fn read_mut(&self, id: &DataId) -> Result<Value, EngineError> {
        self.entries
            .get(id)
            .map(|entry| (*entry.payload).clone())
            .ok_or(EngineError::DataNotFound(*id))
    }

    fn release(&self, id: &DataId) {
        // 先尝试递减；如果归零则移除条目。
        // 使用 DashMap::remove_if 确保原子性。
        self.entries.remove_if(id, |_key, entry| {
            entry.remaining.fetch_sub(1, Ordering::AcqRel) == 1
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn 写入后可零拷贝读取() {
        let store = ArenaDataStore::new();
        let data_id = store.write(json!({"value": 42}), 1).unwrap_or_else(|e| panic!("写入失败: {e}"));
        let payload = store.read(&data_id).unwrap_or_else(|e| panic!("读取失败: {e}"));
        assert_eq!(*payload, json!({"value": 42}));
    }

    #[test]
    fn 读取可修改副本不影响原始数据() {
        let store = ArenaDataStore::new();
        let data_id = store.write(json!({"value": 1}), 2).unwrap_or_else(|e| panic!("写入失败: {e}"));

        let mut copy = store.read_mut(&data_id).unwrap_or_else(|e| panic!("读取失败: {e}"));
        if let Some(obj) = copy.as_object_mut() {
            obj.insert("value".to_owned(), json!(999));
        }

        let original = store.read(&data_id).unwrap_or_else(|e| panic!("原始读取失败: {e}"));
        assert_eq!(*original, json!({"value": 1}), "原始数据不应被修改");
    }

    #[test]
    fn 引用计数归零后数据被释放() {
        let store = ArenaDataStore::new();
        let data_id = store.write(json!("hello"), 3).unwrap_or_else(|e| panic!("写入失败: {e}"));

        store.release(&data_id);
        assert!(store.read(&data_id).is_ok(), "释放 1/3 后仍可读");

        store.release(&data_id);
        assert!(store.read(&data_id).is_ok(), "释放 2/3 后仍可读");

        store.release(&data_id);
        assert!(store.read(&data_id).is_err(), "释放 3/3 后应已移除");
    }

    #[test]
    fn 扇出共享同一份数据() {
        let store = ArenaDataStore::new();
        let data_id = store.write(json!({"fan": "out"}), 3).unwrap_or_else(|e| panic!("写入失败: {e}"));

        let a = store.read(&data_id).unwrap_or_else(|e| panic!("读取 A 失败: {e}"));
        let b = store.read(&data_id).unwrap_or_else(|e| panic!("读取 B 失败: {e}"));
        // Arc::ptr_eq 验证两次读取返回的是同一份内存
        assert!(Arc::ptr_eq(&a, &b), "扇出读取应共享同一份 Arc");
    }

    #[test]
    fn 容量上限拒绝写入() {
        let store = ArenaDataStore::with_capacity(2);
        assert!(store.write(json!(1), 1).is_ok());
        assert!(store.write(json!(2), 1).is_ok());
        assert!(store.write(json!(3), 1).is_err(), "超出容量应返回错误");
    }

    #[test]
    fn 不存在的数据返回错误() {
        let store = ArenaDataStore::new();
        let fake_id = DataId::new();
        assert!(store.read(&fake_id).is_err());
        assert!(store.read_mut(&fake_id).is_err());
    }
}
