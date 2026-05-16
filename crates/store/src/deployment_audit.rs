//! 部署审计持久化（RFC-0003 Phase 3 子集）。

use crate::{Store, StoreError};
use rusqlite::params;

/// 部署审计记录。
#[derive(Debug, Clone)]
pub struct DeploymentAuditRecord {
    pub id: String,
    pub workflow_id: String,
    pub action: String,
    pub level: String,
    pub timestamp: String,
    pub project_id: Option<String>,
    pub project_name: Option<String>,
    pub environment_id: Option<String>,
    pub environment_name: Option<String>,
    pub message: String,
    pub detail: Option<String>,
    pub data: Option<serde_json::Value>,
}

impl Store {
    /// 追加一条部署审计记录。
    pub fn insert_deployment_audit(
        &self,
        record: &DeploymentAuditRecord,
    ) -> Result<(), StoreError> {
        let data_json = record
            .data
            .as_ref()
            .map(serde_json::to_string)
            .transpose()?;
        self.db().execute(
            "INSERT INTO deployment_audit
                (id, workflow_id, action, level, timestamp, project_id, project_name,
                 environment_id, environment_name, message, detail, data)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
            params![
                record.id,
                record.workflow_id,
                record.action,
                record.level,
                record.timestamp,
                record.project_id,
                record.project_name,
                record.environment_id,
                record.environment_name,
                record.message,
                record.detail,
                data_json
            ],
        )?;
        Ok(())
    }

    /// 查询指定工作流的部署审计记录。
    pub fn list_deployment_audit(
        &self,
        workflow_id: &str,
        limit: usize,
    ) -> Result<Vec<DeploymentAuditRecord>, StoreError> {
        let db = self.db();
        let mut stmt = db.prepare(
            "SELECT id, workflow_id, action, level, timestamp, project_id, project_name,
                    environment_id, environment_name, message, detail, data
             FROM deployment_audit
             WHERE workflow_id = ?1
             ORDER BY timestamp DESC, id DESC
             LIMIT ?2",
        )?;
        let limit = i64::try_from(limit).unwrap_or(i64::MAX);
        let rows = stmt.query_map(params![workflow_id, limit], |row| {
            let data_json: Option<String> = row.get(11)?;
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, String>(4)?,
                row.get::<_, Option<String>>(5)?,
                row.get::<_, Option<String>>(6)?,
                row.get::<_, Option<String>>(7)?,
                row.get::<_, Option<String>>(8)?,
                row.get::<_, String>(9)?,
                row.get::<_, Option<String>>(10)?,
                data_json,
            ))
        })?;

        let mut records = Vec::new();
        for row in rows {
            let (
                id,
                workflow_id,
                action,
                level,
                timestamp,
                project_id,
                project_name,
                environment_id,
                environment_name,
                message,
                detail,
                data_json,
            ) = row?;
            records.push(DeploymentAuditRecord {
                id,
                workflow_id,
                action,
                level,
                timestamp,
                project_id,
                project_name,
                environment_id,
                environment_name,
                message,
                detail,
                data: data_json.as_deref().map(serde_json::from_str).transpose()?,
            });
        }
        Ok(records)
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
    fn insert_and_list_deployment_audit() {
        let store = test_store();
        store
            .insert_deployment_audit(&DeploymentAuditRecord {
                id: "audit-1".to_owned(),
                workflow_id: "wf-1".to_owned(),
                action: "deploy_success".to_owned(),
                level: "success".to_owned(),
                timestamp: "2026-05-16T00:00:00Z".to_owned(),
                project_id: Some("project-1".to_owned()),
                project_name: Some("项目".to_owned()),
                environment_id: None,
                environment_name: None,
                message: "部署完成".to_owned(),
                detail: None,
                data: Some(serde_json::json!({"node_count": 2})),
            })
            .unwrap();

        let records = store.list_deployment_audit("wf-1", 10).unwrap();
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].action, "deploy_success");
        assert_eq!(records[0].data, Some(serde_json::json!({"node_count": 2})));
    }
}
