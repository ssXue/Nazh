//! `workflow_deploy` 辅助函数：ID 生成、SQL Writer 路径规范化。

use std::path::{Component, Path, PathBuf};

use nazh_engine::{EngineError, WorkflowGraph};
use serde_json::Value;
use tauri_bindings::ObservabilityContextInput;

const SQL_WRITER_DEFAULT_DATABASE_PATH: &str = "./nazh-local.sqlite3";

pub(super) fn derive_workflow_id(
    requested_workflow_id: Option<&str>,
    graph_name: Option<&str>,
    observability_context: Option<&ObservabilityContextInput>,
) -> String {
    if let Some(requested) = requested_workflow_id
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        return requested.to_owned();
    }

    if let Some(project_id) = observability_context
        .map(|context| context.project_id.trim())
        .filter(|value| !value.is_empty())
    {
        return project_id.to_owned();
    }

    let candidate = graph_name.map(str::trim).filter(|value| !value.is_empty());

    let sanitized = candidate
        .map(|value| {
            value
                .chars()
                .map(|ch| {
                    if ch.is_ascii_alphanumeric() {
                        ch.to_ascii_lowercase()
                    } else if matches!(ch, '-' | '_') {
                        ch
                    } else {
                        '-'
                    }
                })
                .collect::<String>()
                .trim_matches('-')
                .to_owned()
        })
        .filter(|value| !value.is_empty());

    sanitized.unwrap_or_else(|| format!("workflow-{}", chrono::Utc::now().timestamp_millis()))
}

pub(super) fn normalize_sql_writer_paths(
    graph: &mut WorkflowGraph,
    workspace_dir: &Path,
) -> Result<(), EngineError> {
    for node_definition in graph.nodes.values_mut() {
        if node_definition.node_type() != "sqlWriter" && node_definition.node_type() != "sql/writer"
        {
            continue;
        }

        let node_id = node_definition.id().to_owned();
        let Some(config_map) = node_definition.config_mut().as_object_mut() else {
            continue;
        };

        let raw_database_path = config_map
            .get("database_path")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .unwrap_or(SQL_WRITER_DEFAULT_DATABASE_PATH)
            .to_owned();

        let resolved_path =
            normalize_sql_writer_database_path(&raw_database_path, workspace_dir, &node_id)?;
        config_map.insert(
            "database_path".to_owned(),
            Value::String(resolved_path.to_string_lossy().to_string()),
        );
    }

    Ok(())
}

fn normalize_sql_writer_database_path(
    raw_path: &str,
    workspace_dir: &Path,
    node_id: &str,
) -> Result<PathBuf, EngineError> {
    let path = Path::new(raw_path);
    if path.is_absolute() {
        if path
            .components()
            .any(|component| component == Component::ParentDir)
        {
            return Err(EngineError::node_config(
                node_id.to_owned(),
                "database_path 不允许包含路径穿越（..）",
            ));
        }

        if !path.starts_with(workspace_dir) {
            return Err(EngineError::node_config(
                node_id.to_owned(),
                "database_path 需要位于当前工作目录内",
            ));
        }

        return Ok(path.to_path_buf());
    }

    Ok(workspace_dir.join(sanitize_relative_path(raw_path)))
}

fn sanitize_relative_path(raw_path: &str) -> PathBuf {
    let mut sanitized = PathBuf::new();

    for component in Path::new(raw_path).components() {
        match component {
            Component::Normal(segment) => sanitized.push(segment),
            Component::CurDir
            | Component::ParentDir
            | Component::RootDir
            | Component::Prefix(_) => {}
        }
    }

    if sanitized.as_os_str().is_empty() {
        sanitized.push("nazh-local.sqlite3");
    }

    sanitized
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::{normalize_sql_writer_database_path, sanitize_relative_path};
    use nazh_engine::EngineError;
    use std::path::PathBuf;

    #[test]
    fn sanitize_relative_path_removes_escape_segments() {
        let sanitized = sanitize_relative_path("../data/./edge-runtime.sqlite3");
        assert_eq!(sanitized, PathBuf::from("data/edge-runtime.sqlite3"));
    }

    #[test]
    fn sanitize_relative_path_falls_back_when_empty() {
        let sanitized = sanitize_relative_path("./");
        assert_eq!(sanitized, PathBuf::from("nazh-local.sqlite3"));
    }

    #[test]
    fn sql_writer_relative_path_resolves_inside_workspace() {
        let workspace = PathBuf::from("/tmp/nazh-workspace");
        let normalized =
            normalize_sql_writer_database_path("./data/edge-runtime.sqlite3", &workspace, "sql_1")
                .unwrap();

        assert_eq!(normalized, workspace.join("data/edge-runtime.sqlite3"));
    }

    #[test]
    fn sql_writer_escape_segments_stay_inside_workspace() {
        let workspace = PathBuf::from("/tmp/nazh-workspace");
        let normalized =
            normalize_sql_writer_database_path("../audit.sqlite3", &workspace, "sql_1").unwrap();

        assert_eq!(normalized, workspace.join("audit.sqlite3"));
    }

    #[test]
    fn sql_writer_absolute_path_inside_workspace_is_allowed() {
        let workspace = PathBuf::from("/tmp/nazh-workspace");
        let normalized = normalize_sql_writer_database_path(
            "/tmp/nazh-workspace/data/audit.sqlite3",
            &workspace,
            "sql_1",
        )
        .unwrap();

        assert_eq!(normalized, workspace.join("data/audit.sqlite3"));
    }

    #[test]
    fn sql_writer_absolute_path_outside_workspace_is_rejected() {
        let workspace = PathBuf::from("/tmp/nazh-workspace");
        let error = normalize_sql_writer_database_path("/tmp/audit.sqlite3", &workspace, "sql_1")
            .unwrap_err();

        assert!(matches!(
            error,
            EngineError::NodeConfig { node_id, message }
                if node_id == "sql_1" && message.contains("工作目录")
        ));
    }
}
