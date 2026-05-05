//! 本地 `SQLite` 存储层（RFC-0003 Phase 1，ADR-0022）。
//!
//! Ring 1 crate，封装 [`rusqlite`]，提供类型化 API。消费方为 Tauri 壳层和
//! 未来的 edge-daemon。Ring 0 保持纯净——本 crate 只消费 [`serde_json::Value`]
//! 和基础类型，不依赖 `nazh-core`。

mod error;
mod global_variables;
mod history;
pub(crate) mod migrations;
mod variables;

pub use error::StoreError;

use rusqlite::Connection;
use std::path::Path;

/// 本地存储引擎。
///
/// `Connection` 不是 `Send + Sync`，因此用 `std::sync::Mutex` 包裹。
/// 壳层持有 `Arc<Store>`，多任务间通过 Mutex 序列化访问。
pub struct Store {
    db: std::sync::Mutex<Connection>,
}

impl Store {
    /// 打开（或创建）SQLite 数据库并执行待应用的 migrations。
    ///
    /// # Errors
    ///
    /// - `StoreError::Rusqlite` — 数据库文件无法创建或打开。
    /// - `StoreError::Rusqlite` — migration 执行失败。
    pub fn open(path: &Path) -> Result<Self, StoreError> {
        let db = Connection::open(path)?;
        db.execute_batch("PRAGMA journal_mode=WAL; PRAGMA foreign_keys=ON;")?;
        migrations::run(&db)?;
        Ok(Self {
            db: std::sync::Mutex::new(db),
        })
    }

    /// 仅用于测试：打开内存数据库。
    #[cfg(test)]
    pub fn open_in_memory() -> Result<Self, StoreError> {
        let db = Connection::open_in_memory()?;
        db.execute_batch("PRAGMA journal_mode=WAL; PRAGMA foreign_keys=ON;")?;
        migrations::run(&db)?;
        Ok(Self {
            db: std::sync::Mutex::new(db),
        })
    }

    /// 打开内存数据库（非持久化）。用于 Default 构造或无需持久化的场景。
    ///
    /// 启动后壳层 `setup` 会替换为文件持久化 Store。
    #[allow(clippy::expect_used, clippy::missing_panics_doc)]
    pub fn open_unpersisted() -> Self {
        let db = Connection::open_in_memory().expect("内存 `SQLite` 应始终成功");
        db.execute_batch("PRAGMA journal_mode=WAL; PRAGMA foreign_keys=ON;")
            .expect("PRAGMA 设置应成功");
        migrations::run(&db).expect("migration 应成功");
        Self {
            db: std::sync::Mutex::new(db),
        }
    }

    /// 获取数据库连接的 Mutex 守卫。
    #[allow(clippy::expect_used)]
    pub(crate) fn db(&self) -> std::sync::MutexGuard<'_, Connection> {
        self.db.lock().expect("Store Mutex 不应被 poisoned")
    }
}
