//! Schema 版本管理与 migration 执行器（RFC-0003 Phase 1，ADR-0022）。

use rusqlite::{Connection, Error};

/// 内联 SQL migrations，按版本号顺序执行。
const MIGRATIONS: &[(&str, &str)] = &[
    (
        "001",
        "
        CREATE TABLE IF NOT EXISTS schema_version (
            version    INTEGER PRIMARY KEY,
            applied_at TEXT NOT NULL
        );

        CREATE TABLE IF NOT EXISTS variables (
            workflow_id TEXT NOT NULL,
            key         TEXT NOT NULL,
            value       TEXT NOT NULL,
            var_type    TEXT NOT NULL,
            initial     TEXT NOT NULL,
            updated_at  TEXT NOT NULL,
            updated_by  TEXT,
            PRIMARY KEY (workflow_id, key)
        );

        CREATE TABLE IF NOT EXISTS variable_history (
            id          INTEGER PRIMARY KEY AUTOINCREMENT,
            workflow_id TEXT NOT NULL,
            key         TEXT NOT NULL,
            value       TEXT NOT NULL,
            updated_at  TEXT NOT NULL,
            updated_by  TEXT
        );
        CREATE INDEX IF NOT EXISTS idx_var_history_time
            ON variable_history(workflow_id, key, updated_at);

        CREATE TABLE IF NOT EXISTS global_variables (
            namespace  TEXT NOT NULL,
            key        TEXT NOT NULL,
            value      TEXT NOT NULL,
            var_type   TEXT NOT NULL,
            updated_at TEXT NOT NULL,
            updated_by TEXT,
            PRIMARY KEY (namespace, key)
        );
        ",
    ),
    (
        "004",
        "
        CREATE TABLE IF NOT EXISTS copilot_conversations (
            id         TEXT PRIMARY KEY,
            title      TEXT NOT NULL,
            created_at TEXT NOT NULL,
            updated_at TEXT NOT NULL
        );

        CREATE TABLE IF NOT EXISTS copilot_messages (
            id              TEXT PRIMARY KEY,
            conversation_id TEXT NOT NULL REFERENCES copilot_conversations(id) ON DELETE CASCADE,
            role            TEXT NOT NULL CHECK(role IN ('user','assistant','system')),
            content         TEXT NOT NULL,
            created_at      TEXT NOT NULL
        );
        CREATE INDEX IF NOT EXISTS idx_copilot_messages_conv
            ON copilot_messages(conversation_id, created_at);
        ",
    ),
    (
        "006",
        "
        ALTER TABLE copilot_messages ADD COLUMN thinking TEXT;
        ",
    ),
    (
        "007",
        "
        CREATE TABLE IF NOT EXISTS observability_records (
            id          TEXT PRIMARY KEY,
            record_kind TEXT NOT NULL,
            category    TEXT NOT NULL,
            timestamp   TEXT NOT NULL,
            trace_id    TEXT,
            search_text TEXT NOT NULL,
            payload     TEXT NOT NULL
        );
        CREATE INDEX IF NOT EXISTS idx_observability_records_time
            ON observability_records(timestamp);
        CREATE INDEX IF NOT EXISTS idx_observability_records_trace
            ON observability_records(trace_id, timestamp);
        CREATE INDEX IF NOT EXISTS idx_observability_records_kind
            ON observability_records(record_kind, category, timestamp);

        CREATE TABLE IF NOT EXISTS deployment_audit (
            id               TEXT PRIMARY KEY,
            workflow_id      TEXT NOT NULL,
            action           TEXT NOT NULL,
            level            TEXT NOT NULL,
            timestamp        TEXT NOT NULL,
            project_id       TEXT,
            project_name     TEXT,
            environment_id   TEXT,
            environment_name TEXT,
            message          TEXT NOT NULL,
            detail           TEXT,
            data             TEXT
        );
        CREATE INDEX IF NOT EXISTS idx_deployment_audit_workflow_time
            ON deployment_audit(workflow_id, timestamp);
        CREATE INDEX IF NOT EXISTS idx_deployment_audit_time
            ON deployment_audit(timestamp);
        ",
    ),
    (
        "008",
        "
        CREATE TABLE IF NOT EXISTS ai_config (
            key   TEXT PRIMARY KEY,
            value TEXT NOT NULL
        );
        ",
    ),
];

/// 检查 `schema_version` 表，执行尚未应用的 migrations。
pub(crate) fn run(db: &Connection) -> Result<(), rusqlite::Error> {
    let tx = db.unchecked_transaction()?;
    for (version, sql) in MIGRATIONS {
        let applied = match tx.query_row(
            "SELECT COUNT(*) > 0 FROM schema_version WHERE version = ?1",
            [version],
            |row| row.get(0),
        ) {
            Ok(applied) => applied,
            Err(error) if is_missing_schema_version_table(&error) => false,
            Err(error) => return Err(error),
        };
        if applied {
            continue;
        }
        tx.execute_batch(sql)?;
        tx.execute(
            "INSERT INTO schema_version (version, applied_at) VALUES (?1, datetime('now'))",
            [version],
        )?;
    }
    tx.commit()
}

fn is_missing_schema_version_table(error: &Error) -> bool {
    matches!(
        error,
        Error::SqliteFailure(_, Some(message)) if message.contains("no such table: schema_version")
    )
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn schema_version_缺失时执行_bootstrap() {
        let db = Connection::open_in_memory().unwrap();

        run(&db).unwrap();

        let variables_count: i64 = db
            .query_row("SELECT COUNT(*) FROM variables", [], |row| row.get(0))
            .unwrap();
        assert_eq!(variables_count, 0);
    }

    #[test]
    fn schema_version_结构损坏时返回原始错误且不留下半套_schema() {
        let db = Connection::open_in_memory().unwrap();
        db.execute_batch("CREATE TABLE schema_version (applied_at TEXT NOT NULL);")
            .unwrap();

        let error = run(&db).unwrap_err();

        assert!(error.to_string().contains("no such column: version"));
        assert!(
            db.query_row("SELECT COUNT(*) FROM variables", [], |row| {
                row.get::<_, i64>(0)
            })
            .is_err()
        );
    }
}
