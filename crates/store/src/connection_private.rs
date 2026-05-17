//! 连接私有配置：密钥与本机覆盖（ADR-0025）。
//!
//! 工程连接资产不进入 Store；本模块只保存不应写入 YAML 的本机私有数据。

use crate::{Store, StoreError};
use rusqlite::{OptionalExtension, params};

/// 持久化连接密钥记录。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredConnectionSecret {
    pub connection_id: String,
    pub secret_key: String,
    pub value: String,
    pub updated_at: String,
    pub updated_by: Option<String>,
}

/// 持久化连接本机覆盖记录。
#[derive(Debug, Clone, PartialEq)]
pub struct StoredConnectionLocalOverride {
    pub connection_id: String,
    pub environment_id: String,
    pub key: String,
    pub value: serde_json::Value,
    pub updated_at: String,
    pub updated_by: Option<String>,
}

impl Store {
    /// 写入或更新一个连接密钥。
    pub fn upsert_connection_secret(
        &self,
        connection_id: &str,
        secret_key: &str,
        value: &str,
        updated_at: &str,
        updated_by: Option<&str>,
    ) -> Result<(), StoreError> {
        self.db().execute(
            "INSERT INTO connection_secrets
                (connection_id, secret_key, value, updated_at, updated_by)
             VALUES (?1, ?2, ?3, ?4, ?5)
             ON CONFLICT(connection_id, secret_key) DO UPDATE SET
                value = excluded.value,
                updated_at = excluded.updated_at,
                updated_by = excluded.updated_by",
            params![connection_id, secret_key, value, updated_at, updated_by],
        )?;
        Ok(())
    }

    /// 读取一个连接密钥。
    pub fn load_connection_secret(
        &self,
        connection_id: &str,
        secret_key: &str,
    ) -> Result<Option<StoredConnectionSecret>, StoreError> {
        self.db()
            .query_row(
                "SELECT connection_id, secret_key, value, updated_at, updated_by
                 FROM connection_secrets
                 WHERE connection_id = ?1 AND secret_key = ?2",
                params![connection_id, secret_key],
                row_to_secret,
            )
            .optional()
            .map_err(Into::into)
    }

    /// 列出指定连接的所有密钥。
    pub fn list_connection_secrets(
        &self,
        connection_id: &str,
    ) -> Result<Vec<StoredConnectionSecret>, StoreError> {
        let db = self.db();
        let mut stmt = db.prepare(
            "SELECT connection_id, secret_key, value, updated_at, updated_by
             FROM connection_secrets
             WHERE connection_id = ?1
             ORDER BY secret_key",
        )?;
        let rows = stmt.query_map(params![connection_id], row_to_secret)?;
        Ok(rows.collect::<Result<Vec<_>, _>>()?)
    }

    /// 删除一个连接密钥。
    pub fn delete_connection_secret(
        &self,
        connection_id: &str,
        secret_key: &str,
    ) -> Result<(), StoreError> {
        self.db().execute(
            "DELETE FROM connection_secrets
             WHERE connection_id = ?1 AND secret_key = ?2",
            params![connection_id, secret_key],
        )?;
        Ok(())
    }

    /// 写入或更新一个连接本机覆盖。
    #[allow(clippy::too_many_arguments)]
    pub fn upsert_connection_local_override(
        &self,
        connection_id: &str,
        environment_id: &str,
        key: &str,
        value: &serde_json::Value,
        updated_at: &str,
        updated_by: Option<&str>,
    ) -> Result<(), StoreError> {
        let value_json = serde_json::to_string(value)?;
        self.db().execute(
            "INSERT INTO connection_local_overrides
                (connection_id, environment_id, key, value, updated_at, updated_by)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)
             ON CONFLICT(connection_id, environment_id, key) DO UPDATE SET
                value = excluded.value,
                updated_at = excluded.updated_at,
                updated_by = excluded.updated_by",
            params![
                connection_id,
                environment_id,
                key,
                value_json,
                updated_at,
                updated_by
            ],
        )?;
        Ok(())
    }

    /// 读取一个连接本机覆盖。
    pub fn load_connection_local_override(
        &self,
        connection_id: &str,
        environment_id: &str,
        key: &str,
    ) -> Result<Option<StoredConnectionLocalOverride>, StoreError> {
        let result = self
            .db()
            .query_row(
                "SELECT connection_id, environment_id, key, value, updated_at, updated_by
                 FROM connection_local_overrides
                 WHERE connection_id = ?1 AND environment_id = ?2 AND key = ?3",
                params![connection_id, environment_id, key],
                row_to_local_override_tuple,
            )
            .optional()?;
        result.map(tuple_to_local_override).transpose()
    }

    /// 列出连接本机覆盖。`environment_id` 为 `Some` 时按环境过滤。
    pub fn list_connection_local_overrides(
        &self,
        connection_id: &str,
        environment_id: Option<&str>,
    ) -> Result<Vec<StoredConnectionLocalOverride>, StoreError> {
        let sql = if environment_id.is_some() {
            "SELECT connection_id, environment_id, key, value, updated_at, updated_by
             FROM connection_local_overrides
             WHERE connection_id = ?1 AND environment_id = ?2
             ORDER BY environment_id, key"
        } else {
            "SELECT connection_id, environment_id, key, value, updated_at, updated_by
             FROM connection_local_overrides
             WHERE connection_id = ?1
             ORDER BY environment_id, key"
        };
        let db = self.db();
        let mut stmt = db.prepare(sql)?;
        let rows = if let Some(env_id) = environment_id {
            stmt.query_map(params![connection_id, env_id], row_to_local_override_tuple)?
        } else {
            stmt.query_map(params![connection_id], row_to_local_override_tuple)?
        };
        let tuples = rows.collect::<Result<Vec<_>, _>>()?;
        tuples.into_iter().map(tuple_to_local_override).collect()
    }

    /// 删除一个连接本机覆盖。
    pub fn delete_connection_local_override(
        &self,
        connection_id: &str,
        environment_id: &str,
        key: &str,
    ) -> Result<(), StoreError> {
        self.db().execute(
            "DELETE FROM connection_local_overrides
             WHERE connection_id = ?1 AND environment_id = ?2 AND key = ?3",
            params![connection_id, environment_id, key],
        )?;
        Ok(())
    }
}

fn row_to_secret(row: &rusqlite::Row<'_>) -> rusqlite::Result<StoredConnectionSecret> {
    Ok(StoredConnectionSecret {
        connection_id: row.get(0)?,
        secret_key: row.get(1)?,
        value: row.get(2)?,
        updated_at: row.get(3)?,
        updated_by: row.get(4)?,
    })
}

type LocalOverrideRow = (String, String, String, String, String, Option<String>);

fn row_to_local_override_tuple(row: &rusqlite::Row<'_>) -> rusqlite::Result<LocalOverrideRow> {
    Ok((
        row.get(0)?,
        row.get(1)?,
        row.get(2)?,
        row.get(3)?,
        row.get(4)?,
        row.get(5)?,
    ))
}

fn tuple_to_local_override(
    (connection_id, environment_id, key, value_json, updated_at, updated_by): LocalOverrideRow,
) -> Result<StoredConnectionLocalOverride, StoreError> {
    Ok(StoredConnectionLocalOverride {
        connection_id,
        environment_id,
        key,
        value: serde_json::from_str(&value_json)?,
        updated_at,
        updated_by,
    })
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    fn test_store() -> Store {
        Store::open_in_memory().expect("内存 Store 应可打开")
    }

    #[test]
    fn connection_secret_读写覆盖删除() {
        let store = test_store();
        store
            .upsert_connection_secret("mqtt-main", "password", "old", "t1", Some("u1"))
            .unwrap();
        store
            .upsert_connection_secret("mqtt-main", "password", "new", "t2", Some("u2"))
            .unwrap();

        let secret = store
            .load_connection_secret("mqtt-main", "password")
            .unwrap()
            .unwrap();
        assert_eq!(secret.value, "new");
        assert_eq!(secret.updated_at, "t2");
        assert_eq!(secret.updated_by.as_deref(), Some("u2"));

        store
            .delete_connection_secret("mqtt-main", "password")
            .unwrap();
        assert!(
            store
                .load_connection_secret("mqtt-main", "password")
                .unwrap()
                .is_none()
        );
    }

    #[test]
    fn connection_secret_按连接列出() {
        let store = test_store();
        store
            .upsert_connection_secret("a", "password", "p", "t", None)
            .unwrap();
        store
            .upsert_connection_secret("a", "username", "u", "t", None)
            .unwrap();
        store
            .upsert_connection_secret("b", "password", "other", "t", None)
            .unwrap();

        let secrets = store.list_connection_secrets("a").unwrap();
        assert_eq!(secrets.len(), 2);
        assert_eq!(secrets[0].secret_key, "password");
        assert_eq!(secrets[1].secret_key, "username");
    }

    #[test]
    fn connection_local_override_读写覆盖删除() {
        let store = test_store();
        store
            .upsert_connection_local_override(
                "serial-main",
                "env-prod",
                "port_path",
                &serde_json::json!("/dev/ttyUSB0"),
                "t1",
                None,
            )
            .unwrap();
        store
            .upsert_connection_local_override(
                "serial-main",
                "env-prod",
                "port_path",
                &serde_json::json!("/dev/ttyUSB1"),
                "t2",
                Some("operator"),
            )
            .unwrap();

        let value = store
            .load_connection_local_override("serial-main", "env-prod", "port_path")
            .unwrap()
            .unwrap();
        assert_eq!(value.value, serde_json::json!("/dev/ttyUSB1"));
        assert_eq!(value.updated_by.as_deref(), Some("operator"));

        store
            .delete_connection_local_override("serial-main", "env-prod", "port_path")
            .unwrap();
        assert!(
            store
                .load_connection_local_override("serial-main", "env-prod", "port_path")
                .unwrap()
                .is_none()
        );
    }

    #[test]
    fn connection_local_override_按环境过滤() {
        let store = test_store();
        store
            .upsert_connection_local_override(
                "eth-main",
                "env-prod",
                "interface",
                &serde_json::json!("en0"),
                "t",
                None,
            )
            .unwrap();
        store
            .upsert_connection_local_override(
                "eth-main",
                "env-dev",
                "interface",
                &serde_json::json!("lo0"),
                "t",
                None,
            )
            .unwrap();

        let all = store
            .list_connection_local_overrides("eth-main", None)
            .unwrap();
        assert_eq!(all.len(), 2);

        let prod = store
            .list_connection_local_overrides("eth-main", Some("env-prod"))
            .unwrap();
        assert_eq!(prod.len(), 1);
        assert_eq!(prod[0].value, serde_json::json!("en0"));
    }
}
