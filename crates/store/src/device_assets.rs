//! 设备资产持久化（RFC-0004 Phase 1）。

use crate::{Store, StoreError};
use rusqlite::params;

/// 设备资产摘要（列表视图）。
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct DeviceAssetSummary {
    pub id: String,
    pub name: String,
    pub device_type: String,
    pub version: i64,
    pub updated_at: String,
}

/// 设备资产完整记录。
#[derive(Debug, Clone)]
pub struct StoredDeviceAsset {
    pub id: String,
    pub name: String,
    pub device_type: String,
    pub version: i64,
    pub spec_json: serde_json::Value,
    pub created_at: String,
    pub updated_at: String,
}

/// 设备资产版本记录。
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct StoredAssetVersion {
    pub asset_id: String,
    pub version: i64,
    pub spec_json: serde_json::Value,
    pub source_summary: Option<String>,
    pub created_at: String,
}

/// 设备资产版本摘要。
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct AssetVersionSummary {
    pub version: i64,
    pub created_at: String,
    pub source_summary: Option<String>,
}

/// AI 抽取来源追溯记录。
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct FieldSource {
    pub field_path: String,
    pub source_text: String,
    pub confidence: f64,
}

impl Store {
    /// 保存（或更新）设备资产，自动递增版本号。
    pub fn save_device_asset(
        &self,
        id: &str,
        name: &str,
        device_type: &str,
        spec_json: &serde_json::Value,
    ) -> Result<(), StoreError> {
        let spec_str = serde_json::to_string(spec_json)?;
        let db = self.db();

        // 检查是否已存在，获取当前版本
        let current_version: Option<i64> = db
            .query_row(
                "SELECT version FROM device_assets WHERE id = ?1",
                [id],
                |row| row.get(0),
            )
            .ok();

        let new_version = current_version.map_or(1, |v| v + 1);

        db.execute(
            "INSERT INTO device_assets (id, name, device_type, version, spec_json, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, datetime('now'), datetime('now'))
             ON CONFLICT(id) DO UPDATE SET
                name = excluded.name,
                device_type = excluded.device_type,
                version = excluded.version,
                spec_json = excluded.spec_json,
                updated_at = datetime('now')",
            params![id, name, device_type, new_version, spec_str],
        )?;

        // 同时写入版本历史
        db.execute(
            "INSERT INTO device_asset_versions (asset_id, version, spec_json, source_summary, created_at)
             VALUES (?1, ?2, ?3, NULL, datetime('now'))",
            params![id, new_version, spec_str],
        )?;

        Ok(())
    }

    /// 加载指定设备资产。
    pub fn load_device_asset(&self, id: &str) -> Result<Option<StoredDeviceAsset>, StoreError> {
        let db = self.db();
        let mut stmt = db.prepare(
            "SELECT id, name, device_type, version, spec_json, created_at, updated_at
             FROM device_assets WHERE id = ?1",
        )?;

        let mut rows = stmt.query_map([id], |row| {
            let spec_json_str: String = row.get(4)?;
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, i64>(3)?,
                spec_json_str,
                row.get::<_, String>(5)?,
                row.get::<_, String>(6)?,
            ))
        })?;

        let row = match rows.next() {
            Some(row) => row?,
            None => return Ok(None),
        };

        let (id, name, device_type, version, spec_json_str, created_at, updated_at) = row;
        Ok(Some(StoredDeviceAsset {
            id,
            name,
            device_type,
            version,
            spec_json: serde_json::from_str(&spec_json_str)?,
            created_at,
            updated_at,
        }))
    }

    /// 列出所有设备资产摘要。
    pub fn list_device_assets(&self) -> Result<Vec<DeviceAssetSummary>, StoreError> {
        let db = self.db();
        let mut stmt = db.prepare(
            "SELECT id, name, device_type, version, updated_at FROM device_assets ORDER BY updated_at DESC",
        )?;

        let rows = stmt.query_map([], |row| {
            Ok(DeviceAssetSummary {
                id: row.get(0)?,
                name: row.get(1)?,
                device_type: row.get(2)?,
                version: row.get(3)?,
                updated_at: row.get(4)?,
            })
        })?;

        let mut result = Vec::new();
        for row in rows {
            result.push(row?);
        }
        Ok(result)
    }

    /// 删除设备资产及其所有版本和来源记录。
    pub fn delete_device_asset(&self, id: &str) -> Result<(), StoreError> {
        let db = self.db();
        db.execute("DELETE FROM device_asset_sources WHERE asset_id = ?1", [id])?;
        db.execute(
            "DELETE FROM device_asset_versions WHERE asset_id = ?1",
            [id],
        )?;
        db.execute("DELETE FROM device_assets WHERE id = ?1", [id])?;
        Ok(())
    }

    /// 列出设备资产的所有版本摘要。
    pub fn list_asset_versions(
        &self,
        asset_id: &str,
    ) -> Result<Vec<AssetVersionSummary>, StoreError> {
        let db = self.db();
        let mut stmt = db.prepare(
            "SELECT version, created_at, source_summary FROM device_asset_versions
             WHERE asset_id = ?1 ORDER BY version DESC",
        )?;

        let rows = stmt.query_map([asset_id], |row| {
            Ok(AssetVersionSummary {
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

    /// 加载特定版本的设备资产。
    pub fn load_asset_version(
        &self,
        asset_id: &str,
        version: i64,
    ) -> Result<Option<StoredAssetVersion>, StoreError> {
        let db = self.db();
        let mut stmt = db.prepare(
            "SELECT asset_id, version, spec_json, source_summary, created_at
             FROM device_asset_versions WHERE asset_id = ?1 AND version = ?2",
        )?;

        let mut rows = stmt.query_map(params![asset_id, version], |row| {
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

        let (asset_id, version, spec_json_str, source_summary, created_at) = row;
        Ok(Some(StoredAssetVersion {
            asset_id,
            version,
            spec_json: serde_json::from_str(&spec_json_str)?,
            source_summary,
            created_at,
        }))
    }

    /// 批量保存 AI 抽取来源追溯记录。
    pub fn save_asset_sources(
        &self,
        asset_id: &str,
        sources: &[FieldSource],
    ) -> Result<(), StoreError> {
        let db = self.db();
        // 先清除旧记录
        db.execute(
            "DELETE FROM device_asset_sources WHERE asset_id = ?1",
            [asset_id],
        )?;
        for source in sources {
            db.execute(
                "INSERT INTO device_asset_sources (asset_id, field_path, source_text, confidence, created_at)
                 VALUES (?1, ?2, ?3, ?4, datetime('now'))",
                params![asset_id, source.field_path, source.source_text, source.confidence],
            )?;
        }
        Ok(())
    }

    /// 加载设备资产的所有来源追溯记录。
    pub fn load_asset_sources(&self, asset_id: &str) -> Result<Vec<FieldSource>, StoreError> {
        let db = self.db();
        let mut stmt = db.prepare(
            "SELECT field_path, source_text, confidence FROM device_asset_sources WHERE asset_id = ?1",
        )?;

        let rows = stmt.query_map([asset_id], |row| {
            Ok(FieldSource {
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
            "id": "press_1",
            "type": "hydraulic_press",
            "connection": { "type": "modbus-tcp", "id": "conn1" },
            "signals": [
                { "id": "pressure", "signal_type": "analog_input", "source": { "type": "register", "register": 40001, "data_type": "float32" } }
            ]
        })
    }

    #[test]
    fn save_and_load_device_asset() {
        let store = test_store();
        store
            .save_device_asset("press_1", "液压机 1", "hydraulic_press", &sample_spec())
            .unwrap();

        let asset = store.load_device_asset("press_1").unwrap().unwrap();
        assert_eq!(asset.id, "press_1");
        assert_eq!(asset.name, "液压机 1");
        assert_eq!(asset.device_type, "hydraulic_press");
        assert_eq!(asset.version, 1);
        assert_eq!(asset.spec_json["id"], "press_1");
    }

    #[test]
    fn save_覆盖_版本递增() {
        let store = test_store();
        store
            .save_device_asset("dev1", "设备", "sensor", &json!({"v": 1}))
            .unwrap();
        store
            .save_device_asset("dev1", "设备更新", "sensor", &json!({"v": 2}))
            .unwrap();

        let asset = store.load_device_asset("dev1").unwrap().unwrap();
        assert_eq!(asset.version, 2);
        assert_eq!(asset.name, "设备更新");
        assert_eq!(asset.spec_json["v"], 2);
    }

    #[test]
    fn list_device_assets_返回摘要列表() {
        let store = test_store();
        store
            .save_device_asset("a", "设备A", "type_a", &json!({}))
            .unwrap();
        store
            .save_device_asset("b", "设备B", "type_b", &json!({}))
            .unwrap();

        let list = store.list_device_assets().unwrap();
        assert_eq!(list.len(), 2);
        assert!(list.iter().any(|s| s.id == "a"));
        assert!(list.iter().any(|s| s.id == "b"));
    }

    #[test]
    fn delete_device_asset_级联删除() {
        let store = test_store();
        store
            .save_device_asset("x", "设备X", "type", &json!({}))
            .unwrap();
        store.delete_device_asset("x").unwrap();

        assert!(store.load_device_asset("x").unwrap().is_none());
        assert!(store.list_device_assets().unwrap().is_empty());
    }

    #[test]
    fn load_不存在的资产返回_none() {
        let store = test_store();
        assert!(store.load_device_asset("nope").unwrap().is_none());
    }

    #[test]
    fn asset_versions_自动记录() {
        let store = test_store();
        store
            .save_device_asset("v", "V", "type", &json!({"step": 1}))
            .unwrap();
        store
            .save_device_asset("v", "V", "type", &json!({"step": 2}))
            .unwrap();

        let versions = store.list_asset_versions("v").unwrap();
        assert_eq!(versions.len(), 2);
        // 按版本降序
        assert_eq!(versions[0].version, 2);
        assert_eq!(versions[1].version, 1);

        // 加载特定版本
        let v1 = store.load_asset_version("v", 1).unwrap().unwrap();
        assert_eq!(v1.spec_json["step"], 1);
    }

    #[test]
    fn asset_sources_批量保存和加载() {
        let store = test_store();
        store
            .save_device_asset("s", "S", "type", &json!({}))
            .unwrap();

        let sources = vec![
            FieldSource {
                field_path: "signals[0].range".to_owned(),
                source_text: "量程 0-35 MPa".to_owned(),
                confidence: 0.95,
            },
            FieldSource {
                field_path: "signals[1].id".to_owned(),
                source_text: "位置传感器".to_owned(),
                confidence: 0.8,
            },
        ];
        store.save_asset_sources("s", &sources).unwrap();

        let loaded = store.load_asset_sources("s").unwrap();
        assert_eq!(loaded.len(), 2);
        assert_eq!(loaded[0].field_path, "signals[0].range");
        assert!((loaded[0].confidence - 0.95).abs() < f64::EPSILON);
    }

    #[test]
    fn asset_sources_覆盖旧记录() {
        let store = test_store();
        store
            .save_device_asset("so", "SO", "type", &json!({}))
            .unwrap();

        let v1 = vec![FieldSource {
            field_path: "a".to_owned(),
            source_text: "old".to_owned(),
            confidence: 0.5,
        }];
        store.save_asset_sources("so", &v1).unwrap();
        assert_eq!(store.load_asset_sources("so").unwrap().len(), 1);

        let v2 = vec![FieldSource {
            field_path: "b".to_owned(),
            source_text: "new".to_owned(),
            confidence: 0.9,
        }];
        store.save_asset_sources("so", &v2).unwrap();
        let loaded = store.load_asset_sources("so").unwrap();
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0].field_path, "b");
    }
}
