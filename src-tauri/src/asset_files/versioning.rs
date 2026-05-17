//! 资产版本管理：版本号递增、版本列表、版本内容加载。

use std::path::Path;

use tauri::AppHandle;
use tokio::fs;

use super::types::{
    AssetVersionFile, CAPABILITIES_DIR, CONNECTIONS_DIR, DEVICES_DIR, DSL_ASSETS_DIR, VERSIONS_DIR,
    file_modified_at, sanitize_asset_file_stem,
};
use crate::workspace::resolve_project_workspace_dir;

pub(crate) async fn next_device_asset_version(
    app: &AppHandle,
    workspace_path: Option<&str>,
    asset_id: &str,
) -> Result<i64, String> {
    next_asset_version(app, workspace_path, asset_id, DEVICES_DIR, ".device.yaml").await
}

pub(crate) async fn next_connection_asset_version(
    app: &AppHandle,
    workspace_path: Option<&str>,
    connection_id: &str,
) -> Result<i64, String> {
    next_asset_version(
        app,
        workspace_path,
        connection_id,
        CONNECTIONS_DIR,
        ".connection.yaml",
    )
    .await
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

// ---- 内部辅助 ----

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
