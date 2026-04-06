use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct WorkflowContext {
    pub trace_id: Uuid,
    pub timestamp: DateTime<Utc>,
    pub payload: Value,
}

impl WorkflowContext {
    pub fn new(payload: Value) -> Self {
        Self {
            trace_id: Uuid::new_v4(),
            timestamp: Utc::now(),
            payload,
        }
    }

    pub fn from_parts(trace_id: Uuid, timestamp: DateTime<Utc>, payload: Value) -> Self {
        Self {
            trace_id,
            timestamp,
            payload,
        }
    }

    pub fn with_payload(mut self, payload: Value) -> Self {
        self.payload = payload;
        self.timestamp = Utc::now();
        self
    }

    pub fn touch(mut self) -> Self {
        self.timestamp = Utc::now();
        self
    }
}
