//! Capability 资产 IPC 命令域（RFC-0004 Phase 2）。
//!
//! 提供能力资产的 CRUD、自动生成和来源追溯命令。

use nazh_dsl_core::{generate_capabilities_from_device, parse_capability_yaml};
use tauri::AppHandle;

use crate::asset_files::{
    AssetFieldSource, capability_asset_latest_path, delete_capability_asset_yaml, file_modified_at,
    list_capability_asset_version_files, list_capability_asset_yaml_files,
    load_capability_asset_version_yaml, next_capability_asset_version,
    read_capability_asset_sources, write_capability_asset_sources, write_capability_asset_yaml,
};
use crate::commands::devices::load_device_asset;

// ---- IPC 响应类型 ----

/// 能力资产摘要（IPC 响应）。
#[derive(Debug, Clone, serde::Serialize)]
pub struct CapabilitySummary {
    pub id: String,
    pub device_id: String,
    pub name: String,
    pub description: Option<String>,
    pub version: i64,
    pub updated_at: String,
}

/// 能力资产完整详情（IPC 响应）。
#[derive(Debug, Clone, serde::Serialize)]
pub struct CapabilityDetail {
    pub id: String,
    pub device_id: String,
    pub name: String,
    pub description: Option<String>,
    pub version: i64,
    pub spec_json: serde_json::Value,
    pub spec_yaml: String,
    pub yaml_file_path: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

/// 自动生成的能力条目（IPC 响应）。
#[derive(Debug, Clone, serde::Serialize)]
pub struct GeneratedCapability {
    pub capability_yaml: String,
    pub capability_id: String,
}

/// 能力资产版本摘要（IPC 响应）。
#[derive(Debug, Clone, serde::Serialize)]
pub struct CapabilityVersionSummary {
    pub version: i64,
    pub created_at: String,
    pub source_summary: Option<String>,
}

/// 能力资产版本详情（IPC 响应）。
#[derive(Debug, Clone, serde::Serialize)]
pub struct StoredCapabilityVersion {
    pub capability_id: String,
    pub version: i64,
    pub spec_json: serde_json::Value,
    pub source_summary: Option<String>,
    pub created_at: String,
}

/// 能力字段来源追溯记录。
#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
pub struct CapabilitySource {
    pub field_path: String,
    pub source_text: String,
    pub confidence: f64,
}

// ---- CRUD 命令 ----

/// 列出能力资产摘要。`device_id` 为 `Some` 时按设备过滤。
#[tauri::command]
pub(crate) async fn list_capabilities(
    app: AppHandle,
    device_id: Option<String>,
    workspace_path: Option<String>,
) -> Result<Vec<CapabilitySummary>, String> {
    let files = list_capability_asset_yaml_files(&app, workspace_path.as_deref()).await?;
    let mut summaries = Vec::with_capacity(files.len());
    for file in files {
        let detail =
            load_capability_detail_from_path(&app, workspace_path.as_deref(), &file).await?;
        if device_id
            .as_deref()
            .is_some_and(|filter| filter != detail.device_id)
        {
            continue;
        }
        summaries.push(CapabilitySummary {
            id: detail.id,
            device_id: detail.device_id,
            name: detail.name,
            description: detail.description,
            version: detail.version,
            updated_at: detail.updated_at,
        });
    }
    summaries.sort_by(|left, right| left.id.cmp(&right.id));
    Ok(summaries)
}

/// 加载指定能力资产详情。
#[tauri::command]
pub(crate) async fn load_capability(
    app: AppHandle,
    id: String,
    workspace_path: Option<String>,
) -> Result<Option<CapabilityDetail>, String> {
    let path = capability_asset_latest_path(&app, workspace_path.as_deref(), &id)?;
    if !path.exists() {
        return Ok(None);
    }
    load_capability_detail_from_path(&app, workspace_path.as_deref(), &path)
        .await
        .map(Some)
}

/// 保存（或更新）能力资产。
///
/// 接收 YAML 格式的能力规格，解析校验后存储为 JSON。
///
/// 保持 Tauri 扁平参数形状，避免前端调用改成嵌套 payload。
#[allow(clippy::too_many_arguments)]
#[tauri::command]
pub(crate) async fn save_capability(
    app: AppHandle,
    id: String,
    device_id: String,
    name: String,
    description: Option<String>,
    spec_yaml: String,
    workspace_path: Option<String>,
) -> Result<(), String> {
    // 解析 YAML 校验合法性
    let spec = parse_capability_yaml(&spec_yaml).map_err(|e| format!("能力 DSL 解析失败: {e}"))?;
    if spec.id != id {
        return Err(format!(
            "能力资产 ID `{id}` 与 Capability DSL id `{}` 不一致",
            spec.id
        ));
    }
    if spec.device_id != device_id {
        return Err(format!(
            "能力所属设备 `{device_id}` 与 Capability DSL device_id `{}` 不一致",
            spec.device_id
        ));
    }
    let _ = (name, description);
    let version = next_capability_asset_version(&app, workspace_path.as_deref(), &id).await?;
    write_capability_asset_yaml(
        &app,
        workspace_path.as_deref(),
        &id,
        version,
        spec_yaml.trim(),
    )
    .await
    .map(|_| ())
}

/// 删除能力资产及其版本历史。
#[tauri::command]
pub(crate) async fn delete_capability(
    app: AppHandle,
    id: String,
    workspace_path: Option<String>,
) -> Result<(), String> {
    delete_capability_asset_yaml(&app, workspace_path.as_deref(), &id).await
}

// ---- 版本管理 ----

/// 列出能力资产的所有版本摘要。
#[tauri::command]
pub(crate) async fn list_capability_versions(
    app: AppHandle,
    capability_id: String,
    workspace_path: Option<String>,
) -> Result<Vec<CapabilityVersionSummary>, String> {
    let versions =
        list_capability_asset_version_files(&app, workspace_path.as_deref(), &capability_id)
            .await?;
    Ok(versions
        .into_iter()
        .map(|version| CapabilityVersionSummary {
            version: version.version,
            created_at: version.created_at,
            source_summary: None,
        })
        .collect())
}

/// 加载特定版本的能力资产。
#[tauri::command]
pub(crate) async fn load_capability_version(
    app: AppHandle,
    capability_id: String,
    version: i64,
    workspace_path: Option<String>,
) -> Result<Option<StoredCapabilityVersion>, String> {
    let Some((yaml, created_at)) = load_capability_asset_version_yaml(
        &app,
        workspace_path.as_deref(),
        &capability_id,
        version,
    )
    .await?
    else {
        return Ok(None);
    };
    let spec = parse_capability_yaml(&yaml).map_err(|e| format!("能力 DSL 解析失败: {e}"))?;
    let spec_json = serde_json::to_value(&spec).map_err(|e| format!("能力规格序列化失败: {e}"))?;
    Ok(Some(StoredCapabilityVersion {
        capability_id,
        version,
        spec_json,
        source_summary: None,
        created_at,
    }))
}

// ---- 自动生成 ----

/// 从设备资产的写信号自动生成能力列表。
///
/// 返回每个生成的 `CapabilitySpec` 的 YAML 文本和 ID，
/// 前端可展示后选择保存。
#[tauri::command]
pub(crate) async fn generate_capabilities_from_device_cmd(
    app: AppHandle,
    device_id: String,
    workspace_path: Option<String>,
) -> Result<Vec<GeneratedCapability>, String> {
    let asset = load_device_asset(app, device_id.clone(), workspace_path)
        .await?
        .ok_or_else(|| format!("设备资产 `{device_id}` 不存在"))?;
    let spec = nazh_dsl_core::parse_device_yaml(&asset.spec_yaml)
        .map_err(|e| format!("设备 DSL 解析失败: {e}"))?;

    let caps = generate_capabilities_from_device(&spec);
    Ok(caps
        .into_iter()
        .map(|cap| {
            let cap_id = cap.id.clone();
            let yaml = serde_yaml::to_string(&cap).unwrap_or_default();
            GeneratedCapability {
                capability_yaml: yaml,
                capability_id: cap_id,
            }
        })
        .collect())
}

// ---- 来源追溯 ----

/// 批量保存能力来源追溯记录。
#[tauri::command]
pub(crate) async fn save_capability_sources(
    app: AppHandle,
    capability_id: String,
    sources: Vec<CapabilitySource>,
    workspace_path: Option<String>,
) -> Result<(), String> {
    let converted = sources
        .into_iter()
        .map(AssetFieldSource::from)
        .collect::<Vec<_>>();
    write_capability_asset_sources(&app, workspace_path.as_deref(), &capability_id, &converted)
        .await
}

/// 加载能力资产的来源追溯记录。
#[tauri::command]
pub(crate) async fn load_capability_sources(
    app: AppHandle,
    capability_id: String,
    workspace_path: Option<String>,
) -> Result<Vec<CapabilitySource>, String> {
    let sources =
        read_capability_asset_sources(&app, workspace_path.as_deref(), &capability_id).await?;
    Ok(sources.into_iter().map(CapabilitySource::from).collect())
}

// ---- 辅助函数 ----

async fn load_capability_detail_from_path(
    app: &AppHandle,
    workspace_path: Option<&str>,
    path: &std::path::Path,
) -> Result<CapabilityDetail, String> {
    let spec_yaml = tokio::fs::read_to_string(path)
        .await
        .map_err(|error| format!("读取能力 DSL 文件失败 `{}`: {error}", path.display()))?;
    let spec = parse_capability_yaml(&spec_yaml).map_err(|e| format!("能力 DSL 解析失败: {e}"))?;
    let spec_json = serde_json::to_value(&spec).map_err(|e| format!("能力规格序列化失败: {e}"))?;
    let versions = list_capability_asset_version_files(app, workspace_path, &spec.id).await?;
    let version = versions.first().map_or(1, |item| item.version);
    let updated_at = file_modified_at(path).await?;
    let created_at = versions
        .last()
        .map_or_else(|| updated_at.clone(), |item| item.created_at.clone());
    let description = if spec.description.trim().is_empty() {
        None
    } else {
        Some(spec.description.clone())
    };

    Ok(CapabilityDetail {
        id: spec.id.clone(),
        device_id: spec.device_id.clone(),
        name: spec.id.rsplit('.').next().unwrap_or(&spec.id).to_owned(),
        description,
        version,
        spec_json,
        spec_yaml,
        yaml_file_path: Some(path.to_string_lossy().to_string()),
        created_at,
        updated_at,
    })
}

impl From<CapabilitySource> for AssetFieldSource {
    fn from(value: CapabilitySource) -> Self {
        Self {
            field_path: value.field_path,
            source_text: value.source_text,
            confidence: value.confidence,
        }
    }
}

impl From<AssetFieldSource> for CapabilitySource {
    fn from(value: AssetFieldSource) -> Self {
        Self {
            field_path: value.field_path,
            source_text: value.source_text,
            confidence: value.confidence,
        }
    }
}
