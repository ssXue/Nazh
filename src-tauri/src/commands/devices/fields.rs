//! 设备资产字段修补、信号/告警增删、连接绑定、Pin schema 生成命令。

use nazh_dsl_core::{DeviceSpec, parse_device_yaml, signals_to_pin_definitions};
use tauri::AppHandle;

use crate::asset_files::{
    DeviceSnapshotMeta, SnapshotReason, append_device_snapshot, device_asset_latest_path,
    next_device_asset_version, write_device_asset_yaml,
};

use super::assets::load_device_asset;
use super::types::{FieldSource, PinSchemaEntry};

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

/// 绑定或解绑设备与全局连接的关联。
///
/// 将设备 DSL 的 `connection:` 块写入或清除。
/// - `connection_id` / `connection_type` 均为 `None` 时，清除绑定
/// - 否则写入或更新 `connection` 块
#[tauri::command]
pub(crate) async fn bind_device_connection(
    app: AppHandle,
    asset_id: String,
    connection_type: Option<String>,
    connection_id: Option<String>,
    unit: Option<u8>,
    workspace_path: Option<String>,
) -> Result<(), String> {
    let path = device_asset_latest_path(&app, workspace_path.as_deref(), &asset_id)?;
    if !path.exists() {
        return Err(format!("设备资产 `{asset_id}` 不存在"));
    }
    let yaml_text = tokio::fs::read_to_string(&path)
        .await
        .map_err(|e| format!("读取设备 DSL 失败: {e}"))?;

    let mut spec = parse_device_yaml(&yaml_text).map_err(|e| format!("设备 DSL 解析失败: {e}"))?;

    spec.connection = match (connection_id, connection_type) {
        (Some(id), Some(connection_type)) => Some(nazh_dsl_core::ConnectionRef {
            connection_type,
            id,
            unit,
        }),
        _ => None,
    };

    let new_yaml = serde_yaml::to_string(&spec).map_err(|e| format!("序列化设备 DSL 失败: {e}"))?;

    let version = next_device_asset_version(&app, workspace_path.as_deref(), &asset_id).await?;
    write_device_asset_yaml(
        &app,
        workspace_path.as_deref(),
        &asset_id,
        version,
        new_yaml.trim(),
    )
    .await?;

    let meta = DeviceSnapshotMeta {
        version,
        label: "绑定连接".to_owned(),
        description: String::new(),
        reason: SnapshotReason::Edit,
        created_at: chrono::Utc::now().to_rfc3339(),
    };
    append_device_snapshot(&app, workspace_path.as_deref(), &asset_id, meta).await?;

    Ok(())
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
        .map(crate::asset_files::AssetFieldSource::from)
        .collect::<Vec<_>>();
    crate::asset_files::write_device_asset_sources(
        &app,
        workspace_path.as_deref(),
        &asset_id,
        &converted,
    )
    .await
}

/// 加载设备资产的来源追溯记录。
#[tauri::command]
pub(crate) async fn load_device_asset_sources(
    app: AppHandle,
    asset_id: String,
    workspace_path: Option<String>,
) -> Result<Vec<FieldSource>, String> {
    let sources =
        crate::asset_files::read_device_asset_sources(&app, workspace_path.as_deref(), &asset_id)
            .await?;
    Ok(sources.into_iter().map(FieldSource::from).collect())
}

// ---- 辅助函数 ----

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
