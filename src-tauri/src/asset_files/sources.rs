//! AI 字段来源（`AssetFieldSource`）读写管理。

use std::path::PathBuf;

use tauri::AppHandle;
use tokio::fs;

use super::types::{
    AssetFieldSource, CAPABILITIES_DIR, DEVICES_DIR, DSL_ASSETS_DIR, SOURCES_DIR,
    sanitize_asset_file_stem,
};
use crate::workspace::resolve_project_workspace_dir;

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

/// 删除资产来源文件（由 `io::delete_device_asset_yaml` / `io::delete_capability_asset_yaml` 调用）。
pub(super) async fn delete_asset_sources_file(
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
    super::io::remove_file_if_exists(&path).await
}

// ---- 内部辅助 ----

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
