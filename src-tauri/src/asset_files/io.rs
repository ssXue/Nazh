//! 资产文件路径解析、读写与删除。

use std::path::{Path, PathBuf};

use tauri::AppHandle;
use tokio::fs;

use super::types::{
    AssetFilePaths, CAPABILITIES_DIR, DEVICES_DIR, DSL_ASSETS_DIR, VERSIONS_DIR,
    sanitize_asset_file_stem,
};
use crate::workspace::resolve_project_workspace_dir;

pub(crate) fn device_asset_file_paths(
    app: &AppHandle,
    workspace_path: Option<&str>,
    asset_id: &str,
    version: i64,
) -> Result<AssetFilePaths, String> {
    let (workspace_dir, _) = resolve_project_workspace_dir(app, workspace_path)?;
    let stem = sanitize_asset_file_stem(asset_id);
    let dir = workspace_dir.join(DSL_ASSETS_DIR).join(DEVICES_DIR);
    Ok(AssetFilePaths {
        latest: dir.join(format!("{stem}.device.yaml")),
        versioned: dir
            .join(VERSIONS_DIR)
            .join(format!("{stem}.v{version}.device.yaml")),
    })
}

pub(crate) fn capability_asset_file_paths(
    app: &AppHandle,
    workspace_path: Option<&str>,
    capability_id: &str,
    version: i64,
) -> Result<AssetFilePaths, String> {
    let (workspace_dir, _) = resolve_project_workspace_dir(app, workspace_path)?;
    let stem = sanitize_asset_file_stem(capability_id);
    let dir = workspace_dir.join(DSL_ASSETS_DIR).join(CAPABILITIES_DIR);
    Ok(AssetFilePaths {
        latest: dir.join(format!("{stem}.capability.yaml")),
        versioned: dir
            .join(VERSIONS_DIR)
            .join(format!("{stem}.v{version}.capability.yaml")),
    })
}

pub(crate) fn device_asset_latest_path(
    app: &AppHandle,
    workspace_path: Option<&str>,
    asset_id: &str,
) -> Result<PathBuf, String> {
    let (workspace_dir, _) = resolve_project_workspace_dir(app, workspace_path)?;
    Ok(workspace_dir
        .join(DSL_ASSETS_DIR)
        .join(DEVICES_DIR)
        .join(format!(
            "{}.device.yaml",
            sanitize_asset_file_stem(asset_id)
        )))
}

pub(crate) fn capability_asset_latest_path(
    app: &AppHandle,
    workspace_path: Option<&str>,
    capability_id: &str,
) -> Result<PathBuf, String> {
    let (workspace_dir, _) = resolve_project_workspace_dir(app, workspace_path)?;
    Ok(workspace_dir
        .join(DSL_ASSETS_DIR)
        .join(CAPABILITIES_DIR)
        .join(format!(
            "{}.capability.yaml",
            sanitize_asset_file_stem(capability_id)
        )))
}

pub(crate) async fn write_device_asset_yaml(
    app: &AppHandle,
    workspace_path: Option<&str>,
    asset_id: &str,
    version: i64,
    yaml_text: &str,
) -> Result<AssetFilePaths, String> {
    let paths = device_asset_file_paths(app, workspace_path, asset_id, version)?;
    write_yaml_mirror(&paths, yaml_text).await?;
    Ok(paths)
}

pub(crate) async fn write_capability_asset_yaml(
    app: &AppHandle,
    workspace_path: Option<&str>,
    capability_id: &str,
    version: i64,
    yaml_text: &str,
) -> Result<AssetFilePaths, String> {
    let paths = capability_asset_file_paths(app, workspace_path, capability_id, version)?;
    write_yaml_mirror(&paths, yaml_text).await?;
    Ok(paths)
}

pub(crate) async fn list_device_asset_yaml_files(
    app: &AppHandle,
    workspace_path: Option<&str>,
) -> Result<Vec<PathBuf>, String> {
    let (workspace_dir, _) = resolve_project_workspace_dir(app, workspace_path)?;
    list_latest_yaml_files(
        &workspace_dir.join(DSL_ASSETS_DIR).join(DEVICES_DIR),
        ".device.yaml",
    )
    .await
}

pub(crate) async fn list_capability_asset_yaml_files(
    app: &AppHandle,
    workspace_path: Option<&str>,
) -> Result<Vec<PathBuf>, String> {
    let (workspace_dir, _) = resolve_project_workspace_dir(app, workspace_path)?;
    list_latest_yaml_files(
        &workspace_dir.join(DSL_ASSETS_DIR).join(CAPABILITIES_DIR),
        ".capability.yaml",
    )
    .await
}

pub(crate) async fn delete_device_asset_yaml(
    app: &AppHandle,
    workspace_path: Option<&str>,
    asset_id: &str,
) -> Result<(), String> {
    let (workspace_dir, _) = resolve_project_workspace_dir(app, workspace_path)?;
    delete_asset_yaml_files(
        &workspace_dir.join(DSL_ASSETS_DIR).join(DEVICES_DIR),
        &sanitize_asset_file_stem(asset_id),
        ".device.yaml",
    )
    .await?;
    super::sources::delete_asset_sources_file(
        app,
        workspace_path,
        asset_id,
        DEVICES_DIR,
        ".sources.yaml",
    )
    .await
}

pub(crate) async fn delete_capability_asset_yaml(
    app: &AppHandle,
    workspace_path: Option<&str>,
    capability_id: &str,
) -> Result<(), String> {
    let (workspace_dir, _) = resolve_project_workspace_dir(app, workspace_path)?;
    delete_asset_yaml_files(
        &workspace_dir.join(DSL_ASSETS_DIR).join(CAPABILITIES_DIR),
        &sanitize_asset_file_stem(capability_id),
        ".capability.yaml",
    )
    .await?;
    super::sources::delete_asset_sources_file(
        app,
        workspace_path,
        capability_id,
        CAPABILITIES_DIR,
        ".sources.yaml",
    )
    .await
}

// ---- 内部辅助 ----

async fn write_yaml_mirror(paths: &AssetFilePaths, yaml_text: &str) -> Result<(), String> {
    if let Some(parent) = paths.latest.parent() {
        fs::create_dir_all(parent)
            .await
            .map_err(|error| format!("创建设备 DSL 目录失败: {error}"))?;
    }
    if let Some(parent) = paths.versioned.parent() {
        fs::create_dir_all(parent)
            .await
            .map_err(|error| format!("创建设备 DSL 版本目录失败: {error}"))?;
    }

    fs::write(&paths.latest, yaml_text).await.map_err(|error| {
        format!(
            "写入 DSL 资产文件失败 `{}`: {error}",
            paths.latest.display()
        )
    })?;
    fs::write(&paths.versioned, yaml_text)
        .await
        .map_err(|error| {
            format!(
                "写入 DSL 资产版本文件失败 `{}`: {error}",
                paths.versioned.display()
            )
        })?;

    Ok(())
}

pub(super) async fn list_latest_yaml_files(
    dir: &Path,
    suffix: &str,
) -> Result<Vec<PathBuf>, String> {
    if !dir.exists() {
        return Ok(Vec::new());
    }

    let mut files = Vec::new();
    let mut entries = fs::read_dir(dir)
        .await
        .map_err(|error| format!("读取 DSL 资产目录失败 `{}`: {error}", dir.display()))?;
    while let Some(entry) = entries
        .next_entry()
        .await
        .map_err(|error| format!("读取 DSL 资产目录条目失败: {error}"))?
    {
        let path = entry.path();
        if path.is_file()
            && path
                .file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|file_name| file_name.ends_with(suffix))
        {
            files.push(path);
        }
    }
    files.sort();
    Ok(files)
}

async fn delete_asset_yaml_files(
    dir: &Path,
    stem: &str,
    latest_suffix: &str,
) -> Result<(), String> {
    let latest = dir.join(format!("{stem}{latest_suffix}"));
    remove_file_if_exists(&latest).await?;

    let versions_dir = dir.join(VERSIONS_DIR);
    if !versions_dir.exists() {
        return Ok(());
    }

    let mut entries = fs::read_dir(&versions_dir).await.map_err(|error| {
        format!(
            "读取 DSL 版本目录失败 `{}`: {error}",
            versions_dir.display()
        )
    })?;
    while let Some(entry) = entries
        .next_entry()
        .await
        .map_err(|error| format!("读取 DSL 版本目录条目失败: {error}"))?
    {
        let path = entry.path();
        let Some(file_name) = path.file_name().and_then(|name| name.to_str()) else {
            continue;
        };
        let prefix = format!("{stem}.v");
        if file_name.starts_with(&prefix) && file_name.ends_with(latest_suffix) {
            remove_file_if_exists(&path).await?;
        }
    }

    Ok(())
}

pub(super) async fn remove_file_if_exists(path: &Path) -> Result<(), String> {
    match fs::remove_file(path).await {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(format!(
            "删除 DSL 资产文件失败 `{}`: {error}",
            path.display()
        )),
    }
}
