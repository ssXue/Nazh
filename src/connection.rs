use std::{collections::HashMap, sync::Arc};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio::sync::RwLock;

use crate::EngineError;

pub type SharedConnectionManager = Arc<RwLock<ConnectionManager>>;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ConnectionDefinition {
    pub id: String,
    #[serde(rename = "type", alias = "kind")]
    pub kind: String,
    #[serde(default)]
    pub metadata: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ConnectionLease {
    pub id: String,
    pub kind: String,
    pub metadata: Value,
    pub borrowed_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ConnectionRecord {
    pub id: String,
    pub kind: String,
    pub metadata: Value,
    pub in_use: bool,
    pub last_borrowed_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Default)]
pub struct ConnectionManager {
    connections: HashMap<String, ConnectionRecord>,
}

pub fn shared_connection_manager() -> SharedConnectionManager {
    Arc::new(RwLock::new(ConnectionManager::default()))
}

impl ConnectionManager {
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

    pub fn upsert_connections(
        &mut self,
        definitions: impl IntoIterator<Item = ConnectionDefinition>,
    ) {
        for definition in definitions {
            self.upsert_connection(definition);
        }
    }

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

    pub fn release(&mut self, connection_id: &str) -> Result<(), EngineError> {
        let Some(record) = self.connections.get_mut(connection_id) else {
            return Err(EngineError::ConnectionNotFound(connection_id.to_owned()));
        };

        record.in_use = false;
        Ok(())
    }

    pub fn get(&self, connection_id: &str) -> Option<ConnectionRecord> {
        self.connections.get(connection_id).cloned()
    }

    pub fn list(&self) -> Vec<ConnectionRecord> {
        self.connections.values().cloned().collect()
    }
}
