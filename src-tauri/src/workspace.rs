use std::path::PathBuf;

use tauri::{AppHandle, Manager};

/// 检查工作路径是否指向已知的系统敏感目录。
fn is_safe_workspace_path(path: &std::path::Path) -> Result<(), String> {
    let path_str = path.to_string_lossy();
    let forbidden_prefixes = [
        "/etc",
        "/var",
        "/sys",
        "/proc",
        "/dev",
        "/System",
        "/Library",
        "/usr",
        "/bin",
        "/sbin",
        "/private/etc",
        "/private/var",
    ];
    for prefix in &forbidden_prefixes {
        if path_str.starts_with(prefix) {
            return Err(format!("工作路径不允许指向系统目录: {prefix}"));
        }
    }
    Ok(())
}

pub(crate) fn resolve_project_workspace_dir(
    app: &AppHandle,
    workspace_path: Option<&str>,
) -> Result<(PathBuf, bool), String> {
    let trimmed = workspace_path.unwrap_or_default().trim();
    if trimmed.is_empty() {
        let default_dir = app
            .path()
            .app_local_data_dir()
            .map_err(|error| format!("无法解析默认工程目录: {error}"))?
            .join("workspace");
        return Ok((default_dir, true));
    }

    let expanded = expand_user_path(app, trimmed)?;
    if !expanded.is_absolute() {
        return Err("工作路径需要填写绝对路径。".to_owned());
    }

    is_safe_workspace_path(&expanded)?;

    Ok((expanded, false))
}

fn expand_user_path(app: &AppHandle, raw_path: &str) -> Result<PathBuf, String> {
    if raw_path == "~" || raw_path.starts_with("~/") {
        let home_dir = app
            .path()
            .home_dir()
            .map_err(|error| format!("无法解析用户目录: {error}"))?;
        let suffix = raw_path.trim_start_matches('~').trim_start_matches('/');
        return Ok(if suffix.is_empty() {
            home_dir
        } else {
            home_dir.join(suffix)
        });
    }

    Ok(PathBuf::from(raw_path))
}
