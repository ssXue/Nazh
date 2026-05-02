//! `SQLite` 持久化写入节点，将 payload 序列化后插入本地数据库。
//!
//! 表名通过 [`sanitize_sqlite_identifier`] 校验防止 SQL 注入，
//! 数据库操作在 [`tokio::task::spawn_blocking`] 中执行以避免阻塞异步运行时。

use async_trait::async_trait;
use chrono::Utc;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use uuid::Uuid;

use nazh_core::{EngineError, NodeExecution, NodeTrait, PinDefinition, PinType, into_payload_map};

fn default_sqlite_path() -> String {
    "./nazh-local.sqlite3".to_owned()
}

fn default_sqlite_table() -> String {
    "workflow_logs".to_owned()
}

/// 校验 `SQLite` 标识符：只允许字母、数字、下划线，且不能以数字开头。
fn sanitize_sqlite_identifier(identifier: &str) -> Option<String> {
    let trimmed = identifier.trim();
    let mut chars = trimmed.chars();
    let first = chars.next()?;

    if !(first == '_' || first.is_ascii_alphabetic()) {
        return None;
    }

    if chars.all(|ch| ch == '_' || ch.is_ascii_alphanumeric()) {
        Some(trimmed.to_owned())
    } else {
        None
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SqlWriterNodeConfig {
    #[serde(default = "default_sqlite_path")]
    pub database_path: String,
    #[serde(default = "default_sqlite_table")]
    pub table: String,
}

/// `SQLite` 持久化写入节点。
pub struct SqlWriterNode {
    id: String,
    config: SqlWriterNodeConfig,
}

impl SqlWriterNode {
    pub fn new(id: impl Into<String>, config: SqlWriterNodeConfig) -> Self {
        Self {
            id: id.into(),
            config,
        }
    }
}

#[async_trait]
impl NodeTrait for SqlWriterNode {
    nazh_core::impl_node_meta!("sqlWriter");

    /// 输入引脚：必需的 `Json` 端口。
    ///
    /// `sqlWriter` 是纯 sink——payload 必须是有列结构的 JSON 对象，
    /// 字段名映射到目标表的列。`required: true` 让部署期校验拒绝
    /// "无上游入边"的 sql 节点（默认 `id == "in"` 的根节点豁免该校验，
    /// 由 ingress 直接喂数据；详见 [`PinDefinition::default_input`]）。
    ///
    /// 输出端口保留 trait 默认（`Any`）——写入确认元信息基本无下游消费。
    fn input_pins(&self) -> Vec<PinDefinition> {
        vec![PinDefinition::required_input(
            PinType::Json,
            "要写入数据库的行数据；JSON 对象的字段映射到目标表的列",
        )]
    }

    async fn transform(
        &self,
        trace_id: Uuid,
        payload: Value,
    ) -> Result<NodeExecution, EngineError> {
        let database_path = self.config.database_path.trim().to_owned();
        if database_path.contains("..") {
            return Err(EngineError::node_config(
                self.id.clone(),
                "database_path 不允许包含路径穿越（..）",
            ));
        }
        let table = sanitize_sqlite_identifier(&self.config.table).ok_or_else(|| {
            EngineError::node_config(
                self.id.clone(),
                "SQL Writer 表名只能包含字母、数字和下划线，且不能以数字开头",
            )
        })?;
        let node_id = self.id.clone();
        let payload_json = serde_json::to_string(&payload)
            .map_err(|error| EngineError::payload_conversion(self.id.clone(), error.to_string()))?;
        let timestamp = Utc::now().to_rfc3339();
        let db_path_clone = database_path.clone();
        let table_clone = table.clone();
        let timestamp_clone = timestamp.clone();

        tokio::task::spawn_blocking(move || {
            if let Some(parent) = std::path::Path::new(&db_path_clone).parent()
                && !parent.as_os_str().is_empty()
            {
                std::fs::create_dir_all(parent).map_err(|error| {
                    EngineError::stage_execution(
                        node_id.clone(),
                        trace_id,
                        format!("创建 SQLite 目录失败: {error}"),
                    )
                })?;
            }

            let conn = rusqlite::Connection::open(&db_path_clone).map_err(|error| {
                EngineError::stage_execution(
                    node_id.clone(),
                    trace_id,
                    format!("打开 SQLite 数据库失败: {error}"),
                )
            })?;

            let create_sql = format!(
                "CREATE TABLE IF NOT EXISTS {table_clone} (\
                 id INTEGER PRIMARY KEY AUTOINCREMENT, \
                 trace_id TEXT NOT NULL, \
                 node_id TEXT NOT NULL, \
                 created_at TEXT NOT NULL, \
                 payload_json TEXT NOT NULL)"
            );
            conn.execute_batch(&create_sql).map_err(|error| {
                EngineError::stage_execution(
                    node_id.clone(),
                    trace_id,
                    format!("创建 SQLite 表失败: {error}"),
                )
            })?;

            let insert_sql = format!(
                "INSERT INTO {table_clone} (trace_id, node_id, created_at, payload_json) \
                 VALUES (?1, ?2, ?3, ?4)"
            );
            conn.execute(
                &insert_sql,
                rusqlite::params![trace_id.to_string(), node_id, timestamp_clone, payload_json,],
            )
            .map_err(|error| {
                EngineError::stage_execution(
                    node_id.clone(),
                    trace_id,
                    format!("SQLite 插入失败: {error}"),
                )
            })?;

            Ok(())
        })
        .await
        .map_err(|_| EngineError::StagePanicked {
            stage: self.id.clone(),
            trace_id,
        })??;

        let payload_map = into_payload_map(payload);
        let metadata = serde_json::Map::from_iter([(
            "sql_writer".to_owned(),
            json!({
                "database_path": database_path,
                "table": table,
                "written_at": timestamp,
            }),
        )]);

        Ok(NodeExecution::broadcast(Value::Object(payload_map)).with_metadata(Some(metadata)))
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    fn make_node() -> SqlWriterNode {
        let config: SqlWriterNodeConfig = serde_json::from_value(json!({})).unwrap();
        SqlWriterNode::new("sql-1", config)
    }

    #[test]
    fn input_pin_是_json_必需() {
        let node = make_node();
        let pins = node.input_pins();
        assert_eq!(pins.len(), 1, "sqlWriter 只声明单个输入端口");
        assert_eq!(pins[0].id, "in");
        assert_eq!(pins[0].pin_type, PinType::Json);
        assert!(pins[0].required, "sqlWriter 输入必需——sink 节点不能空跑");
    }

    #[test]
    fn output_pin_保留默认_any() {
        // sqlWriter 是纯 sink，下游基本不消费输出，输出端口保留 Any
        // 减少耦合。
        let node = make_node();
        let pins = node.output_pins();
        assert_eq!(pins.len(), 1);
        assert_eq!(pins[0].pin_type, PinType::Any);
    }
}
