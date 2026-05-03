//! Schema 版本管理与 migration 执行器（RFC-0003 Phase 1，ADR-0022）。

use rusqlite::Connection;

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
        "002",
        "
        CREATE TABLE IF NOT EXISTS device_assets (
            id          TEXT PRIMARY KEY,
            name        TEXT NOT NULL,
            device_type TEXT NOT NULL,
            version     INTEGER NOT NULL DEFAULT 1,
            spec_json   TEXT NOT NULL,
            created_at  TEXT NOT NULL,
            updated_at  TEXT NOT NULL
        );

        CREATE TABLE IF NOT EXISTS device_asset_versions (
            asset_id        TEXT NOT NULL,
            version         INTEGER NOT NULL,
            spec_json       TEXT NOT NULL,
            source_summary  TEXT,
            created_at      TEXT NOT NULL,
            PRIMARY KEY (asset_id, version)
        );

        CREATE TABLE IF NOT EXISTS device_asset_sources (
            asset_id    TEXT NOT NULL,
            field_path  TEXT NOT NULL,
            source_text TEXT NOT NULL,
            confidence  REAL NOT NULL,
            created_at  TEXT NOT NULL
        );
        CREATE INDEX IF NOT EXISTS idx_device_asset_sources_asset
            ON device_asset_sources(asset_id);
        ",
    ),
    (
        "003",
        "
        CREATE TABLE IF NOT EXISTS capability_assets (
            id          TEXT PRIMARY KEY,
            device_id   TEXT NOT NULL,
            name        TEXT NOT NULL,
            description TEXT,
            version     INTEGER NOT NULL DEFAULT 1,
            spec_json   TEXT NOT NULL,
            created_at  TEXT NOT NULL,
            updated_at  TEXT NOT NULL
        );
        CREATE INDEX IF NOT EXISTS idx_capability_assets_device
            ON capability_assets(device_id);

        CREATE TABLE IF NOT EXISTS capability_versions (
            capability_id   TEXT NOT NULL,
            version         INTEGER NOT NULL,
            spec_json       TEXT NOT NULL,
            source_summary  TEXT,
            created_at      TEXT NOT NULL,
            PRIMARY KEY (capability_id, version)
        );

        CREATE TABLE IF NOT EXISTS capability_sources (
            capability_id   TEXT NOT NULL,
            field_path      TEXT NOT NULL,
            source_text     TEXT NOT NULL,
            confidence      REAL NOT NULL,
            created_at      TEXT NOT NULL
        );
        CREATE INDEX IF NOT EXISTS idx_capability_sources_capability
            ON capability_sources(capability_id);
        ",
    ),
];

/// 检查 `schema_version` 表，执行尚未应用的 migrations。
pub(crate) fn run(db: &Connection) -> Result<(), rusqlite::Error> {
    for (version, sql) in MIGRATIONS {
        let applied: bool = db
            .query_row(
                "SELECT COUNT(*) > 0 FROM schema_version WHERE version = ?1",
                [version],
                |row| row.get(0),
            )
            .unwrap_or(false);
        if applied {
            continue;
        }
        db.execute_batch(sql)?;
        db.execute(
            "INSERT INTO schema_version (version, applied_at) VALUES (?1, datetime('now'))",
            [version],
        )?;
    }
    Ok(())
}
