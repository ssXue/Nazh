//! 全局连接资源池。
//!
//! 节点绝不直接访问硬件。所有协议连接（Modbus、MQTT、HTTP 等）
//! 均注册在 [`ConnectionManager`] 中，通过共享的 `Arc<ConnectionManager>`
//! 以借出/归还模式访问。内部对每个连接单独加锁，不同连接的并发借出互不阻塞。

use std::{collections::HashMap, sync::Arc};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio::sync::{Mutex, RwLock};
use ts_rs::TS;

use crate::EngineError;

/// 全局连接池的线程安全句柄。
pub type SharedConnectionManager = Arc<ConnectionManager>;

/// 连接资源的声明式定义（用于工作流 AST）。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, TS)]
#[ts(export)]
pub struct ConnectionDefinition {
    pub id: String,
    #[serde(rename = "type")]
    #[serde(alias = "kind")]
    pub kind: String,
    #[serde(default)]
    pub metadata: Value,
}

/// 由 [`ConnectionManager::borrow`] 返回的临时借出连接句柄。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ConnectionLease {
    pub id: String,
    pub kind: String,
    pub metadata: Value,
    pub borrowed_at: DateTime<Utc>,
}

/// 已注册连接的内部记录，追踪其借出状态。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, TS)]
#[ts(export)]
pub struct ConnectionRecord {
    pub id: String,
    pub kind: String,
    pub metadata: Value,
    pub in_use: bool,
    #[ts(optional)]
    pub last_borrowed_at: Option<DateTime<Utc>>,
}

/// 管理具名连接资源池，采用排他借出语义。
///
/// 当前为骨架实现，尚未对接真实的 Modbus/MQTT/HTTP 驱动。
/// 资源池保证同一连接同时只能被一个节点借出。
/// 内部使用细粒度锁：外层 `RwLock` 保护 `HashMap` 结构，
/// 每条连接记录由独立的 `Mutex` 保护，不同连接可并发借出。
#[derive(Debug, Default)]
pub struct ConnectionManager {
    connections: RwLock<HashMap<String, Arc<Mutex<ConnectionRecord>>>>,
}

/// 创建一个空的 [`ConnectionManager`]，包装在 `Arc<...>` 中。
pub fn shared_connection_manager() -> SharedConnectionManager {
    Arc::new(ConnectionManager::default())
}

impl ConnectionManager {
    /// 注册新连接。若 ID 已存在则返回错误。
    ///
    /// # Errors
    ///
    /// 连接 ID 已存在时返回 [`EngineError::ConnectionAlreadyExists`]。
    pub async fn register_connection(
        &self,
        definition: ConnectionDefinition,
    ) -> Result<(), EngineError> {
        let mut connections = self.connections.write().await;
        if connections.contains_key(&definition.id) {
            return Err(EngineError::ConnectionAlreadyExists(definition.id));
        }
        let record = ConnectionRecord {
            id: definition.id.clone(),
            kind: definition.kind,
            metadata: definition.metadata,
            in_use: false,
            last_borrowed_at: None,
        };
        connections.insert(definition.id, Arc::new(Mutex::new(record)));
        Ok(())
    }

    /// 插入或替换连接定义（幂等操作）。
    pub async fn upsert_connection(&self, definition: ConnectionDefinition) {
        let record = ConnectionRecord {
            id: definition.id.clone(),
            kind: definition.kind,
            metadata: definition.metadata,
            in_use: false,
            last_borrowed_at: None,
        };
        let mut connections = self.connections.write().await;
        connections.insert(definition.id, Arc::new(Mutex::new(record)));
    }

    /// 批量插入或替换连接定义。
    pub async fn upsert_connections(
        &self,
        definitions: impl IntoIterator<Item = ConnectionDefinition>,
    ) {
        let mut connections = self.connections.write().await;
        for definition in definitions {
            let record = ConnectionRecord {
                id: definition.id.clone(),
                kind: definition.kind,
                metadata: definition.metadata,
                in_use: false,
                last_borrowed_at: None,
            };
            connections.insert(definition.id, Arc::new(Mutex::new(record)));
        }
    }

    /// 按 ID 定位连接的内层 `Arc`，释放外层读锁后返回。
    ///
    /// 先取出内层 Arc 副本并释放外层读锁，避免持有外层锁跨 await。
    async fn entry(
        &self,
        connection_id: &str,
    ) -> Result<Arc<Mutex<ConnectionRecord>>, EngineError> {
        let connections = self.connections.read().await;
        connections
            .get(connection_id)
            .cloned()
            .ok_or_else(|| EngineError::ConnectionNotFound(connection_id.to_owned()))
    }

    /// 排他借出一个连接。若已被借出或不存在则返回错误。
    ///
    /// # Errors
    ///
    /// 连接不存在时返回 [`EngineError::ConnectionNotFound`]，
    /// 已被借出时返回 [`EngineError::ConnectionBusy`]。
    pub async fn borrow(&self, connection_id: &str) -> Result<ConnectionLease, EngineError> {
        let entry = self.entry(connection_id).await?;
        let mut record = entry.lock().await;
        if record.in_use {
            return Err(EngineError::ConnectionBusy(connection_id.to_owned()));
        }

        let borrowed_at = Utc::now();
        record.in_use = true;
        record.last_borrowed_at = Some(borrowed_at);

        Ok(ConnectionLease {
            id: record.id.clone(),
            kind: record.kind.clone(),
            metadata: record.metadata.clone(),
            borrowed_at,
        })
    }

    /// 将已借出的连接归还到资源池。
    ///
    /// # Errors
    ///
    /// 连接不存在时返回 [`EngineError::ConnectionNotFound`]。
    pub async fn release(&self, connection_id: &str) -> Result<(), EngineError> {
        let entry = self.entry(connection_id).await?;
        entry.lock().await.in_use = false;
        Ok(())
    }

    /// 返回单个连接记录的快照。
    pub async fn get(&self, connection_id: &str) -> Option<ConnectionRecord> {
        let entry = self.entry(connection_id).await.ok()?;
        let record = entry.lock().await;
        let snapshot = record.clone();
        Some(snapshot)
    }

    /// 返回所有已注册连接的快照列表。
    pub async fn list(&self) -> Vec<ConnectionRecord> {
        let connections = self.connections.read().await;
        let entries: Vec<Arc<Mutex<ConnectionRecord>>> = connections.values().cloned().collect();
        drop(connections);

        let mut result = Vec::with_capacity(entries.len());
        for entry in entries {
            let record = entry.lock().await;
            result.push(record.clone());
        }
        result
    }
}
