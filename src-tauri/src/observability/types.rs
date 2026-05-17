//! 可观测性类型定义与常量。

use std::collections::HashMap;
use std::sync::Arc;

use chrono::{DateTime, Utc};
use serde_json::Value;
use store::{BatchWriter, ObservabilityBatchItem};
use tokio::sync::Mutex;

pub type SharedObservabilityStore = Arc<ObservabilityStore>;

#[derive(Debug, Clone)]
pub(crate) struct ObservabilitySession {
    pub(crate) project_id: String,
    pub(crate) project_name: String,
    pub(crate) environment_id: String,
    pub(crate) environment_name: String,
}

#[derive(Debug, Default)]
pub(crate) struct ObservabilityRuntimeState {
    pub(crate) active_spans: HashMap<String, DateTime<Utc>>,
}

pub struct ObservabilityStore {
    pub(crate) session: ObservabilitySession,
    pub(crate) state: Mutex<ObservabilityRuntimeState>,
    pub(crate) batch_writer: Option<BatchWriter<ObservabilityBatchItem>>,
}

pub(crate) struct ObservabilityEntryDraft {
    pub(crate) level: String,
    pub(crate) category: String,
    pub(crate) source: String,
    pub(crate) message: String,
    pub(crate) detail: Option<String>,
    pub(crate) trace_id: Option<String>,
    pub(crate) node_id: Option<String>,
    pub(crate) duration_ms: Option<u64>,
    pub(crate) data: Option<Value>,
    pub(crate) timestamp: DateTime<Utc>,
    pub(crate) event_kind: Option<String>,
}

impl ObservabilityEntryDraft {
    pub(crate) fn execution(
        level: &str,
        kind: &str,
        source: String,
        message: String,
        trace_id: String,
        timestamp: DateTime<Utc>,
    ) -> Self {
        Self {
            level: level.to_owned(),
            category: "execution".to_owned(),
            source,
            message,
            detail: None,
            trace_id: Some(trace_id),
            node_id: None,
            duration_ms: None,
            data: None,
            timestamp,
            event_kind: Some(kind.to_owned()),
        }
    }

    pub(crate) fn with_detail(mut self, detail: Option<String>) -> Self {
        self.detail = detail;
        self
    }

    pub(crate) fn with_node_id(mut self, node_id: Option<String>) -> Self {
        self.node_id = node_id;
        self
    }

    pub(crate) fn with_duration_ms(mut self, duration_ms: Option<u64>) -> Self {
        self.duration_ms = duration_ms;
        self
    }
}

// ---- 纯工具函数 ----

pub(crate) fn elapsed_ms_since(now: DateTime<Utc>, started_at: DateTime<Utc>) -> u64 {
    (now - started_at).num_milliseconds().max(0).cast_unsigned()
}

pub(crate) fn span_key(trace_id: impl std::fmt::Display, stage: &str) -> String {
    format!("{trace_id}:{stage}")
}

pub(crate) fn build_record_id(prefix: &str, timestamp: &DateTime<Utc>) -> String {
    format!(
        "{prefix}-{}-{}",
        timestamp.timestamp_millis(),
        timestamp.timestamp_subsec_nanos()
    )
}

pub(crate) fn summarize_payload(payload: &Value) -> Value {
    match payload {
        Value::Object(map) => {
            let mut summary = serde_json::Map::new();
            for (key, value) in map.iter().take(8) {
                summary.insert(key.clone(), truncate_value(value, 220));
            }
            Value::Object(summary)
        }
        other => truncate_value(other, 220),
    }
}

fn truncate_value(value: &Value, max_text_len: usize) -> Value {
    match value {
        Value::String(text) => {
            if text.chars().count() <= max_text_len {
                Value::String(text.clone())
            } else {
                Value::String(text.chars().take(max_text_len).collect::<String>())
            }
        }
        Value::Array(items) => Value::Array(
            items
                .iter()
                .take(12)
                .map(|item| truncate_value(item, max_text_len))
                .collect(),
        ),
        Value::Object(map) => {
            let mut next = serde_json::Map::new();
            for (key, nested) in map.iter().take(12) {
                next.insert(key.clone(), truncate_value(nested, max_text_len));
            }
            Value::Object(next)
        }
        other => other.clone(),
    }
}

pub(crate) fn min_timestamp(current: Option<String>, next: Option<String>) -> Option<String> {
    match (current, next) {
        (Some(left), Some(right)) => Some(if left <= right { left } else { right }),
        (Some(left), None) => Some(left),
        (None, Some(right)) => Some(right),
        (None, None) => None,
    }
}

pub(crate) fn max_timestamp(current: Option<String>, next: Option<String>) -> Option<String> {
    match (current, next) {
        (Some(left), Some(right)) => Some(if left >= right { left } else { right }),
        (Some(left), None) => Some(left),
        (None, Some(right)) => Some(right),
        (None, None) => None,
    }
}

pub(crate) fn entry_search_text(entry: &tauri_bindings::ObservabilityEntry) -> String {
    format!(
        "{} {} {} {} {} {} {} {}",
        entry.level,
        entry.category,
        entry.source,
        entry.message,
        entry.detail.as_deref().unwrap_or_default(),
        entry.trace_id.as_deref().unwrap_or_default(),
        entry.node_id.as_deref().unwrap_or_default(),
        entry.project_name.as_deref().unwrap_or_default(),
    )
}
