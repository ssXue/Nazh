//! AI 配置 Store 持久化（RFC-0003 Phase 4）。

use crate::{Store, StoreError};

impl Store {
    /// 读取 AI 配置 JSON。
    pub fn load_ai_config(&self) -> Result<Option<String>, StoreError> {
        let result = self.db().query_row(
            "SELECT value FROM ai_config WHERE key = 'config_json'",
            [],
            |row| row.get(0),
        );
        match result {
            Ok(json) => Ok(Some(json)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(StoreError::from(e)),
        }
    }

    /// 写入 AI 配置 JSON（整体替换）。
    pub fn save_ai_config(&self, json: &str) -> Result<(), StoreError> {
        self.db().execute(
            "INSERT OR REPLACE INTO ai_config (key, value) VALUES ('config_json', ?1)",
            rusqlite::params![json],
        )?;
        Ok(())
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    fn test_store() -> Store {
        Store::open_in_memory().expect("内存 Store 应可打开")
    }

    #[test]
    fn 读写往返() {
        let store = test_store();
        assert!(store.load_ai_config().unwrap().is_none());

        let config = r#"{"version":1,"providers":[]}"#;
        store.save_ai_config(config).unwrap();

        let loaded = store.load_ai_config().unwrap();
        assert_eq!(loaded.as_deref(), Some(config));
    }

    #[test]
    fn save_覆盖旧值() {
        let store = test_store();
        store.save_ai_config("old").unwrap();
        store.save_ai_config("new").unwrap();
        assert_eq!(store.load_ai_config().unwrap().as_deref(), Some("new"));
    }
}
