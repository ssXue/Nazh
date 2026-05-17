use ai::{AiConfigUpdate, AiConfigView};
use tauri::{AppHandle, State};

use crate::commands::{
    capabilities::{list_capabilities, load_capability},
    devices::assets::{list_device_assets, load_device_asset},
};
use crate::state::DesktopState;

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AiDeviceAssetContext {
    pub(crate) id: String,
    pub(crate) name: String,
    pub(crate) device_type: String,
    pub(crate) version: i64,
    pub(crate) yaml: String,
    pub(crate) yaml_file_path: Option<String>,
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AiCapabilityAssetContext {
    pub(crate) id: String,
    pub(crate) device_id: String,
    pub(crate) name: String,
    pub(crate) description: Option<String>,
    pub(crate) version: i64,
    pub(crate) yaml: String,
    pub(crate) yaml_file_path: Option<String>,
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AiAssetContext {
    pub(crate) devices: Vec<AiDeviceAssetContext>,
    pub(crate) capabilities: Vec<AiCapabilityAssetContext>,
}

#[tauri::command]
pub(crate) async fn load_ai_config(state: State<'_, DesktopState>) -> Result<AiConfigView, String> {
    let config = state.ai_config.read().await;
    Ok(config.to_view())
}

/// 读取指定 provider 的 API key，供前端直调 AI provider 时按需使用。
///
/// 密钥仅在调用时短暂暴露到 webview 进程内存，前端应立即用于构造
/// provider 实例，不缓存到全局变量或 store。
#[tauri::command]
pub(crate) async fn load_ai_api_key(
    provider_id: String,
    state: State<'_, DesktopState>,
) -> Result<String, String> {
    let config = state.ai_config.read().await;
    Ok(config
        .api_key_for_provider(&provider_id)
        .map_err(|error| error.to_string())?
        .to_owned())
}

#[tauri::command]
pub(crate) async fn save_ai_config(
    state: State<'_, DesktopState>,
    update: AiConfigUpdate,
) -> Result<AiConfigView, String> {
    let store = state.store_handle()?;
    let mut config = state.ai_config.write().await;
    config.merge_update(update);

    let text = serde_json::to_string_pretty(&*config)
        .map_err(|error| format!("序列化 AI 配置失败: {error}"))?;
    store
        .save_ai_config(text)
        .await
        .map_err(|error| format!("保存 AI 配置到 Store 失败: {error}"))?;

    Ok(config.to_view())
}

/// 读取 AI 编辑链路可见的已审查设备/能力资产上下文。
#[tauri::command]
pub(crate) async fn load_ai_asset_context(
    app: AppHandle,
    workspace_path: Option<String>,
) -> Result<AiAssetContext, String> {
    let device_summaries = list_device_assets(app.clone(), workspace_path.clone()).await?;
    let capability_summaries = list_capabilities(app.clone(), None, workspace_path.clone()).await?;

    let mut devices = Vec::with_capacity(device_summaries.len());
    for summary in device_summaries {
        if let Some(asset) =
            load_device_asset(app.clone(), summary.id.clone(), workspace_path.clone()).await?
        {
            devices.push(AiDeviceAssetContext {
                id: asset.id,
                name: asset.name,
                device_type: asset.device_type,
                version: asset.version,
                yaml: asset.spec_yaml,
                yaml_file_path: asset.yaml_file_path,
            });
        }
    }

    let mut capabilities = Vec::with_capacity(capability_summaries.len());
    for summary in capability_summaries {
        if let Some(capability) =
            load_capability(app.clone(), summary.id.clone(), workspace_path.clone()).await?
        {
            capabilities.push(AiCapabilityAssetContext {
                id: capability.id,
                device_id: capability.device_id,
                name: capability.name,
                description: capability.description,
                version: capability.version,
                yaml: capability.spec_yaml,
                yaml_file_path: capability.yaml_file_path,
            });
        }
    }

    Ok(AiAssetContext {
        devices,
        capabilities,
    })
}
