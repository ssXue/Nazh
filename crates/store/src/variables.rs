//! 工作流变量持久化（ADR-0022）。

use crate::Store;
use crate::StoreError;
use rusqlite::params;

/// 持久化变量记录。
#[derive(Debug, Clone)]
pub struct StoredVariable {
    pub key: String,
    pub value: serde_json::Value,
    pub var_type: String,
    pub initial: serde_json::Value,
    pub updated_at: String,
    pub updated_by: Option<String>,
}

impl Store {
    /// 写入（或更新）一个变量。
    #[allow(clippy::too_many_arguments)]
    pub fn upsert_variable(
        &self,
        workflow_id: &str,
        key: &str,
        value: &serde_json::Value,
        var_type: &str,
        initial: &serde_json::Value,
        updated_at: &str,
        updated_by: Option<&str>,
    ) -> Result<(), StoreError> {
        let value_json = serde_json::to_string(value)?;
        let initial_json = serde_json::to_string(initial)?;
        self.db().execute(
            "INSERT INTO variables (workflow_id, key, value, var_type, initial, updated_at, updated_by)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
             ON CONFLICT(workflow_id, key) DO UPDATE SET
                value = excluded.value,
                var_type = excluded.var_type,
                initial = excluded.initial,
                updated_at = excluded.updated_at,
                updated_by = excluded.updated_by",
            params![workflow_id, key, value_json, var_type, initial_json, updated_at, updated_by],
        )?;
        Ok(())
    }

    /// 加载指定工作流的所有持久化变量。
    pub fn load_variables(&self, workflow_id: &str) -> Result<Vec<StoredVariable>, StoreError> {
        let db = self.db();
        let mut stmt = db.prepare(
            "SELECT key, value, var_type, initial, updated_at, updated_by FROM variables WHERE workflow_id = ?1",
        )?;
        let rows = stmt.query_map([workflow_id], |row| {
            let key: String = row.get(0)?;
            let value_json: String = row.get(1)?;
            let var_type: String = row.get(2)?;
            let initial_json: String = row.get(3)?;
            let updated_at: String = row.get(4)?;
            let updated_by: Option<String> = row.get(5)?;
            Ok((
                key,
                value_json,
                var_type,
                initial_json,
                updated_at,
                updated_by,
            ))
        })?;

        let mut result = Vec::new();
        for row in rows {
            let (key, value_json, var_type, initial_json, updated_at, updated_by) = row?;
            result.push(StoredVariable {
                key,
                value: serde_json::from_str(&value_json)?,
                var_type,
                initial: serde_json::from_str(&initial_json)?,
                updated_at,
                updated_by,
            });
        }
        Ok(result)
    }

    /// 删除指定变量。
    pub fn delete_variable(&self, workflow_id: &str, key: &str) -> Result<(), StoreError> {
        self.db().execute(
            "DELETE FROM variables WHERE workflow_id = ?1 AND key = ?2",
            params![workflow_id, key],
        )?;
        Ok(())
    }

    /// 删除指定工作流的所有变量。
    pub fn delete_all_variables(&self, workflow_id: &str) -> Result<(), StoreError> {
        self.db().execute(
            "DELETE FROM variables WHERE workflow_id = ?1",
            [workflow_id],
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
    fn open_in_memory_成功() {
        test_store();
    }

    #[test]
    fn upsert_and_load_往返() {
        let store = test_store();
        store
            .upsert_variable(
                "wf-1",
                "counter",
                &serde_json::json!(42),
                "Integer",
                &serde_json::json!(0),
                "2026-05-03T10:00:00Z",
                Some("node-A"),
            )
            .unwrap();

        let vars = store.load_variables("wf-1").unwrap();
        assert_eq!(vars.len(), 1);
        assert_eq!(vars[0].key, "counter");
        assert_eq!(vars[0].value, serde_json::json!(42));
        assert_eq!(vars[0].var_type, "Integer");
        assert_eq!(vars[0].initial, serde_json::json!(0));
        assert_eq!(vars[0].updated_by.as_deref(), Some("node-A"));
    }

    #[test]
    fn upsert_覆盖旧值() {
        let store = test_store();
        store
            .upsert_variable(
                "wf-1",
                "x",
                &serde_json::json!(1),
                "Integer",
                &serde_json::json!(0),
                "t1",
                None,
            )
            .unwrap();
        store
            .upsert_variable(
                "wf-1",
                "x",
                &serde_json::json!(2),
                "Integer",
                &serde_json::json!(0),
                "t2",
                Some("ipc"),
            )
            .unwrap();

        let vars = store.load_variables("wf-1").unwrap();
        assert_eq!(vars.len(), 1);
        assert_eq!(vars[0].value, serde_json::json!(2));
    }

    #[test]
    fn delete_variable_移除指定键() {
        let store = test_store();
        store
            .upsert_variable(
                "wf-1",
                "a",
                &serde_json::json!(1),
                "Integer",
                &serde_json::json!(0),
                "t",
                None,
            )
            .unwrap();
        store
            .upsert_variable(
                "wf-1",
                "b",
                &serde_json::json!(2),
                "Integer",
                &serde_json::json!(0),
                "t",
                None,
            )
            .unwrap();

        store.delete_variable("wf-1", "a").unwrap();
        let vars = store.load_variables("wf-1").unwrap();
        assert_eq!(vars.len(), 1);
        assert_eq!(vars[0].key, "b");
    }

    #[test]
    fn delete_all_variables_清空工作流() {
        let store = test_store();
        store
            .upsert_variable(
                "wf-1",
                "a",
                &serde_json::json!(1),
                "Integer",
                &serde_json::json!(0),
                "t",
                None,
            )
            .unwrap();
        store
            .upsert_variable(
                "wf-2",
                "b",
                &serde_json::json!(2),
                "Integer",
                &serde_json::json!(0),
                "t",
                None,
            )
            .unwrap();

        store.delete_all_variables("wf-1").unwrap();
        assert!(store.load_variables("wf-1").unwrap().is_empty());
        assert_eq!(store.load_variables("wf-2").unwrap().len(), 1);
    }

    #[test]
    fn load_不存在的_工作流返回空() {
        let store = test_store();
        assert!(store.load_variables("nope").unwrap().is_empty());
    }
}
