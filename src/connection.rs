//! 全局连接资源池。
//!
//! 节点绝不直接访问硬件。所有协议连接（Modbus、MQTT、HTTP 等）
//! 均注册在 [`ConnectionManager`] 中，通过共享的 `Arc<RwLock<ConnectionManager>>`
//! 以借出/归还模式访问。

use std::{collections::HashMap, sync::Arc};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio::sync::RwLock;

use crate::EngineError;

/// 全局连接池的线程安全句柄。
pub type SharedConnectionManager = Arc<RwLock<ConnectionManager>>;

/// 连接资源的声明式定义（用于工作流 AST）。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ConnectionDefinition {
    pub id: String,
    #[serde(rename = "type", alias = "kind")]
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
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ConnectionRecord {
    pub id: String,
    pub kind: String,
    pub metadata: Value,
    pub in_use: bool,
    pub last_borrowed_at: Option<DateTime<Utc>>,
}

/// 管理具名连接资源池，采用排他借出语义。
///
/// 当前为骨架实现，尚未对接真实的 Modbus/MQTT/HTTP 驱动。
/// 资源池保证同一连接同时只能被一个节点借出。
#[derive(Debug, Default)]
pub struct ConnectionManager {
    connections: HashMap<String, ConnectionRecord>,
}

/// 创建一个空的 [`ConnectionManager`]，包装在 `Arc<RwLock<...>>` 中。
pub fn shared_connection_manager() -> SharedConnectionManager {
    Arc::new(RwLock::new(ConnectionManager::default()))
}

impl ConnectionManager {
    /// 注册新连接。若 ID 已存在则返回错误。
    ///
    /// # Errors
    ///
    /// 连接 ID 已存在时返回 [`EngineError::ConnectionAlreadyExists`]。
    pub fn register_connection(
        &mut self,
        definition: ConnectionDefinition,
    ) -> Result<(), EngineError> {
        if self.connections.contains_key(&definition.id) {
            return Err(EngineError::ConnectionAlreadyExists(definition.id));
        }

        self.upsert_connection(definition);
        Ok(())
    }

    /// 插入或替换连接定义（幂等操作）。
    pub fn upsert_connection(&mut self, definition: ConnectionDefinition) {
        let record = ConnectionRecord {
            id: definition.id.clone(),
            kind: definition.kind,
            metadata: definition.metadata,
            in_use: false,
            last_borrowed_at: None,
        };
        self.connections.insert(definition.id, record);
    }

    /// 批量插入或替换连接定义。
    pub fn upsert_connections(
        &mut self,
        definitions: impl IntoIterator<Item = ConnectionDefinition>,
    ) {
        for definition in definitions {
            self.upsert_connection(definition);
        }
    }

    /// 排他借出一个连接。若已被借出或不存在则返回错误。
    ///
    /// # Errors
    ///
    /// 连接不存在时返回 [`EngineError::ConnectionNotFound`]，
    /// 已被借出时返回 [`EngineError::ConnectionBusy`]。
    pub fn borrow(&mut self, connection_id: &str) -> Result<ConnectionLease, EngineError> {
        let Some(record) = self.connections.get_mut(connection_id) else {
            return Err(EngineError::ConnectionNotFound(connection_id.to_owned()));
        };

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
    pub fn release(&mut self, connection_id: &str) -> Result<(), EngineError> {
        let Some(record) = self.connections.get_mut(connection_id) else {
            return Err(EngineError::ConnectionNotFound(connection_id.to_owned()));
        };

        record.in_use = false;
        Ok(())
    }

    /// 返回单个连接记录的快照。
    pub fn get(&self, connection_id: &str) -> Option<ConnectionRecord> {
        self.connections.get(connection_id).cloned()
    }

    /// 返回所有已注册连接的快照列表。
    pub fn list(&self) -> Vec<ConnectionRecord> {
        self.connections.values().cloned().collect()
    }
}
