//! 设备资产 IPC 命令域（RFC-0004 Phase 1 + Phase 4A）。
//!
//! 提供设备资产的 CRUD、AI 抽取、结构化提案和 Pin schema 生成命令。

use nazh_dsl_core::{DeviceSpec, parse_device_yaml, signals_to_pin_definitions};
use serde::{Deserialize, Serialize};
use tauri::AppHandle;

use crate::asset_files::{
    AssetFieldSource, DeviceSnapshotMeta, SnapshotReason, append_device_snapshot,
    delete_device_asset_yaml, delete_device_snapshot_meta, device_asset_latest_path,
    file_modified_at, list_device_asset_version_files, list_device_asset_yaml_files,
    load_device_asset_version_yaml, next_device_asset_version, read_device_asset_sources,
    read_device_snapshots, write_device_asset_sources, write_device_asset_yaml,
};
use crate::ethercat_esi::import_esi_to_device_yaml;
use base64::Engine;

// ---- IPC 响应类型 ----

/// 设备资产摘要（IPC 响应）。
#[derive(Debug, Clone, serde::Serialize)]
pub struct DeviceAssetSummary {
    pub id: String,
    pub name: String,
    pub device_type: String,
    pub version: i64,
    pub updated_at: String,
}

/// 设备资产完整详情（IPC 响应）。
#[derive(Debug, Clone, serde::Serialize)]
pub struct DeviceAssetDetail {
    pub id: String,
    pub name: String,
    pub device_type: String,
    pub version: i64,
    pub spec_json: serde_json::Value,
    pub spec_yaml: String,
    pub yaml_file_path: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

/// Pin schema 条目（IPC 响应）。
#[derive(Debug, Clone, serde::Serialize)]
pub struct PinSchemaEntry {
    pub id: String,
    pub label: String,
    pub pin_type: String,
    pub direction: String,
    pub description: Option<String>,
}

/// 设备资产版本摘要（IPC 响应）。
#[derive(Debug, Clone, serde::Serialize)]
pub struct AssetVersionSummary {
    pub version: i64,
    pub created_at: String,
    pub source_summary: Option<String>,
}

/// 设备资产版本详情（IPC 响应）。
#[derive(Debug, Clone, serde::Serialize)]
pub struct StoredAssetVersion {
    pub asset_id: String,
    pub version: i64,
    pub spec_json: serde_json::Value,
    pub source_summary: Option<String>,
    pub created_at: String,
}

/// AI 来源追溯记录。
#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
pub struct FieldSource {
    pub field_path: String,
    pub source_text: String,
    pub confidence: f64,
}

// ---- CRUD 命令 ----

/// 列出所有设备资产摘要。
#[tauri::command]
pub(crate) async fn list_device_assets(
    app: AppHandle,
    workspace_path: Option<String>,
) -> Result<Vec<DeviceAssetSummary>, String> {
    let files = list_device_asset_yaml_files(&app, workspace_path.as_deref()).await?;
    let mut summaries = Vec::with_capacity(files.len());
    for file in files {
        let detail = load_device_detail_from_path(&app, workspace_path.as_deref(), &file).await?;
        summaries.push(DeviceAssetSummary {
            id: detail.id,
            name: detail.name,
            device_type: detail.device_type,
            version: detail.version,
            updated_at: detail.updated_at,
        });
    }
    summaries.sort_by(|left, right| left.id.cmp(&right.id));
    Ok(summaries)
}

/// 加载指定设备资产详情。
#[tauri::command]
pub(crate) async fn load_device_asset(
    app: AppHandle,
    id: String,
    workspace_path: Option<String>,
) -> Result<Option<DeviceAssetDetail>, String> {
    let path = device_asset_latest_path(&app, workspace_path.as_deref(), &id)?;
    if !path.exists() {
        return Ok(None);
    }
    load_device_detail_from_path(&app, workspace_path.as_deref(), &path)
        .await
        .map(Some)
}

/// 保存（或更新）设备资产。
///
/// 接收 YAML 格式的设备规格，解析校验后存储为 JSON。
/// 可选 `snapshot_label` / `snapshot_reason` 用于记录快照元数据。
#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub(crate) async fn save_device_asset(
    app: AppHandle,
    id: String,
    name: String,
    device_type: String,
    spec_yaml: String,
    workspace_path: Option<String>,
    snapshot_label: Option<String>,
    snapshot_reason: Option<String>,
) -> Result<(), String> {
    // 解析 YAML 校验合法性
    let spec = parse_device_yaml(&spec_yaml).map_err(|e| format!("设备 DSL 解析失败: {e}"))?;
    if spec.id != id {
        return Err(format!(
            "设备资产 ID `{id}` 与 Device DSL id `{}` 不一致",
            spec.id
        ));
    }
    if spec.device_type != device_type {
        return Err(format!(
            "设备类型 `{device_type}` 与 Device DSL type `{}` 不一致",
            spec.device_type
        ));
    }
    let _ = name;
    let version = next_device_asset_version(&app, workspace_path.as_deref(), &id).await?;
    write_device_asset_yaml(
        &app,
        workspace_path.as_deref(),
        &id,
        version,
        spec_yaml.trim(),
    )
    .await?;

    // 记录快照元数据
    let reason_str = snapshot_reason.as_deref().unwrap_or("edit");
    let reason = match reason_str {
        "seed" => SnapshotReason::Seed,
        "manual" => SnapshotReason::Manual,
        "import" => SnapshotReason::Import,
        "rollback" => SnapshotReason::Rollback,
        _ => SnapshotReason::Edit,
    };
    let label = snapshot_label.unwrap_or_else(|| match reason {
        SnapshotReason::Seed => "初始快照".to_owned(),
        SnapshotReason::Import => "导入快照".to_owned(),
        SnapshotReason::Rollback => "回滚前保护".to_owned(),
        SnapshotReason::Edit => format!("编辑快照 v{version}"),
        SnapshotReason::Manual => format!("手动快照 v{version}"),
    });
    let meta = DeviceSnapshotMeta {
        version,
        label,
        description: String::new(),
        reason,
        created_at: chrono::Utc::now().to_rfc3339(),
    };
    append_device_snapshot(&app, workspace_path.as_deref(), &id, meta).await?;

    Ok(())
}

/// 删除设备资产及其版本历史。
#[tauri::command]
pub(crate) async fn delete_device_asset(
    app: AppHandle,
    id: String,
    workspace_path: Option<String>,
) -> Result<(), String> {
    delete_device_asset_yaml(&app, workspace_path.as_deref(), &id).await
}

// ---- 版本管理 ----

/// 列出设备资产的所有版本摘要。
#[tauri::command]
pub(crate) async fn list_asset_versions(
    app: AppHandle,
    asset_id: String,
    workspace_path: Option<String>,
) -> Result<Vec<AssetVersionSummary>, String> {
    let versions =
        list_device_asset_version_files(&app, workspace_path.as_deref(), &asset_id).await?;
    Ok(versions
        .into_iter()
        .map(|version| AssetVersionSummary {
            version: version.version,
            created_at: version.created_at,
            source_summary: None,
        })
        .collect())
}

/// 加载特定版本的设备资产。
#[tauri::command]
pub(crate) async fn load_asset_version(
    app: AppHandle,
    asset_id: String,
    version: i64,
    workspace_path: Option<String>,
) -> Result<Option<StoredAssetVersion>, String> {
    let Some((yaml, created_at)) =
        load_device_asset_version_yaml(&app, workspace_path.as_deref(), &asset_id, version).await?
    else {
        return Ok(None);
    };
    let spec = parse_device_yaml(&yaml).map_err(|e| format!("设备 DSL 解析失败: {e}"))?;
    let spec_json = serde_json::to_value(&spec).map_err(|e| format!("设备规格序列化失败: {e}"))?;
    Ok(Some(StoredAssetVersion {
        asset_id,
        version,
        spec_json,
        source_summary: None,
        created_at,
    }))
}

// ---- 快照管理 ----

/// 快照摘要（IPC 响应）。
#[derive(Debug, Clone, serde::Serialize)]
pub struct DeviceSnapshotSummary {
    pub version: i64,
    pub label: String,
    pub description: String,
    pub reason: String,
    pub created_at: String,
}

/// 列出设备资产的所有快照。
#[tauri::command]
pub(crate) async fn list_device_snapshots(
    app: AppHandle,
    asset_id: String,
    workspace_path: Option<String>,
) -> Result<Vec<DeviceSnapshotSummary>, String> {
    let snapshots = read_device_snapshots(&app, workspace_path.as_deref(), &asset_id).await?;
    Ok(snapshots
        .into_iter()
        .map(|s| DeviceSnapshotSummary {
            version: s.version,
            label: s.label,
            description: s.description,
            reason: match s.reason {
                SnapshotReason::Seed => "seed",
                SnapshotReason::Manual => "manual",
                SnapshotReason::Import => "import",
                SnapshotReason::Edit => "edit",
                SnapshotReason::Rollback => "rollback",
            }
            .to_owned(),
            created_at: s.created_at,
        })
        .collect())
}

/// 创建手动快照（保存当前最新内容为新版本，附带标签）。
#[tauri::command]
pub(crate) async fn create_device_snapshot(
    app: AppHandle,
    asset_id: String,
    label: Option<String>,
    description: Option<String>,
    workspace_path: Option<String>,
) -> Result<(), String> {
    // 读取当前最新内容
    let path = device_asset_latest_path(&app, workspace_path.as_deref(), &asset_id)?;
    if !path.exists() {
        return Err(format!("设备资产 `{asset_id}` 不存在"));
    }
    let yaml = tokio::fs::read_to_string(&path)
        .await
        .map_err(|e| format!("读取设备 DSL 失败: {e}"))?;

    // 保存为新版本
    let version = next_device_asset_version(&app, workspace_path.as_deref(), &asset_id).await?;
    write_device_asset_yaml(
        &app,
        workspace_path.as_deref(),
        &asset_id,
        version,
        yaml.trim(),
    )
    .await?;

    let meta = DeviceSnapshotMeta {
        version,
        label: label.unwrap_or_else(|| format!("手动快照 v{version}")),
        description: description.unwrap_or_default(),
        reason: SnapshotReason::Manual,
        created_at: chrono::Utc::now().to_rfc3339(),
    };
    append_device_snapshot(&app, workspace_path.as_deref(), &asset_id, meta).await
}

/// 回滚到指定快照版本。先保存当前状态为保护快照，再恢复目标版本内容。
#[tauri::command]
pub(crate) async fn rollback_device_snapshot(
    app: AppHandle,
    asset_id: String,
    target_version: i64,
    workspace_path: Option<String>,
) -> Result<(), String> {
    // 读取当前最新
    let latest_path = device_asset_latest_path(&app, workspace_path.as_deref(), &asset_id)?;
    let current_yaml = tokio::fs::read_to_string(&latest_path)
        .await
        .map_err(|e| format!("读取当前设备 DSL 失败: {e}"))?;

    // 保护快照
    let protection_version =
        next_device_asset_version(&app, workspace_path.as_deref(), &asset_id).await?;
    write_device_asset_yaml(
        &app,
        workspace_path.as_deref(),
        &asset_id,
        protection_version,
        current_yaml.trim(),
    )
    .await?;
    let protection_meta = DeviceSnapshotMeta {
        version: protection_version,
        label: format!("回滚前保护（→ v{target_version}）"),
        description: format!("回滚到 v{target_version} 前自动保留的快照。"),
        reason: SnapshotReason::Rollback,
        created_at: chrono::Utc::now().to_rfc3339(),
    };
    append_device_snapshot(&app, workspace_path.as_deref(), &asset_id, protection_meta).await?;

    // 恢复目标版本
    let Some((target_yaml, _)) =
        load_device_asset_version_yaml(&app, workspace_path.as_deref(), &asset_id, target_version)
            .await?
    else {
        return Err(format!("快照 v{target_version} 不存在"));
    };
    let restore_version =
        next_device_asset_version(&app, workspace_path.as_deref(), &asset_id).await?;
    write_device_asset_yaml(
        &app,
        workspace_path.as_deref(),
        &asset_id,
        restore_version,
        target_yaml.trim(),
    )
    .await?;

    let restore_meta = DeviceSnapshotMeta {
        version: restore_version,
        label: format!("回滚到 v{target_version}"),
        description: String::new(),
        reason: SnapshotReason::Rollback,
        created_at: chrono::Utc::now().to_rfc3339(),
    };
    append_device_snapshot(&app, workspace_path.as_deref(), &asset_id, restore_meta).await
}

/// 删除指定快照的元数据（不删除版本文件本身）。
#[tauri::command]
pub(crate) async fn delete_device_snapshot(
    app: AppHandle,
    asset_id: String,
    version: i64,
    workspace_path: Option<String>,
) -> Result<(), String> {
    delete_device_snapshot_meta(&app, workspace_path.as_deref(), &asset_id, version).await
}

// ---- 字段修补 ----

/// 修改设备资产的指定字段（JSON Pointer 路径），解析校验后保存为新版本。
///
/// `json_path` 使用 RFC 6901 JSON Pointer 格式，例如：
/// - `/model` → 顶层 model 字段
/// - `/manufacturer` → 顶层 manufacturer 字段
/// - `/connection/type` → 连接类型
/// - `/signals/0/unit` → 第 0 个信号的 unit
/// - `/signals/0/range/0` → 第 0 个信号的 range.min（数组格式 [min, max]）
/// - `/alarms/0/condition` → 第 0 个告警的 condition
#[tauri::command]
pub(crate) async fn patch_device_field(
    app: AppHandle,
    asset_id: String,
    json_path: String,
    value: String,
    snapshot_label: Option<String>,
    workspace_path: Option<String>,
) -> Result<(), String> {
    // 读取当前最新 YAML
    let path = device_asset_latest_path(&app, workspace_path.as_deref(), &asset_id)?;
    if !path.exists() {
        return Err(format!("设备资产 `{asset_id}` 不存在"));
    }
    let yaml_text = tokio::fs::read_to_string(&path)
        .await
        .map_err(|e| format!("读取设备 DSL 失败: {e}"))?;

    // 解析为 DeviceSpec 再转为 JSON Value
    let spec = parse_device_yaml(&yaml_text).map_err(|e| format!("设备 DSL 解析失败: {e}"))?;
    let mut spec_value =
        serde_json::to_value(&spec).map_err(|e| format!("设备规格序列化失败: {e}"))?;

    // 将字符串 value 转为合适的 JSON 值
    let json_value = parse_patch_value(&value);

    // 用 JSON Pointer 定位并修改
    spec_value
        .pointer_mut(&json_path)
        .ok_or_else(|| format!("路径 `{json_path}` 不存在于设备规格中"))?
        .clone_from(&json_value);

    // 校验修改后的结构仍然合法
    let patched_spec: DeviceSpec =
        serde_json::from_value(spec_value).map_err(|e| format!("修改后的设备规格不合法: {e}"))?;

    // 重序列化为 YAML
    let new_yaml =
        serde_yaml::to_string(&patched_spec).map_err(|e| format!("序列化设备 DSL 失败: {e}"))?;

    // 保存
    let version = next_device_asset_version(&app, workspace_path.as_deref(), &asset_id).await?;
    write_device_asset_yaml(
        &app,
        workspace_path.as_deref(),
        &asset_id,
        version,
        new_yaml.trim(),
    )
    .await?;

    // 记录快照
    let label = snapshot_label.unwrap_or_else(|| format!("编辑 {json_path}"));
    let meta = DeviceSnapshotMeta {
        version,
        label,
        description: String::new(),
        reason: SnapshotReason::Edit,
        created_at: chrono::Utc::now().to_rfc3339(),
    };
    append_device_snapshot(&app, workspace_path.as_deref(), &asset_id, meta).await?;

    Ok(())
}

/// 将前端传入的字符串 value 解析为合适的 JSON 值类型。
fn parse_patch_value(raw: &str) -> serde_json::Value {
    if raw == "null" {
        return serde_json::Value::Null;
    }
    if raw == "true" {
        return serde_json::Value::Bool(true);
    }
    if raw == "false" {
        return serde_json::Value::Bool(false);
    }
    if let Ok(n) = raw.parse::<i64>() {
        return serde_json::Value::Number(n.into());
    }
    if let Ok(n) = raw.parse::<f64>()
        && let Some(v) = serde_json::Number::from_f64(n)
    {
        return serde_json::Value::Number(v);
    }
    serde_json::Value::String(raw.to_owned())
}

// ---- 信号/告警增删 ----

/// 新增信号。`signal_yaml` 为合法的 `SignalSpec` YAML 片段。
#[tauri::command]
pub(crate) async fn add_device_signal(
    app: AppHandle,
    asset_id: String,
    signal_yaml: String,
    workspace_path: Option<String>,
) -> Result<(), String> {
    let signal: nazh_dsl_core::SignalSpec =
        parse_device_yaml_fragment(&signal_yaml).map_err(|e| format!("信号 YAML 解析失败: {e}"))?;

    let mut spec = load_device_spec(&app, workspace_path.as_deref(), &asset_id).await?;
    spec.signals.push(signal);
    save_device_spec(
        &app,
        workspace_path.as_deref(),
        &asset_id,
        &spec,
        "新增信号",
    )
    .await
}

/// 删除指定索引的信号。
#[tauri::command]
pub(crate) async fn remove_device_signal(
    app: AppHandle,
    asset_id: String,
    index: usize,
    workspace_path: Option<String>,
) -> Result<(), String> {
    let mut spec = load_device_spec(&app, workspace_path.as_deref(), &asset_id).await?;
    if index >= spec.signals.len() {
        return Err(format!(
            "信号索引 {index} 越界（共 {} 个信号）",
            spec.signals.len()
        ));
    }
    spec.signals.remove(index);
    save_device_spec(
        &app,
        workspace_path.as_deref(),
        &asset_id,
        &spec,
        "删除信号",
    )
    .await
}

/// 新增告警。`alarm_yaml` 为合法的 `AlarmSpec` YAML 片段。
#[tauri::command]
pub(crate) async fn add_device_alarm(
    app: AppHandle,
    asset_id: String,
    alarm_yaml: String,
    workspace_path: Option<String>,
) -> Result<(), String> {
    let alarm: nazh_dsl_core::AlarmSpec =
        parse_device_yaml_fragment(&alarm_yaml).map_err(|e| format!("告警 YAML 解析失败: {e}"))?;

    let mut spec = load_device_spec(&app, workspace_path.as_deref(), &asset_id).await?;
    spec.alarms.push(alarm);
    save_device_spec(
        &app,
        workspace_path.as_deref(),
        &asset_id,
        &spec,
        "新增告警",
    )
    .await
}

/// 删除指定索引的告警。
#[tauri::command]
pub(crate) async fn remove_device_alarm(
    app: AppHandle,
    asset_id: String,
    index: usize,
    workspace_path: Option<String>,
) -> Result<(), String> {
    let mut spec = load_device_spec(&app, workspace_path.as_deref(), &asset_id).await?;
    if index >= spec.alarms.len() {
        return Err(format!(
            "告警索引 {index} 越界（共 {} 个告警）",
            spec.alarms.len()
        ));
    }
    spec.alarms.remove(index);
    save_device_spec(
        &app,
        workspace_path.as_deref(),
        &asset_id,
        &spec,
        "删除告警",
    )
    .await
}

/// 从 YAML 片段解析为指定类型（通用片段解析器）。
fn parse_device_yaml_fragment<T: serde::de::DeserializeOwned>(yaml: &str) -> Result<T, String> {
    serde_yaml::from_str(yaml).map_err(|e| format!("YAML 片段解析失败: {e}"))
}

/// 加载设备资产的 `DeviceSpec`。
async fn load_device_spec(
    app: &AppHandle,
    workspace_path: Option<&str>,
    asset_id: &str,
) -> Result<DeviceSpec, String> {
    let path = device_asset_latest_path(app, workspace_path, asset_id)?;
    if !path.exists() {
        return Err(format!("设备资产 `{asset_id}` 不存在"));
    }
    let yaml = tokio::fs::read_to_string(&path)
        .await
        .map_err(|e| format!("读取设备 DSL 失败: {e}"))?;
    parse_device_yaml(&yaml).map_err(|e| format!("设备 DSL 解析失败: {e}"))
}

/// 保存 `DeviceSpec` 为新版本快照。
async fn save_device_spec(
    app: &AppHandle,
    workspace_path: Option<&str>,
    asset_id: &str,
    spec: &DeviceSpec,
    snapshot_label: &str,
) -> Result<(), String> {
    let yaml = serde_yaml::to_string(spec).map_err(|e| format!("序列化设备 DSL 失败: {e}"))?;
    let version = next_device_asset_version(app, workspace_path, asset_id).await?;
    write_device_asset_yaml(app, workspace_path, asset_id, version, yaml.trim()).await?;
    let meta = DeviceSnapshotMeta {
        version,
        label: snapshot_label.to_owned(),
        description: String::new(),
        reason: SnapshotReason::Edit,
        created_at: chrono::Utc::now().to_rfc3339(),
    };
    append_device_snapshot(app, workspace_path, asset_id, meta).await
}

// ---- Pin Schema 生成 ----

/// 从设备资产生成 Pin 声明列表。
#[tauri::command]
pub(crate) async fn generate_pin_schema(
    app: AppHandle,
    device_id: String,
    workspace_path: Option<String>,
) -> Result<Vec<PinSchemaEntry>, String> {
    let detail = load_device_asset(app, device_id.clone(), workspace_path)
        .await?
        .ok_or_else(|| format!("设备资产 `{device_id}` 不存在"))?;
    let spec =
        parse_device_yaml(&detail.spec_yaml).map_err(|e| format!("设备 DSL 解析失败: {e}"))?;

    let pin_defs = signals_to_pin_definitions(&spec.signals);
    Ok(pin_defs
        .into_iter()
        .map(|pin| PinSchemaEntry {
            id: pin.id,
            label: pin.label,
            pin_type: format!("{:?}", pin.pin_type).to_lowercase(),
            direction: format!("{:?}", pin.direction).to_lowercase(),
            description: pin.description,
        })
        .collect())
}

// ---- 来源追溯 ----

/// 批量保存 AI 抽取来源追溯记录。
#[tauri::command]
pub(crate) async fn save_device_asset_sources(
    app: AppHandle,
    asset_id: String,
    sources: Vec<FieldSource>,
    workspace_path: Option<String>,
) -> Result<(), String> {
    let converted = sources
        .into_iter()
        .map(AssetFieldSource::from)
        .collect::<Vec<_>>();
    write_device_asset_sources(&app, workspace_path.as_deref(), &asset_id, &converted).await
}

/// 加载设备资产的来源追溯记录。
#[tauri::command]
pub(crate) async fn load_device_asset_sources(
    app: AppHandle,
    asset_id: String,
    workspace_path: Option<String>,
) -> Result<Vec<FieldSource>, String> {
    let sources = read_device_asset_sources(&app, workspace_path.as_deref(), &asset_id).await?;
    Ok(sources.into_iter().map(FieldSource::from).collect())
}

// ---- 结构化提案类型（ESI 导入、PDF 抽取共用） ----

/// AI 抽取的不确定项。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UncertaintyItem {
    pub field_path: String,
    pub guessed_value: String,
    pub reason: String,
}

/// 设备 + 能力的结构化抽取提案（RFC-0004 Phase 4A）。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeviceExtractionProposal {
    pub device_yamls: Vec<String>,
    pub capability_yamls: Vec<String>,
    pub uncertainties: Vec<UncertaintyItem>,
    pub warnings: Vec<String>,
}

// ---- PDF 文本提取 ----

/// 从 PDF 文件（base64 编码）中提取纯文本。
#[tauri::command]
pub(crate) async fn extract_text_from_pdf(pdf_base64: String) -> Result<String, String> {
    let pdf_bytes = base64::engine::general_purpose::STANDARD
        .decode(&pdf_base64)
        .map_err(|e| format!("PDF base64 解码失败: {e}"))?;

    tracing::info!("PDF 文本提取开始，文件大小 {} 字节", pdf_bytes.len());

    let text = pdf_extract::extract_text_from_mem(&pdf_bytes)
        .map_err(|e| format!("PDF 文本提取失败: {e}"))?;

    let trimmed = text.trim();
    if trimmed.is_empty() {
        return Err("PDF 文本提取结果为空，文件可能是扫描件或图片型 PDF".to_owned());
    }

    tracing::info!("PDF 文本提取完成，提取字符数 {}", trimmed.len());

    Ok(trimmed.to_owned())
}

// ---- EtherCAT ESI 导入 ----

/// 从 `EtherCAT` ESI XML 文件导入设备 DSL 草稿。
#[tauri::command]
pub(crate) async fn import_ethercat_esi(
    esi_xml: String,
) -> Result<DeviceExtractionProposal, String> {
    let result = import_esi_to_device_yaml(&esi_xml)?;
    for yaml in &result.device_yamls {
        parse_device_yaml(yaml)
            .map_err(|error| format!("ESI 导入结果不是合法 DeviceSpec: {error}"))?;
    }
    Ok(DeviceExtractionProposal {
        device_yamls: result.device_yamls,
        capability_yamls: Vec::new(),
        uncertainties: Vec::new(),
        warnings: result.warnings,
    })
}

// ---- 辅助函数 ----

async fn load_device_detail_from_path(
    app: &AppHandle,
    workspace_path: Option<&str>,
    path: &std::path::Path,
) -> Result<DeviceAssetDetail, String> {
    let spec_yaml = tokio::fs::read_to_string(path)
        .await
        .map_err(|error| format!("读取设备 DSL 文件失败 `{}`: {error}", path.display()))?;
    let spec = parse_device_yaml(&spec_yaml).map_err(|e| format!("设备 DSL 解析失败: {e}"))?;
    let spec_json = serde_json::to_value(&spec).map_err(|e| format!("设备规格序列化失败: {e}"))?;
    let versions = list_device_asset_version_files(app, workspace_path, &spec.id).await?;
    let version = versions.first().map_or(1, |item| item.version);
    let updated_at = file_modified_at(path).await?;
    let created_at = versions
        .last()
        .map_or_else(|| updated_at.clone(), |item| item.created_at.clone());

    Ok(DeviceAssetDetail {
        id: spec.id.clone(),
        name: spec.model.clone().unwrap_or_else(|| spec.id.clone()),
        device_type: spec.device_type.clone(),
        version,
        spec_json,
        spec_yaml,
        yaml_file_path: Some(path.to_string_lossy().to_string()),
        created_at,
        updated_at,
    })
}

impl From<FieldSource> for AssetFieldSource {
    fn from(value: FieldSource) -> Self {
        Self {
            field_path: value.field_path,
            source_text: value.source_text,
            confidence: value.confidence,
        }
    }
}

impl From<AssetFieldSource> for FieldSource {
    fn from(value: AssetFieldSource) -> Self {
        Self {
            field_path: value.field_path,
            source_text: value.source_text,
            confidence: value.confidence,
        }
    }
}
