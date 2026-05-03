//! Capability 资产持久化（RFC-0004 Phase 2）。

use crate::{Store, StoreError};
use rusqlite::params;

/// 能力资产摘要（列表视图）。
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct CapabilitySummary {
    pub id: String,
    pub device_id: String,
    pub name: String,
    pub description: Option<String>,
    pub version: i64,
    pub updated_at: String,
}

/// 能力资产完整记录。
#[derive(Debug, Clone)]
pub struct StoredCapability {
    pub id: String,
    pub device_id: String,
    pub name: String,
    pub description: Option<String>,
    pub version: i64,
    pub spec_json: serde_json::Value,
    pub created_at: String,
    pub updated_at: String,
}

/// 能力资产版本记录。
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct StoredCapabilityVersion {
    pub capability_id: String,
    pub version: i64,
    pub spec_json: serde_json::Value,
    pub source_summary: Option<String>,
    pub created_at: String,
}

/// 能力版本摘要。
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct CapabilityVersionSummary {
    pub version: i64,
    pub created_at: String,
    pub source_summary: Option<String>,
}

/// AI 抽取来源追溯记录。
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct CapabilitySource {
    pub field_path: String,
    pub source_text: String,
    pub confidence: f64,
}

impl Store {
    /// 保存（或更新）能力资产，自动递增版本号。
    pub fn save_capability(
        &self,
        id: &str,
        device_id: &str,
        name: &str,
        description: Option<&str>,
        spec_json: &serde_json::Value,
    ) -> Result<(), StoreError> {
        let spec_str = serde_json::to_string(spec_json)?;
        let db = self.db();

        let current_version: Option<i64> = db
            .query_row(
                "SELECT version FROM capability_assets WHERE id = ?1",
                [id],
                |row| row.get(0),
            )
            .ok();

        let new_version = current_version.map_or(1, |v| v + 1);

        db.execute(
            "INSERT INTO capability_assets (id, device_id, name, description, version, spec_json, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, datetime('now'), datetime('now'))
             ON CONFLICT(id) DO UPDATE SET
                device_id = excluded.device_id,
                name = excluded.name,
                description = excluded.description,
                version = excluded.version,
                spec_json = excluded.spec_json,
                updated_at = datetime('now')",
            params![id, device_id, name, description, new_version, spec_str],
        )?;

        db.execute(
            "INSERT INTO capability_versions (capability_id, version, spec_json, source_summary, created_at)
             VALUES (?1, ?2, ?3, NULL, datetime('now'))",
            params![id, new_version, spec_str],
        )?;

        Ok(())
    }

    /// 加载指定能力资产。
    pub fn load_capability(&self, id: &str) -> Result<Option<StoredCapability>, StoreError> {
        let db = self.db();
        let mut stmt = db.prepare(
            "SELECT id, device_id, name, description, version, spec_json, created_at, updated_at
             FROM capability_assets WHERE id = ?1",
        )?;

        let mut rows = stmt.query_map([id], |row| {
            let spec_json_str: String = row.get(5)?;
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, Option<String>>(3)?,
                row.get::<_, i64>(4)?,
                spec_json_str,
                row.get::<_, String>(6)?,
                row.get::<_, String>(7)?,
            ))
        })?;

        let row = match rows.next() {
            Some(row) => row?,
            None => return Ok(None),
        };

        let (id, device_id, name, description, version, spec_json_str, created_at, updated_at) =
            row;
        Ok(Some(StoredCapability {
            id,
            device_id,
            name,
            description,
            version,
            spec_json: serde_json::from_str(&spec_json_str)?,
            created_at,
            updated_at,
        }))
    }

    /// 列出能力资产摘要。`device_id` 为 `Some` 时按设备过滤。
    pub fn list_capabilities(
        &self,
        device_id: Option<&str>,
    ) -> Result<Vec<CapabilitySummary>, StoreError> {
        let db = self.db();

        let (sql, params): (&str, Vec<Box<dyn rusqlite::types::ToSql>>) =
            if let Some(did) = device_id {
                (
                    "SELECT id, device_id, name, description, version, updated_at
                     FROM capability_assets WHERE device_id = ?1 ORDER BY updated_at DESC",
                    vec![Box::new(did.to_owned())],
                )
            } else {
                (
                    "SELECT id, device_id, name, description, version, updated_at
                     FROM capability_assets ORDER BY updated_at DESC",
                    vec![],
                )
            };

        let mut stmt = db.prepare(sql)?;
        let param_refs: Vec<&dyn rusqlite::types::ToSql> =
            params.iter().map(AsRef::as_ref).collect();
        let rows = stmt.query_map(param_refs.as_slice(), |row| {
            Ok(CapabilitySummary {
                id: row.get(0)?,
                device_id: row.get(1)?,
                name: row.get(2)?,
                description: row.get(3)?,
                version: row.get(4)?,
                updated_at: row.get(5)?,
            })
        })?;

        let mut result = Vec::new();
        for row in rows {
            result.push(row?);
        }
        Ok(result)
    }

    /// 删除能力资产及其所有版本和来源记录。
    pub fn delete_capability(&self, id: &str) -> Result<(), StoreError> {
        let db = self.db();
        db.execute(
            "DELETE FROM capability_sources WHERE capability_id = ?1",
            [id],
        )?;
        db.execute(
            "DELETE FROM capability_versions WHERE capability_id = ?1",
            [id],
        )?;
        db.execute("DELETE FROM capability_assets WHERE id = ?1", [id])?;
        Ok(())
    }

    /// 列出能力资产的所有版本摘要。
    pub fn list_capability_versions(
        &self,
        capability_id: &str,
    ) -> Result<Vec<CapabilityVersionSummary>, StoreError> {
        let db = self.db();
        let mut stmt = db.prepare(
            "SELECT version, created_at, source_summary FROM capability_versions
             WHERE capability_id = ?1 ORDER BY version DESC",
        )?;

        let rows = stmt.query_map([capability_id], |row| {
            Ok(CapabilityVersionSummary {
                version: row.get(0)?,
                created_at: row.get(1)?,
                source_summary: row.get(2)?,
            })
        })?;

        let mut result = Vec::new();
        for row in rows {
            result.push(row?);
        }
        Ok(result)
    }

    /// 加载特定版本的能力资产。
    pub fn load_capability_version(
        &self,
        capability_id: &str,
        version: i64,
    ) -> Result<Option<StoredCapabilityVersion>, StoreError> {
        let db = self.db();
        let mut stmt = db.prepare(
            "SELECT capability_id, version, spec_json, source_summary, created_at
             FROM capability_versions WHERE capability_id = ?1 AND version = ?2",
        )?;

        let mut rows = stmt.query_map(params![capability_id, version], |row| {
            let spec_json_str: String = row.get(2)?;
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, i64>(1)?,
                spec_json_str,
                row.get::<_, Option<String>>(3)?,
                row.get::<_, String>(4)?,
            ))
        })?;

        let row = match rows.next() {
            Some(row) => row?,
            None => return Ok(None),
        };

        let (capability_id, version, spec_json_str, source_summary, created_at) = row;
        Ok(Some(StoredCapabilityVersion {
            capability_id,
            version,
            spec_json: serde_json::from_str(&spec_json_str)?,
            source_summary,
            created_at,
        }))
    }

    /// 批量保存能力来源追溯记录。
    pub fn save_capability_sources(
        &self,
        capability_id: &str,
        sources: &[CapabilitySource],
    ) -> Result<(), StoreError> {
        let db = self.db();
        db.execute(
            "DELETE FROM capability_sources WHERE capability_id = ?1",
            [capability_id],
        )?;
        for source in sources {
            db.execute(
                "INSERT INTO capability_sources (capability_id, field_path, source_text, confidence, created_at)
                 VALUES (?1, ?2, ?3, ?4, datetime('now'))",
                params![capability_id, source.field_path, source.source_text, source.confidence],
            )?;
        }
        Ok(())
    }

    /// 加载能力资产的所有来源追溯记录。
    pub fn load_capability_sources(
        &self,
        capability_id: &str,
    ) -> Result<Vec<CapabilitySource>, StoreError> {
        let db = self.db();
        let mut stmt = db.prepare(
            "SELECT field_path, source_text, confidence FROM capability_sources WHERE capability_id = ?1",
        )?;

        let rows = stmt.query_map([capability_id], |row| {
            Ok(CapabilitySource {
                field_path: row.get(0)?,
                source_text: row.get(1)?,
                confidence: row.get(2)?,
            })
        })?;

        let mut result = Vec::new();
        for row in rows {
            result.push(row?);
        }
        Ok(result)
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use serde_json::json;

    fn test_store() -> Store {
        Store::open_in_memory().expect("内存数据库应可打开")
    }

    fn sample_spec() -> serde_json::Value {
        json!({
            "id": "axis.move_to",
            "device_id": "press_1",
            "implementation": { "type": "modbus-write", "register": 40010, "value": "${position}" },
            "safety": { "level": "low" }
        })
    }

    #[test]
    fn save_and_load_capability() {
        let store = test_store();
        store
            .save_capability(
                "cap1",
                "press_1",
                "移动轴",
                Some("控制轴移动"),
                &sample_spec(),
            )
            .unwrap();

        let cap = store.load_capability("cap1").unwrap().unwrap();
        assert_eq!(cap.id, "cap1");
        assert_eq!(cap.device_id, "press_1");
        assert_eq!(cap.name, "移动轴");
        assert_eq!(cap.description, Some("控制轴移动".to_owned()));
        assert_eq!(cap.version, 1);
    }

    #[test]
    fn save_覆盖_版本递增() {
        let store = test_store();
        store
            .save_capability("c1", "d1", "能力", None, &json!({"v": 1}))
            .unwrap();
        store
            .save_capability("c1", "d1", "能力更新", None, &json!({"v": 2}))
            .unwrap();

        let cap = store.load_capability("c1").unwrap().unwrap();
        assert_eq!(cap.version, 2);
        assert_eq!(cap.name, "能力更新");
    }

    #[test]
    fn list_capabilities_按设备过滤() {
        let store = test_store();
        store
            .save_capability("ca", "dev_a", "能力A", None, &json!({}))
            .unwrap();
        store
            .save_capability("cb", "dev_b", "能力B", None, &json!({}))
            .unwrap();
        store
            .save_capability("ca2", "dev_a", "能力A2", None, &json!({}))
            .unwrap();

        let all = store.list_capabilities(None).unwrap();
        assert_eq!(all.len(), 3);

        let dev_a = store.list_capabilities(Some("dev_a")).unwrap();
        assert_eq!(dev_a.len(), 2);
        assert!(dev_a.iter().all(|c| c.device_id == "dev_a"));
    }

    #[test]
    fn delete_capability_级联删除() {
        let store = test_store();
        store
            .save_capability("x", "d", "X", None, &json!({}))
            .unwrap();
        store.delete_capability("x").unwrap();

        assert!(store.load_capability("x").unwrap().is_none());
        assert!(store.list_capabilities(None).unwrap().is_empty());
    }

    #[test]
    fn load_不存在的能力返回_none() {
        let store = test_store();
        assert!(store.load_capability("nope").unwrap().is_none());
    }

    #[test]
    fn capability_versions_自动记录() {
        let store = test_store();
        store
            .save_capability("v", "d", "V", None, &json!({"step": 1}))
            .unwrap();
        store
            .save_capability("v", "d", "V", None, &json!({"step": 2}))
            .unwrap();

        let versions = store.list_capability_versions("v").unwrap();
        assert_eq!(versions.len(), 2);
        assert_eq!(versions[0].version, 2);
        assert_eq!(versions[1].version, 1);

        let v1 = store.load_capability_version("v", 1).unwrap().unwrap();
        assert_eq!(v1.spec_json["step"], 1);
    }

    #[test]
    fn capability_sources_批量保存和加载() {
        let store = test_store();
        store
            .save_capability("s", "d", "S", None, &json!({}))
            .unwrap();

        let sources = vec![CapabilitySource {
            field_path: "inputs[0].range".to_owned(),
            source_text: "量程 0-150 mm".to_owned(),
            confidence: 0.92,
        }];
        store.save_capability_sources("s", &sources).unwrap();

        let loaded = store.load_capability_sources("s").unwrap();
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0].field_path, "inputs[0].range");
        assert!((loaded[0].confidence - 0.92).abs() < f64::EPSILON);
    }

    #[test]
    fn capability_sources_覆盖旧记录() {
        let store = test_store();
        store
            .save_capability("so", "d", "SO", None, &json!({}))
            .unwrap();

        let v1 = vec![CapabilitySource {
            field_path: "a".to_owned(),
            source_text: "old".to_owned(),
            confidence: 0.5,
        }];
        store.save_capability_sources("so", &v1).unwrap();

        let v2 = vec![CapabilitySource {
            field_path: "b".to_owned(),
            source_text: "new".to_owned(),
            confidence: 0.9,
        }];
        store.save_capability_sources("so", &v2).unwrap();

        let loaded = store.load_capability_sources("so").unwrap();
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0].field_path, "b");
    }
}
