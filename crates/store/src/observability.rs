//! 可观测性事件索引（RFC-0003 Phase 2）。

use crate::{Store, StoreError};
use rusqlite::params;

/// 批量写入的一行数据元组。
pub(crate) type ObservabilityBatchRow = (
    String,
    String,
    String,
    String,
    Option<String>,
    String,
    serde_json::Value,
);

/// 存入 `SQLite` 的可观测性记录。
#[derive(Debug, Clone)]
pub struct StoredObservabilityRecord {
    pub id: String,
    pub record_kind: String,
    pub category: String,
    pub timestamp: String,
    pub trace_id: Option<String>,
    pub payload: serde_json::Value,
}

impl Store {
    /// 写入一条可观测性记录。`payload` 保存原始 IPC 结构，索引用列服务查询。
    #[allow(clippy::too_many_arguments)]
    pub fn insert_observability_record(
        &self,
        id: &str,
        record_kind: &str,
        category: &str,
        timestamp: &str,
        trace_id: Option<&str>,
        search_text: &str,
        payload: &serde_json::Value,
    ) -> Result<(), StoreError> {
        let payload_json = serde_json::to_string(payload)?;
        self.db().execute(
            "INSERT OR REPLACE INTO observability_records
                (id, record_kind, category, timestamp, trace_id, search_text, payload)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                id,
                record_kind,
                category,
                timestamp,
                trace_id,
                search_text,
                payload_json
            ],
        )?;
        Ok(())
    }

    /// 查询最近的可观测性记录。
    pub fn query_observability_records(
        &self,
        trace_id: Option<&str>,
        search: Option<&str>,
        limit: usize,
    ) -> Result<Vec<StoredObservabilityRecord>, StoreError> {
        let db = self.db();
        let search_like = search.map(|value| format!("%{}%", value.to_ascii_lowercase()));
        let limit = i64::try_from(limit).unwrap_or(i64::MAX);

        let mut records = Vec::new();
        match (trace_id, search_like.as_deref()) {
            (Some(trace_id), Some(search)) => {
                let mut stmt = db.prepare(
                    "SELECT id, record_kind, category, timestamp, trace_id, payload
                     FROM observability_records
                     WHERE trace_id = ?1 AND lower(search_text) LIKE ?2
                     ORDER BY timestamp DESC, id DESC
                     LIMIT ?3",
                )?;
                let rows = stmt.query_map(params![trace_id, search, limit], map_record_row)?;
                collect_records(rows, &mut records)?;
            }
            (Some(trace_id), None) => {
                let mut stmt = db.prepare(
                    "SELECT id, record_kind, category, timestamp, trace_id, payload
                     FROM observability_records
                     WHERE trace_id = ?1
                     ORDER BY timestamp DESC, id DESC
                     LIMIT ?2",
                )?;
                let rows = stmt.query_map(params![trace_id, limit], map_record_row)?;
                collect_records(rows, &mut records)?;
            }
            (None, Some(search)) => {
                let mut stmt = db.prepare(
                    "SELECT id, record_kind, category, timestamp, trace_id, payload
                     FROM observability_records
                     WHERE lower(search_text) LIKE ?1
                     ORDER BY timestamp DESC, id DESC
                     LIMIT ?2",
                )?;
                let rows = stmt.query_map(params![search, limit], map_record_row)?;
                collect_records(rows, &mut records)?;
            }
            (None, None) => {
                let mut stmt = db.prepare(
                    "SELECT id, record_kind, category, timestamp, trace_id, payload
                     FROM observability_records
                     ORDER BY timestamp DESC, id DESC
                     LIMIT ?1",
                )?;
                let rows = stmt.query_map(params![limit], map_record_row)?;
                collect_records(rows, &mut records)?;
            }
        }

        Ok(records)
    }

    /// 批量写入可观测性记录。在单个事务中 INSERT，失败时整批回滚。
    pub fn insert_observability_record_batch(
        &self,
        records: &[ObservabilityBatchRow],
    ) -> Result<(), StoreError> {
        if records.is_empty() {
            return Ok(());
        }
        let db = self.db();
        let tx = db.unchecked_transaction()?;
        for (id, record_kind, category, timestamp, trace_id, search_text, payload) in records {
            let payload_json = serde_json::to_string(payload)?;
            tx.execute(
                "INSERT OR REPLACE INTO observability_records
                    (id, record_kind, category, timestamp, trace_id, search_text, payload)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                params![
                    id,
                    record_kind,
                    category,
                    timestamp,
                    trace_id,
                    search_text,
                    payload_json
                ],
            )?;
        }
        tx.commit()?;
        Ok(())
    }

    /// 清空可观测性索引。
    pub fn clear_observability_records(&self) -> Result<(), StoreError> {
        self.db().execute("DELETE FROM observability_records", [])?;
        Ok(())
    }
}

fn map_record_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<StoredObservabilityRecord> {
    let payload_json: String = row.get(5)?;
    let payload = serde_json::from_str(&payload_json)
        .map_err(|error| rusqlite::Error::ToSqlConversionFailure(Box::new(error)))?;
    Ok(StoredObservabilityRecord {
        id: row.get(0)?,
        record_kind: row.get(1)?,
        category: row.get(2)?,
        timestamp: row.get(3)?,
        trace_id: row.get(4)?,
        payload,
    })
}

fn collect_records<I>(
    rows: I,
    records: &mut Vec<StoredObservabilityRecord>,
) -> Result<(), StoreError>
where
    I: IntoIterator<Item = rusqlite::Result<StoredObservabilityRecord>>,
{
    for row in rows {
        records.push(row?);
    }
    Ok(())
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    fn test_store() -> Store {
        Store::open_in_memory().expect("内存数据库应可打开")
    }

    #[test]
    fn insert_and_query_by_trace() {
        let store = test_store();
        store
            .insert_observability_record(
                "event-1",
                "entry",
                "execution",
                "2026-05-16T00:00:00Z",
                Some("trace-1"),
                "node started trace-1",
                &serde_json::json!({"id": "event-1", "traceId": "trace-1"}),
            )
            .unwrap();

        let records = store
            .query_observability_records(Some("trace-1"), Some("started"), 20)
            .unwrap();
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].id, "event-1");
        assert_eq!(records[0].record_kind, "entry");
    }

    #[test]
    fn clear_removes_records() {
        let store = test_store();
        store
            .insert_observability_record(
                "audit-1",
                "entry",
                "audit",
                "2026-05-16T00:00:00Z",
                None,
                "deploy",
                &serde_json::json!({"id": "audit-1"}),
            )
            .unwrap();
        store.clear_observability_records().unwrap();
        assert!(
            store
                .query_observability_records(None, None, 20)
                .unwrap()
                .is_empty()
        );
    }
}
