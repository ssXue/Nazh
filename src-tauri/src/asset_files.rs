//! 工程工作路径下的 DSL 资产文件存储。
//!
//! Device / Capability 资产以 `YAML` 文件为唯一持久化真值源。
//! `SQLite` 仅服务变量、历史和全局变量，不再保存设备/能力资产索引。

use std::{
    path::{Path, PathBuf},
    time::SystemTime,
};

use chrono::{DateTime, Utc};
use tauri::AppHandle;
use tokio::fs;

use crate::workspace::resolve_project_workspace_dir;

const DSL_ASSETS_DIR: &str = "dsl";
const DEVICES_DIR: &str = "devices";
const CAPABILITIES_DIR: &str = "capabilities";
const VERSIONS_DIR: &str = "versions";
const SOURCES_DIR: &str = "sources";

pub(crate) struct AssetFilePaths {
    pub(crate) latest: PathBuf,
    pub(crate) versioned: PathBuf,
}

#[derive(Debug, Clone)]
pub(crate) struct AssetVersionFile {
    pub(crate) version: i64,
    pub(crate) created_at: String,
}

#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
pub(crate) struct AssetFieldSource {
    pub(crate) field_path: String,
    pub(crate) source_text: String,
    pub(crate) confidence: f64,
}

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

pub(crate) async fn next_device_asset_version(
    app: &AppHandle,
    workspace_path: Option<&str>,
    asset_id: &str,
) -> Result<i64, String> {
    next_asset_version(app, workspace_path, asset_id, DEVICES_DIR, ".device.yaml").await
}

pub(crate) async fn next_capability_asset_version(
    app: &AppHandle,
    workspace_path: Option<&str>,
    capability_id: &str,
) -> Result<i64, String> {
    next_asset_version(
        app,
        workspace_path,
        capability_id,
        CAPABILITIES_DIR,
        ".capability.yaml",
    )
    .await
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

pub(crate) async fn list_device_asset_version_files(
    app: &AppHandle,
    workspace_path: Option<&str>,
    asset_id: &str,
) -> Result<Vec<AssetVersionFile>, String> {
    let (workspace_dir, _) = resolve_project_workspace_dir(app, workspace_path)?;
    list_version_files(
        &workspace_dir
            .join(DSL_ASSETS_DIR)
            .join(DEVICES_DIR)
            .join(VERSIONS_DIR),
        &sanitize_asset_file_stem(asset_id),
        ".device.yaml",
    )
    .await
}

pub(crate) async fn list_capability_asset_version_files(
    app: &AppHandle,
    workspace_path: Option<&str>,
    capability_id: &str,
) -> Result<Vec<AssetVersionFile>, String> {
    let (workspace_dir, _) = resolve_project_workspace_dir(app, workspace_path)?;
    list_version_files(
        &workspace_dir
            .join(DSL_ASSETS_DIR)
            .join(CAPABILITIES_DIR)
            .join(VERSIONS_DIR),
        &sanitize_asset_file_stem(capability_id),
        ".capability.yaml",
    )
    .await
}

pub(crate) async fn load_device_asset_version_yaml(
    app: &AppHandle,
    workspace_path: Option<&str>,
    asset_id: &str,
    version: i64,
) -> Result<Option<(String, String)>, String> {
    load_version_yaml(
        app,
        workspace_path,
        asset_id,
        version,
        DEVICES_DIR,
        ".device.yaml",
    )
    .await
}

pub(crate) async fn load_capability_asset_version_yaml(
    app: &AppHandle,
    workspace_path: Option<&str>,
    capability_id: &str,
    version: i64,
) -> Result<Option<(String, String)>, String> {
    load_version_yaml(
        app,
        workspace_path,
        capability_id,
        version,
        CAPABILITIES_DIR,
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
    delete_asset_sources_file(app, workspace_path, asset_id, DEVICES_DIR, ".sources.yaml").await
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
    delete_asset_sources_file(
        app,
        workspace_path,
        capability_id,
        CAPABILITIES_DIR,
        ".sources.yaml",
    )
    .await
}

pub(crate) async fn write_device_asset_sources(
    app: &AppHandle,
    workspace_path: Option<&str>,
    asset_id: &str,
    sources: &[AssetFieldSource],
) -> Result<(), String> {
    write_asset_sources(app, workspace_path, asset_id, DEVICES_DIR, sources).await
}

pub(crate) async fn read_device_asset_sources(
    app: &AppHandle,
    workspace_path: Option<&str>,
    asset_id: &str,
) -> Result<Vec<AssetFieldSource>, String> {
    read_asset_sources(app, workspace_path, asset_id, DEVICES_DIR).await
}

pub(crate) async fn write_capability_asset_sources(
    app: &AppHandle,
    workspace_path: Option<&str>,
    capability_id: &str,
    sources: &[AssetFieldSource],
) -> Result<(), String> {
    write_asset_sources(
        app,
        workspace_path,
        capability_id,
        CAPABILITIES_DIR,
        sources,
    )
    .await
}

pub(crate) async fn read_capability_asset_sources(
    app: &AppHandle,
    workspace_path: Option<&str>,
    capability_id: &str,
) -> Result<Vec<AssetFieldSource>, String> {
    read_asset_sources(app, workspace_path, capability_id, CAPABILITIES_DIR).await
}

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

async fn next_asset_version(
    app: &AppHandle,
    workspace_path: Option<&str>,
    asset_id: &str,
    asset_dir: &str,
    latest_suffix: &str,
) -> Result<i64, String> {
    let (workspace_dir, _) = resolve_project_workspace_dir(app, workspace_path)?;
    let versions = list_version_files(
        &workspace_dir
            .join(DSL_ASSETS_DIR)
            .join(asset_dir)
            .join(VERSIONS_DIR),
        &sanitize_asset_file_stem(asset_id),
        latest_suffix,
    )
    .await?;
    Ok(versions
        .first()
        .map_or(1, |version| version.version.saturating_add(1)))
}

async fn list_latest_yaml_files(dir: &Path, suffix: &str) -> Result<Vec<PathBuf>, String> {
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

async fn list_version_files(
    versions_dir: &Path,
    stem: &str,
    latest_suffix: &str,
) -> Result<Vec<AssetVersionFile>, String> {
    if !versions_dir.exists() {
        return Ok(Vec::new());
    }

    let mut versions = Vec::new();
    let mut entries = fs::read_dir(versions_dir).await.map_err(|error| {
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
        if !file_name.starts_with(&prefix) || !file_name.ends_with(latest_suffix) {
            continue;
        }
        let version_text = &file_name[prefix.len()..file_name.len() - latest_suffix.len()];
        let Ok(version) = version_text.parse::<i64>() else {
            continue;
        };
        let created_at = file_modified_at(&path).await?;
        versions.push(AssetVersionFile {
            version,
            created_at,
        });
    }
    versions.sort_by_key(|version| std::cmp::Reverse(version.version));
    Ok(versions)
}

async fn load_version_yaml(
    app: &AppHandle,
    workspace_path: Option<&str>,
    asset_id: &str,
    version: i64,
    asset_dir: &str,
    latest_suffix: &str,
) -> Result<Option<(String, String)>, String> {
    let (workspace_dir, _) = resolve_project_workspace_dir(app, workspace_path)?;
    let path = workspace_dir
        .join(DSL_ASSETS_DIR)
        .join(asset_dir)
        .join(VERSIONS_DIR)
        .join(format!(
            "{}.v{version}{latest_suffix}",
            sanitize_asset_file_stem(asset_id)
        ));
    match fs::read_to_string(&path).await {
        Ok(text) => {
            let created_at = file_modified_at(&path).await?;
            Ok(Some((text, created_at)))
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(format!(
            "读取 DSL 资产版本文件失败 `{}`: {error}",
            path.display()
        )),
    }
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

async fn write_asset_sources(
    app: &AppHandle,
    workspace_path: Option<&str>,
    asset_id: &str,
    asset_dir: &str,
    sources: &[AssetFieldSource],
) -> Result<(), String> {
    let path = asset_sources_path(app, workspace_path, asset_id, asset_dir)?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .await
            .map_err(|error| format!("创建 DSL 来源目录失败: {error}"))?;
    }
    let text =
        serde_yaml::to_string(sources).map_err(|error| format!("序列化来源 YAML 失败: {error}"))?;
    fs::write(&path, text)
        .await
        .map_err(|error| format!("写入 DSL 来源文件失败 `{}`: {error}", path.display()))
}

async fn read_asset_sources(
    app: &AppHandle,
    workspace_path: Option<&str>,
    asset_id: &str,
    asset_dir: &str,
) -> Result<Vec<AssetFieldSource>, String> {
    let path = asset_sources_path(app, workspace_path, asset_id, asset_dir)?;
    match fs::read_to_string(&path).await {
        Ok(text) => serde_yaml::from_str::<Vec<AssetFieldSource>>(&text)
            .map_err(|error| format!("解析 DSL 来源文件失败 `{}`: {error}", path.display())),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(Vec::new()),
        Err(error) => Err(format!(
            "读取 DSL 来源文件失败 `{}`: {error}",
            path.display()
        )),
    }
}

async fn delete_asset_sources_file(
    app: &AppHandle,
    workspace_path: Option<&str>,
    asset_id: &str,
    asset_dir: &str,
    suffix: &str,
) -> Result<(), String> {
    let (workspace_dir, _) = resolve_project_workspace_dir(app, workspace_path)?;
    let path = workspace_dir
        .join(DSL_ASSETS_DIR)
        .join(asset_dir)
        .join(SOURCES_DIR)
        .join(format!("{}{}", sanitize_asset_file_stem(asset_id), suffix));
    remove_file_if_exists(&path).await
}

fn asset_sources_path(
    app: &AppHandle,
    workspace_path: Option<&str>,
    asset_id: &str,
    asset_dir: &str,
) -> Result<PathBuf, String> {
    let (workspace_dir, _) = resolve_project_workspace_dir(app, workspace_path)?;
    Ok(workspace_dir
        .join(DSL_ASSETS_DIR)
        .join(asset_dir)
        .join(SOURCES_DIR)
        .join(format!(
            "{}.sources.yaml",
            sanitize_asset_file_stem(asset_id)
        )))
}

async fn remove_file_if_exists(path: &Path) -> Result<(), String> {
    match fs::remove_file(path).await {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(format!(
            "删除 DSL 资产文件失败 `{}`: {error}",
            path.display()
        )),
    }
}

pub(crate) async fn file_modified_at(path: &Path) -> Result<String, String> {
    let modified = fs::metadata(path)
        .await
        .map_err(|error| format!("读取 DSL 资产文件元数据失败 `{}`: {error}", path.display()))?
        .modified()
        .unwrap_or_else(|_| SystemTime::now());
    let dt: DateTime<Utc> = modified.into();
    Ok(dt.to_rfc3339())
}

fn sanitize_asset_file_stem(raw: &str) -> String {
    let stem = raw
        .trim()
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || matches!(ch, '_' | '-' | '.') {
                ch
            } else {
                '_'
            }
        })
        .collect::<String>();
    let stem = stem
        .trim_matches(|ch| matches!(ch, '_' | '-' | '.'))
        .to_owned();
    if stem.is_empty() {
        "asset".to_owned()
    } else {
        stem
    }
}

#[cfg(test)]
mod tests {
    use super::sanitize_asset_file_stem;

    #[test]
    fn 资产文件名只保留安全字符() {
        assert_eq!(sanitize_asset_file_stem(" press/轴 1 "), "press___1");
        assert_eq!(sanitize_asset_file_stem("../"), "asset");
    }
}
