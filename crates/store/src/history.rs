//! 变量变更历史记录（ADR-0022）。

use crate::Store;
use crate::StoreError;
use rusqlite::params;

/// 历史记录条目。
#[derive(Debug, Clone)]
pub struct HistoryEntry {
    pub value: serde_json::Value,
    pub updated_at: String,
    pub updated_by: Option<String>,
}

impl Store {
    /// 记录一条变量变更历史。
    #[allow(clippy::too_many_arguments)]
    pub fn record_history(
        &self,
        workflow_id: &str,
        key: &str,
        value: &serde_json::Value,
        updated_at: &str,
        updated_by: Option<&str>,
    ) -> Result<(), StoreError> {
        let value_json = serde_json::to_string(value)?;
        self.db().execute(
            "INSERT INTO variable_history (workflow_id, key, value, updated_at, updated_by)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![workflow_id, key, value_json, updated_at, updated_by],
        )?;
        Ok(())
    }

    /// 查询指定变量的最近 N 条历史。
    pub fn query_latest(
        &self,
        workflow_id: &str,
        key: &str,
        limit: usize,
    ) -> Result<Vec<HistoryEntry>, StoreError> {
        let db = self.db();
        let mut stmt = db.prepare(
            "SELECT value, updated_at, updated_by FROM variable_history
             WHERE workflow_id = ?1 AND key = ?2
             ORDER BY updated_at DESC LIMIT ?3",
        )?;
        let limit_i64 = i64::try_from(limit).unwrap_or(i64::MAX);
        let rows = stmt.query_map(params![workflow_id, key, limit_i64], |row| {
            let value_json: String = row.get(0)?;
            let updated_at: String = row.get(1)?;
            let updated_by: Option<String> = row.get(2)?;
            Ok((value_json, updated_at, updated_by))
        })?;

        let mut result = Vec::new();
        for row in rows {
            let (value_json, updated_at, updated_by) = row?;
            result.push(HistoryEntry {
                value: serde_json::from_str(&value_json)?,
                updated_at,
                updated_by,
            });
        }
        Ok(result)
    }

    /// 按时间范围查询变量历史。
    #[allow(clippy::too_many_arguments)]
    pub fn query_history_range(
        &self,
        workflow_id: &str,
        key: &str,
        range_start: &str,
        range_end: &str,
        limit: usize,
    ) -> Result<Vec<HistoryEntry>, StoreError> {
        let db = self.db();
        let mut stmt = db.prepare(
            "SELECT value, updated_at, updated_by FROM variable_history
             WHERE workflow_id = ?1 AND key = ?2 AND updated_at >= ?3 AND updated_at <= ?4
             ORDER BY updated_at DESC LIMIT ?5",
        )?;
        let limit_i64 = i64::try_from(limit).unwrap_or(i64::MAX);
        let rows = stmt.query_map(
            params![workflow_id, key, range_start, range_end, limit_i64],
            |row| {
                let value_json: String = row.get(0)?;
                let updated_at: String = row.get(1)?;
                let updated_by: Option<String> = row.get(2)?;
                Ok((value_json, updated_at, updated_by))
            },
        )?;

        let mut result = Vec::new();
        for row in rows {
            let (value_json, updated_at, updated_by) = row?;
            result.push(HistoryEntry {
                value: serde_json::from_str(&value_json)?,
                updated_at,
                updated_by,
            });
        }
        Ok(result)
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    fn test_store() -> Store {
        Store::open_in_memory().expect("内存数据库应可打开")
    }

    #[test]
    fn record_and_query_latest_往返() {
        let store = test_store();
        store
            .record_history(
                "wf-1",
                "x",
                &serde_json::json!(1),
                "2026-05-03T10:00:00Z",
                Some("node-A"),
            )
            .unwrap();
        store
            .record_history(
                "wf-1",
                "x",
                &serde_json::json!(2),
                "2026-05-03T10:01:00Z",
                None,
            )
            .unwrap();
        store
            .record_history(
                "wf-1",
                "x",
                &serde_json::json!(3),
                "2026-05-03T10:02:00Z",
                Some("ipc"),
            )
            .unwrap();

        let entries = store.query_latest("wf-1", "x", 10).unwrap();
        assert_eq!(entries.len(), 3);
        // 降序：最新在前
        assert_eq!(entries[0].value, serde_json::json!(3));
        assert_eq!(entries[2].value, serde_json::json!(1));
    }

    #[test]
    fn query_latest_限制条数() {
        let store = test_store();
        for i in 0..5 {
            store
                .record_history(
                    "wf-1",
                    "x",
                    &serde_json::json!(i),
                    &format!("2026-05-03T10:0{i}:00Z"),
                    None,
                )
                .unwrap();
        }
        let entries = store.query_latest("wf-1", "x", 2).unwrap();
        assert_eq!(entries.len(), 2);
    }

    #[test]
    fn query_history_range_按时间过滤() {
        let store = test_store();
        store
            .record_history(
                "wf-1",
                "x",
                &serde_json::json!(1),
                "2026-05-03T10:00:00Z",
                None,
            )
            .unwrap();
        store
            .record_history(
                "wf-1",
                "x",
                &serde_json::json!(2),
                "2026-05-03T11:00:00Z",
                None,
            )
            .unwrap();
        store
            .record_history(
                "wf-1",
                "x",
                &serde_json::json!(3),
                "2026-05-03T12:00:00Z",
                None,
            )
            .unwrap();

        let entries = store
            .query_history_range(
                "wf-1",
                "x",
                "2026-05-03T10:30:00Z",
                "2026-05-03T11:30:00Z",
                10,
            )
            .unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].value, serde_json::json!(2));
    }

    #[test]
    fn query_latest_不存在的变量返回空() {
        let store = test_store();
        assert!(store.query_latest("wf-1", "nope", 10).unwrap().is_empty());
    }
}
