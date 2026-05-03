//! Capability 资产 IPC 命令域（RFC-0004 Phase 2）。
//!
//! 提供能力资产的 CRUD、自动生成和来源追溯命令。

use std::sync::Arc;

use nazh_dsl_core::{generate_capabilities_from_device, parse_capability_yaml};
use store::{
    CapabilitySource, CapabilitySummary, CapabilityVersionSummary, Store, StoredCapability,
    StoredCapabilityVersion,
};
use tauri::State;

use crate::state::DesktopState;

// ---- IPC 响应类型 ----

/// 能力资产完整详情（IPC 响应）。
#[derive(Debug, Clone, serde::Serialize)]
pub struct CapabilityDetail {
    pub id: String,
    pub device_id: String,
    pub name: String,
    pub description: Option<String>,
    pub version: i64,
    pub spec_json: serde_json::Value,
    pub created_at: String,
    pub updated_at: String,
}

impl From<StoredCapability> for CapabilityDetail {
    fn from(value: StoredCapability) -> Self {
        Self {
            id: value.id,
            device_id: value.device_id,
            name: value.name,
            description: value.description,
            version: value.version,
            spec_json: value.spec_json,
            created_at: value.created_at,
            updated_at: value.updated_at,
        }
    }
}

/// 自动生成的能力条目（IPC 响应）。
#[derive(Debug, Clone, serde::Serialize)]
pub struct GeneratedCapability {
    pub capability_yaml: String,
    pub capability_id: String,
}

// ---- CRUD 命令 ----

/// 列出能力资产摘要。`device_id` 为 `Some` 时按设备过滤。
#[tauri::command]
pub(crate) async fn list_capabilities(
    state: State<'_, DesktopState>,
    device_id: Option<String>,
) -> Result<Vec<CapabilitySummary>, String> {
    let store = get_store(&state)?;
    store
        .list_capabilities(device_id.as_deref())
        .map_err(|e| format!("列出能力资产失败: {e}"))
}

/// 加载指定能力资产详情。
#[tauri::command]
pub(crate) async fn load_capability(
    state: State<'_, DesktopState>,
    id: String,
) -> Result<Option<CapabilityDetail>, String> {
    let store = get_store(&state)?;
    store
        .load_capability(&id)
        .map_err(|e| format!("加载能力资产失败: {e}"))
        .map(|opt| opt.map(CapabilityDetail::from))
}

/// 保存（或更新）能力资产。
///
/// 接收 YAML 格式的能力规格，解析校验后存储为 JSON。
#[tauri::command]
pub(crate) async fn save_capability(
    state: State<'_, DesktopState>,
    id: String,
    device_id: String,
    name: String,
    description: Option<String>,
    spec_yaml: String,
) -> Result<(), String> {
    // 解析 YAML 校验合法性
    let spec = parse_capability_yaml(&spec_yaml).map_err(|e| format!("能力 DSL 解析失败: {e}"))?;
    let spec_json = serde_json::to_value(&spec).map_err(|e| format!("能力规格序列化失败: {e}"))?;

    let store = get_store(&state)?;
    store
        .save_capability(&id, &device_id, &name, description.as_deref(), &spec_json)
        .map_err(|e| format!("保存能力资产失败: {e}"))
}

/// 删除能力资产及其版本历史。
#[tauri::command]
pub(crate) async fn delete_capability(
    state: State<'_, DesktopState>,
    id: String,
) -> Result<(), String> {
    let store = get_store(&state)?;
    store
        .delete_capability(&id)
        .map_err(|e| format!("删除能力资产失败: {e}"))
}

// ---- 版本管理 ----

/// 列出能力资产的所有版本摘要。
#[tauri::command]
pub(crate) async fn list_capability_versions(
    state: State<'_, DesktopState>,
    capability_id: String,
) -> Result<Vec<CapabilityVersionSummary>, String> {
    let store = get_store(&state)?;
    store
        .list_capability_versions(&capability_id)
        .map_err(|e| format!("列出能力版本失败: {e}"))
}

/// 加载特定版本的能力资产。
#[tauri::command]
pub(crate) async fn load_capability_version(
    state: State<'_, DesktopState>,
    capability_id: String,
    version: i64,
) -> Result<Option<StoredCapabilityVersion>, String> {
    let store = get_store(&state)?;
    store
        .load_capability_version(&capability_id, version)
        .map_err(|e| format!("加载能力版本失败: {e}"))
}

// ---- 自动生成 ----

/// 从设备资产的写信号自动生成能力列表。
///
/// 返回每个生成的 `CapabilitySpec` 的 YAML 文本和 ID，
/// 前端可展示后选择保存。
#[tauri::command]
pub(crate) async fn generate_capabilities_from_device_cmd(
    state: State<'_, DesktopState>,
    device_id: String,
) -> Result<Vec<GeneratedCapability>, String> {
    let store = get_store(&state)?;
    let asset = store
        .load_device_asset(&device_id)
        .map_err(|e| format!("加载设备资产失败: {e}"))?
        .ok_or_else(|| format!("设备资产 `{device_id}` 不存在"))?;

    let spec: nazh_dsl_core::DeviceSpec = serde_json::from_value(asset.spec_json)
        .map_err(|e| format!("设备规格反序列化失败: {e}"))?;

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
    state: State<'_, DesktopState>,
    capability_id: String,
    sources: Vec<CapabilitySource>,
) -> Result<(), String> {
    let store = get_store(&state)?;
    store
        .save_capability_sources(&capability_id, &sources)
        .map_err(|e| format!("保存来源记录失败: {e}"))
}

/// 加载能力资产的来源追溯记录。
#[tauri::command]
pub(crate) async fn load_capability_sources(
    state: State<'_, DesktopState>,
    capability_id: String,
) -> Result<Vec<CapabilitySource>, String> {
    let store = get_store(&state)?;
    store
        .load_capability_sources(&capability_id)
        .map_err(|e| format!("加载来源记录失败: {e}"))
}

// ---- 辅助函数 ----

/// 获取 Store 的 Arc 引用。
fn get_store(state: &State<'_, DesktopState>) -> Result<Arc<Store>, String> {
    state
        .store
        .read()
        .map(|guard| Arc::clone(&guard))
        .map_err(|e| format!("Store 锁异常: {e}"))
}
