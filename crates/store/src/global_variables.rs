//! 全局变量（跨工作流共享，ADR-0022）。

use crate::Store;
use crate::StoreError;
use rusqlite::{OptionalExtension, params};

/// 持久化全局变量记录。
#[derive(Debug, Clone)]
pub struct StoredGlobalVariable {
    pub namespace: String,
    pub key: String,
    pub value: serde_json::Value,
    pub var_type: String,
    pub updated_at: String,
    pub updated_by: Option<String>,
}

impl Store {
    /// 写入（或更新）一个全局变量。
    #[allow(clippy::too_many_arguments)]
    pub fn upsert_global(
        &self,
        namespace: &str,
        key: &str,
        value: &serde_json::Value,
        var_type: &str,
        updated_at: &str,
        updated_by: Option<&str>,
    ) -> Result<(), StoreError> {
        let value_json = serde_json::to_string(value)?;
        self.db().execute(
            "INSERT INTO global_variables (namespace, key, value, var_type, updated_at, updated_by)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)
             ON CONFLICT(namespace, key) DO UPDATE SET
                value = excluded.value,
                var_type = excluded.var_type,
                updated_at = excluded.updated_at,
                updated_by = excluded.updated_by",
            params![namespace, key, value_json, var_type, updated_at, updated_by],
        )?;
        Ok(())
    }

    /// 读取一个全局变量。
    pub fn load_global(
        &self,
        namespace: &str,
        key: &str,
    ) -> Result<Option<StoredGlobalVariable>, StoreError> {
        let db = self.db();
        let result = db
            .query_row(
                "SELECT namespace, key, value, var_type, updated_at, updated_by
                 FROM global_variables WHERE namespace = ?1 AND key = ?2",
                params![namespace, key],
                |row| {
                    let namespace: String = row.get(0)?;
                    let key: String = row.get(1)?;
                    let value_json: String = row.get(2)?;
                    let var_type: String = row.get(3)?;
                    let updated_at: String = row.get(4)?;
                    let updated_by: Option<String> = row.get(5)?;
                    Ok((namespace, key, value_json, var_type, updated_at, updated_by))
                },
            )
            .optional()?;

        match result {
            Some((namespace, key, value_json, var_type, updated_at, updated_by)) => {
                Ok(Some(StoredGlobalVariable {
                    namespace,
                    key,
                    value: serde_json::from_str(&value_json)?,
                    var_type,
                    updated_at,
                    updated_by,
                }))
            }
            None => Ok(None),
        }
    }

    /// 列出全局变量。指定 namespace 时过滤，否则返回全部。
    pub fn list_globals(
        &self,
        namespace: Option<&str>,
    ) -> Result<Vec<StoredGlobalVariable>, StoreError> {
        let sql = if namespace.is_some() {
            "SELECT namespace, key, value, var_type, updated_at, updated_by
             FROM global_variables WHERE namespace = ?1 ORDER BY namespace, key"
        } else {
            "SELECT namespace, key, value, var_type, updated_at, updated_by
             FROM global_variables ORDER BY namespace, key"
        };
        let db = self.db();
        let mut stmt = db.prepare(sql)?;

        let mapped = |row: &rusqlite::Row<'_>| -> rusqlite::Result<StoredGlobalVariable> {
            let value_json: String = row.get(2)?;
            Ok(StoredGlobalVariable {
                namespace: row.get(0)?,
                key: row.get(1)?,
                value: serde_json::from_str(&value_json)
                    .map_err(|e| rusqlite::Error::ToSqlConversionFailure(Box::new(e)))?,
                var_type: row.get(3)?,
                updated_at: row.get(4)?,
                updated_by: row.get(5)?,
            })
        };

        let rows: rusqlite::MappedRows<_> = if let Some(ns) = namespace {
            stmt.query_map(params![ns], mapped)?
        } else {
            stmt.query_map([], mapped)?
        };

        Ok(rows.collect::<Result<Vec<_>, _>>()?)
    }

    /// 删除一个全局变量。
    pub fn delete_global(&self, namespace: &str, key: &str) -> Result<(), StoreError> {
        self.db().execute(
            "DELETE FROM global_variables WHERE namespace = ?1 AND key = ?2",
            params![namespace, key],
        )?;
        Ok(())
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
    fn upsert_and_load_往返() {
        let store = test_store();
        store
            .upsert_global(
                "factory",
                "line_id",
                &serde_json::json!("L1"),
                "String",
                "t1",
                Some("admin"),
            )
            .unwrap();

        let g = store.load_global("factory", "line_id").unwrap().unwrap();
        assert_eq!(g.key, "line_id");
        assert_eq!(g.value, serde_json::json!("L1"));
        assert_eq!(g.namespace, "factory");
    }

    #[test]
    fn load_不存在返回_none() {
        let store = test_store();
        assert!(store.load_global("no", "nope").unwrap().is_none());
    }

    #[test]
    fn list_globals_按_namespace_过滤() {
        let store = test_store();
        store
            .upsert_global("ns1", "a", &serde_json::json!(1), "Integer", "t", None)
            .unwrap();
        store
            .upsert_global("ns1", "b", &serde_json::json!(2), "Integer", "t", None)
            .unwrap();
        store
            .upsert_global("ns2", "c", &serde_json::json!(3), "Integer", "t", None)
            .unwrap();

        let ns1 = store.list_globals(Some("ns1")).unwrap();
        assert_eq!(ns1.len(), 2);
        let all = store.list_globals(None).unwrap();
        assert_eq!(all.len(), 3);
    }

    #[test]
    fn delete_global_移除指定键() {
        let store = test_store();
        store
            .upsert_global("ns", "x", &serde_json::json!(1), "Integer", "t", None)
            .unwrap();
        store.delete_global("ns", "x").unwrap();
        assert!(store.load_global("ns", "x").unwrap().is_none());
    }
}
